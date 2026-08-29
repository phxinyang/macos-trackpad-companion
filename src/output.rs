//! macOS event synthesis. Public CGEvent APIs handle cursor, click, and
//! phased smooth scroll. The private path (gated per-gesture by
//! [`Config::pinch`] / [`Config::rotate`] / the swipe backend selectors)
//! injects pinch, rotate, and swipe via undocumented CGEvent type/field
//! IDs that BetterTouchTool, Karabiner-Elements, and similar tools have
//! used for years — stable across recent macOS versions but not in any
//! public Apple header.
//!
//! All field IDs and event-type constants used here are reverse-engineered
//! from NSEvent type values; see the comments next to each declaration.

#![allow(non_upper_case_globals)]

use core_foundation::base::TCFType;
use core_foundation::date::CFAbsoluteTimeGetCurrent;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
use core_foundation_sys::runloop::{
    CFRunLoopAddTimer, CFRunLoopTimerContext, CFRunLoopTimerCreate, CFRunLoopTimerInvalidate,
    CFRunLoopTimerRef,
};
use core_graphics::display::CGDisplay;
use core_graphics::geometry::{CGPoint, CGRect};
use objc2_app_kit::{
    NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime,
    NSHapticFeedbackPerformer,
};
use std::cell::Cell;
use std::ffi::c_void;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use crate::time::Timestamp;

/// Hardcoded macOS double-click interval. Configurable in System
/// Settings (Mouse → Double-Click Speed) but rarely changed; querying
/// `CGEventSourceGetDoubleClickInterval` on every click is overkill,
/// and the default works for almost everyone. Promote to a CLI flag
/// if a user needs to tune it.
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);
/// Maximum cursor displacement (pixels) between successive clicks for
/// them to count as a multi-click sequence. Expanded to 25.0 px to
/// accommodate natural touchscreen finger touchdown drift on mobile trackpads.
const DOUBLE_CLICK_DISTANCE_PX: f64 = 25.0;

/// Inertia / coast tunables.
///
/// Modeled on rmk's `TrackpadProcessor`: an EMA-smoothed velocity sampled
/// during active scroll seeds an exponential decay after the user lifts.
/// The values aren't a direct port — rmk works in chip units per chip
/// cycle, and we work in mm/s with a wall-clock timer — but the wall-clock
/// half-life and stop time roughly match.
///
/// `MOMENTUM_TICK_HZ` drives the CFRunLoopTimer that posts momentum-phase
/// scroll events while coasting. 60 Hz is the natural cadence for
/// momentum-aware UIs and keeps each event small enough to feel smooth.
const MOMENTUM_TICK_HZ: f64 = 60.0;
const MOMENTUM_TICK_INTERVAL: f64 = 1.0 / MOMENTUM_TICK_HZ;
/// Per-second velocity decay multiplier during coast. 0.05 means velocity
/// drops to 5% of its lift value over one second — wall-clock half-life
/// of ~230 ms, full-stop within ~1 s for a typical flick. Tweak by ear.
const MOMENTUM_DECAY_PER_SEC: f64 = 0.05;
/// Speed below which momentum stops emitting (mm/s). Anything smaller
/// would round to under a pixel per tick at the default scroll_accel
/// and just look like jitter trailing off.
const MOMENTUM_STOP_MM_PER_SEC: f64 = 5.0;
/// Speed required at scroll-end to seed inertia (mm/s). Avoids "ghost"
/// coasts from a slow drag that the user wasn't trying to fling.
const MOMENTUM_SEED_MM_PER_SEC: f64 = 25.0;

/// Power-curve acceleration for scroll, modeled on `smooth_scroll.swift`'s
/// `acceleratePixels`. Real trackpads (and LinearMouse) expose this kind
/// of curve because uniform `pixels = mm × accel` feels
/// too linear: slow scrolls overshoot if accel is high enough for flicks
/// to feel responsive, and flicks feel sluggish if accel is low enough
/// for slow scrolls to stay precise. The exponent boosts faster motion
/// disproportionately, giving Apple-style "high initial velocity, clear
/// deceleration" feel without sacrificing slow-scroll precision.
///
/// Curve: `pixels_per_sec = sign(v) × LINEAR × |v|^EXPONENT`, calibrated
/// so the curve passes through the linear value `scroll_accel × v` at
/// `v == REF_MM_PER_SEC` (i.e. crosses the linear feel at "typical"
/// scroll speed). Below the reference, curve is sub-linear → slow
/// motion is slower than linear. Above it, super-linear → fast motion
/// gets amplified.
const SCROLL_CURVE_EXPONENT: f64 = 1.3;
/// Reference velocity (mm/s). At this velocity, the curve's pixel rate
/// equals `scroll_accel × velocity`; ~1 mm per chip frame on a 60 Hz
/// pad, which feels "typical" to the user during deliberate panning.
const SCROLL_CURVE_REF_MM_PER_SEC: f64 = 60.0;

/// Bounds of the display containing `point`, falling back to the main
/// display if the point isn't on any (e.g. just past a screen edge —
/// which is exactly the case we're trying to clamp against). Used by
/// `move_cursor_by` to keep posted event locations on-screen.
fn display_bounds_for(point: CGPoint) -> CGRect {
    if let Ok((ids, _)) = CGDisplay::displays_with_point(point, 1) {
        if let Some(&id) = ids.first() {
            return CGDisplay::new(id).bounds();
        }
    }
    CGDisplay::main().bounds()
}

/// Apply the scroll-acceleration curve to a velocity, returning pixels
/// per second. Caller multiplies by per-tick `dt` for the per-tick
/// pixel delta.
fn accelerate_scroll(v_mm_per_sec: f64, scroll_accel: f64) -> f64 {
    let mag = v_mm_per_sec.abs();
    if mag == 0.0 {
        return 0.0;
    }
    // LINEAR = scroll_accel × REF^(1 - EXPONENT). At v == REF this gives
    // pixels_per_sec = scroll_accel × REF (matches linear feel).
    let linear = scroll_accel * SCROLL_CURVE_REF_MM_PER_SEC.powf(1.0 - SCROLL_CURVE_EXPONENT);
    v_mm_per_sec.signum() * linear * mag.powf(SCROLL_CURVE_EXPONENT)
}

// ---------- Public CGEvent constants (mirrored from CGEventTypes.h) ----------

const kCGEventLeftMouseDown: u32 = 1;
const kCGEventLeftMouseUp: u32 = 2;
const kCGEventRightMouseDown: u32 = 3;
const kCGEventRightMouseUp: u32 = 4;
const kCGEventMouseMoved: u32 = 5;
const kCGEventLeftMouseDragged: u32 = 6;

const kCGMouseButtonLeft: u32 = 0;
const kCGMouseButtonRight: u32 = 1;

const kCGScrollEventUnitPixel: u32 = 0;

/// Event tap location for scroll events. The HID tap (0) sits below the
/// gesture-engine and AppleMultitouchHIDService — events injected there
/// can be filtered or merged into a multitouch device's own gesture
/// state, which on this firmware (matched by AppleMultitouchHIDService
/// on its `(0xFF60, 0x07)` HID usage) means our scroll posts get
/// silently absorbed. The session tap (1) sits one level up, post-HID
/// and pre-annotation: the path real trackpads' events take. Matches
/// `smooth_scroll.swift`'s `.cgSessionEventTap` choice.
const kCGSessionEventTap: u32 = 1;
/// Default for non-scroll events (mouse moves, clicks, gestures); these
/// have been working fine on the HID tap and the session-tap risk isn't
/// worth taking.
const kCGHIDEventTap: u32 = 0;

/// `kCGEventSourceStateCombinedSessionState` from `CGEventSource.h`.
/// A source created with this state behaves like a real input device
/// (per Apple docs: "represents the combined state of all event sources
/// in the user session"), so apps that gate trackpad-only behaviors —
/// notably Chrome's rubber-band bounce in WebKit/Blink — accept our
/// events as a "fling" worth animating. A null source (what
/// `CGEventCreateScrollWheelEvent2` accepts) reads as synthetic.
const kCGEventSourceStateCombinedSessionState: i32 = 0;

/// `kCGMouseEventClickState` — the click count (1, 2, 3, …) macOS
/// uses to decide whether to deliver a double/triple-click. Synthetic
/// CGEvents don't get this auto-computed; callers must set it.
const kCGMouseEventClickState: u32 = 1;
const kCGMouseEventDeltaX: u32 = 35;
const kCGMouseEventDeltaY: u32 = 36;

// Public scroll-event fields (CGEventTypes.h).
const kCGScrollWheelEventPointDeltaAxis1: u32 = 96;
const kCGScrollWheelEventPointDeltaAxis2: u32 = 97;
const kCGScrollWheelEventFixedPtDeltaAxis1: u32 = 93;
const kCGScrollWheelEventFixedPtDeltaAxis2: u32 = 94;
const kCGScrollWheelEventIsContinuous: u32 = 88;
// Public scroll-phase fields (in CGEventTypes.h since macOS 10.7).
const kCGScrollWheelEventScrollPhase: u32 = 99;
const kCGScrollWheelEventMomentumPhase: u32 = 123;

/// Translate a `Phase` to the integer value `kCGScrollWheelEventScrollPhase`
/// expects. That field uses `CGScrollPhase`, **not** `NSEventPhase`:
/// began=1, changed=2, ended=4, cancelled=8. Posting NSEventPhase values
/// here means apps see e.g. our "Changed" (NSEvent=4) as CGScrollPhase
/// `Ended`, and Chrome / terminal-style scroll consumers immediately
/// terminate the gesture instead of tracking motion. AppKit translates
/// CGScrollPhase → NSEventPhase when it builds an NSEvent for delivery,
/// so writing the CG values is what matches real trackpads on the wire.
fn cg_scroll_phase(phase: Phase) -> i64 {
    match phase {
        Phase::Began => 1,
        Phase::Changed => 2,
        Phase::Ended => 4,
        Phase::Cancelled => 0, // sentinel: caller used Cancelled to mean "no scroll phase"
    }
}

/// Translate a `Phase` to the integer value
/// `kCGScrollWheelEventMomentumPhase` expects. That field uses
/// `CGMomentumScrollPhase` (sequential, not bit-flags): none=0, begin=1,
/// continue=2, end=3. Inertia in `Momentum::tick` posts a Began once
/// then Changed-Changed-…; the latter must map to `continue`, and the
/// final Ended (or our momentum-cancel post) maps to `end`.
fn cg_momentum_phase(phase: Phase) -> i64 {
    match phase {
        Phase::Began => 1,
        Phase::Changed => 2,
        Phase::Ended => 3,
        Phase::Cancelled => 0, // sentinel: caller used Cancelled to mean "no momentum phase"
    }
}

/// Translate a `Phase` to the integer value the private gesture-event
/// phase field (`kCGEventGesturePhase`, 132) expects. Despite NSEvent's
/// public `phase` property being NSEventPhase (1/4/8/16 bit flags), the
/// underlying CGEvent field on a private gesture event holds an
/// `IOHIDEventPhaseBits` value: 1=Began, 2=Changed, 4=Ended, 8=Cancelled
/// (sequential-ish, not bit flags). Confirmed via calftrail/Touch's
/// `tl_CGEventCreateFromGesture` (which uses IOHIDEventPhaseBits) and
/// Hammerspoon's `newGesture` (same). Setting NSEventPhase here produces
/// events that NSMagnificationGestureRecognizer-using apps (Photos,
/// Apple Maps) silently drop, even though the simpler
/// NSResponder.magnify(with:) path may still react.
fn iohid_gesture_phase(phase: Phase) -> u32 {
    match phase {
        Phase::Began => 1,
        Phase::Changed => 2,
        Phase::Ended => 4,
        Phase::Cancelled => 8,
    }
}

/// Gesture subtype values. From calftrail/Touch's `TLInfoSubtype` enum
/// (used by Hammerspoon's `tl_CGEventCreateFromGesture` and the older
/// MultitouchSupport sources). The synthesizer writes this to CGEvent
/// field 0x6E ("gestureSubtype") on the `NSEventTypeGesture` (29)
/// wrapper. Used for magnify/rotate; 3F/4F swipes go through the
/// DockControl path below — calftrail's `kTLInfoSubtypeSwipe` (0x10) is
/// a legacy NSEventTypeSwipe shape that modern macOS Mission Control /
/// Spaces no longer consume.
const GESTURE_SUBTYPE_ROTATE: u32 = 0x05;
const GESTURE_SUBTYPE_MAGNIFY: u32 = 0x08;

// ---------- DockSwipe synthesis ----------
//
// macOS routes animated 3F/4F swipes through a `kCGSEventDockControl`
// CGEvent (event type 30) carrying `kIOHIDEventTypeDockSwipe`
// (subtype 23), posted on `kCGSessionEventTap`. Driving a stream of
// Changed events with progress walking smoothly from ~0 toward the
// final value over real wall-clock time animates the Mission
// Control / App Exposé / Spaces / Full-Screen Apps rubber-band
// transition; the user can reverse or abort mid-gesture exactly
// like a real trackpad. (A degenerate Began→Ended pair commits
// without animating — `jurplel/iss`'s "instant switch" shortcut.)
// `Emitter::swipe_synthetic` is live-driven by the gesture engine:
// each chip frame produces one Changed, lift produces Ended.
//
// Sources used:
//   - Field names and indices: Apple's WebKit test SPI header
//     `Tools/TestRunnerShared/spi/CoreGraphicsTestSPI.h`
//     (BSD-2-Clause). Only public, permissively-licensed list of the
//     CGSEvent gesture fields.
//   - The duplicate-phase and axis-marker fields below are transcribed from
//     real MBP captures and the public CoreGraphics test SPI. Their semantics
//     are undocumented and must be treated as a versioned compatibility shape,
//     not a stable API.

// CGSEventType from CoreGraphicsTestSPI.h.
const kCGSEventDockControl: i64 = 30;

// IOHIDEventType from `<IOKit/hid/IOHIDEventTypes.h>`.
const kIOHIDEventTypeDockSwipe: i64 = 23;

// CGEventField indices from CoreGraphicsTestSPI.h.
const kCGSEventTypeField: u32 = 55;
const kCGEventGestureHIDType: u32 = 110;
const kCGEventGestureScrollY: u32 = 119;
const kCGEventGestureSwipeMotion: u32 = 123;
const kCGEventGestureSwipeProgress: u32 = 124;
const kCGEventGestureSwipeVelocityX: u32 = 129;
const kCGEventGestureSwipeVelocityY: u32 = 130;
const kCGEventGesturePhase: u32 = 132;
const kCGEventGesturePhaseDuplicate: u32 = 134;
const kCGEventScrollGestureFlagBits: u32 = 135;
const kCGEventGestureInvertedFromDevice: u32 = 136;
const kCGEventGestureZoomDeltaX: u32 = 139;
const kCGEventGestureSwipeMotionDuplicate: u32 = 165;
const kCGEventGestureMarker: u32 = 41;

/// Legacy DockSwipe carries the axis selector redundantly as the low bits of
/// two denormal Float32 values. This is the shape used by Mac Mouse Fix and
/// dockswipe on macOS 26 and earlier; keep it isolated so a future field
/// change cannot silently alter the pre-27 compatibility path.
fn legacy_axis_marker(motion: i64) -> f64 {
    let bits = match motion {
        SWIPE_MOTION_HORIZONTAL => 1,
        SWIPE_MOTION_VERTICAL => 2,
        _ => 0,
    };
    f64::from(f32::from_bits(bits))
}

/// Field 135 is not a boolean direction flag. Dock reads the low 32 bits as
/// the bit pattern of a Float32 cumulative progress value.
fn legacy_progress_bits(progress: f64) -> i64 {
    (progress as f32).to_bits() as i64
}

// CGSGesturePhase values from CoreGraphicsTestSPI.h. Numerically
// equal to the `IOHIDEventPhaseBits` enum used for pinch/rotate;
// these are the names that apply on DockSwipe events.
const kCGSGesturePhaseBegan: i64 = 1;
const kCGSGesturePhaseChanged: i64 = 2;
const kCGSGesturePhaseEnded: i64 = 4;
const kCGSGesturePhaseCancelled: i64 = 8;

// `kCGEventGestureSwipeMotion` values. No permissive source
// enumerates these symbolically; observed empirically.
const SWIPE_MOTION_HORIZONTAL: i64 = 1;
const SWIPE_MOTION_VERTICAL: i64 = 2;

// ---------- Dock-notification swipe ----------
//
// Alternative to synthesis: rather than building a DockSwipe event,
// just call `CoreDockSendNotification(CFSTR("com.apple.expose.awake"))`
// directly. Discrete (no live animation), vertical-only — same path
// Hammerspoon's `hs.spaces.toggleMissionControl` takes
// (extensions/spaces/libspaces.m:241). Selected per-axis via
// `Config::vertical_swipe = SwipeBackend::Notification`; useful when
// the synthesis path doesn't behave on a given macOS version.
// The symbol is exported from the dyld shared cache (no on-disk
// framework binary on modern macOS), so we resolve it at runtime via
// `dlsym` rather than committing to a specific `-framework` link.

/// Notification name that toggles Mission Control. From
/// hammerspoon/extensions/spaces/spaces.lua:258.
const DOCK_NOTIF_MISSION_CONTROL: &str = "com.apple.expose.awake";
/// Notification name that toggles App Exposé. From
/// hammerspoon/extensions/spaces/spaces.lua:272.
const DOCK_NOTIF_APP_EXPOSE: &str = "com.apple.expose.front.awake";

// macOS 27+ rejects synthetic DockSwipe events generated by third-party
// processes. Mac Mouse Fix's later runtime-version fix keeps the original
// DockSwipe path on macOS 26 and uses the WindowServer's SymbolicHotKey
// registry on macOS 27+. These distances
// are deliberately expressed in the gesture engine's millimetres, not screen
// pixels: the phone/HID input has no stable display scale. They are
// conservative enough to avoid an accidental Space switch while dragging and
// short enough for a natural cross-desktop move. A single switch is allowed
// per cooldown window.
const SYMBOLIC_SPACE_THRESHOLD_MM: f64 = 10.0;
const SYMBOLIC_EXPOSE_THRESHOLD_MM: f64 = 7.0;
const SYMBOLIC_SWITCH_COOLDOWN: Duration = Duration::from_millis(350);
const SYMBOLIC_FLICK_VELOCITY_MM_PER_SEC: f64 = 180.0;
const SWIPE_PROGRESS_REF_MM: f64 = 50.0;

// CGS symbolic hot-key IDs (from CGSHotKeys.h, also used by Mac Mouse Fix).
const SHK_EXPOSE_ALL_WINDOWS: u32 = 32; // Mission Control
const SHK_EXPOSE_APPLICATION_WINDOWS: u32 = 33; // App Exposé
const SHK_SPACE_LEFT: u32 = 79;
const SHK_SPACE_RIGHT: u32 = 81;

/// Cumulative-progress threshold above which a vertical swipe commits
/// the Mission Control / Exposé toggle. With
/// [`SWIPE_PROGRESS_REF_MM`] = 50mm, 0.2 ≈ 10mm of finger travel —
/// large enough to clearly distinguish from a 3F tap or accidental
/// drift past the axis-lock threshold (3mm), small enough that a
/// natural quick flick (10–15mm) reliably fires. Only meaningful in
/// the `Notification` swipe backend — `Synthetic` defers commit
/// decisions to the Dock itself based on origin offset and lift
/// velocity, like a real trackpad.
const SWIPE_VERTICAL_COMMIT_PROGRESS: f64 = 0.2;

/// Soft cap on the End-event velocity (mm/s). The Dock interprets
/// large lift velocities as a deliberate flick and commits the
/// transition with little or no animation; capturing the MBP trackpad
/// shows its driver lands in the 3–7 mm/s range even after a brisk
/// swipe (the EMA tracks deceleration into the lift). Our raw
/// gesture-engine EMA can land much higher when the user lifts while
/// still moving, which would feel "abrupt" relative to the MBP
/// baseline. Saturating at this value keeps fast swipes from flicking
/// past the natural feel without changing slow ones at all. Tunable.
const SWIPE_END_VELOCITY_MAX: f64 = 8.0;

/// Dock can drop an Ended event while WindowServer is busy. Reposting the
/// terminal frame after the short and long delays used by Mac Mouse Fix
/// makes the stream self-healing without changing the live animation.
const DOCK_SWIPE_RESEND_DELAYS: [Duration; 2] =
    [Duration::from_millis(200), Duration::from_millis(500)];

/// Magic CGEventFlags value calftrail's gesture synthesizer sets on the
/// envelope event before serialization (`CGEventSetFlags(e, 256)`).
/// `0x100` is `NX_NONCOALSESCEDMASK` in IOHIDSystem private headers —
/// signals "do not collapse this with adjacent events of the same
/// type". Without it, AppKit's gesture pipeline can drop our synthesized
/// gesture events as duplicates of the surrounding (empty) HID stream.
const GESTURE_EVENT_FLAGS: u64 = 0x100;

// ---------- FFI ----------

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;

#[repr(C)]
struct CFRange {
    location: i64,
    length: i64,
}

unsafe extern "C" {
    fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        ty: u32,
        cursor: CGPoint,
        button: u32,
    ) -> CGEventRef;
    fn CGEventCreateScrollWheelEvent2(
        source: CGEventSourceRef,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventSetLocation(event: CGEventRef, location: CGPoint);
    fn CGEventSetType(event: CGEventRef, ty: u32);
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventSourceFlagsState(state: i32) -> u64;
    fn CGEventSetTimestamp(event: CGEventRef, ts: u64);
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventSetDoubleValueField(event: CGEventRef, field: u32, value: f64);
    fn CGEventCreateData(allocator: *const c_void, event: CGEventRef) -> *mut c_void;
    fn CGEventCreateFromData(allocator: *const c_void, data: *const c_void) -> CGEventRef;
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CGEventSourceCreate(state: i32) -> CGEventSourceRef;
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
    fn CFDataCreateMutableCopy(
        allocator: *const c_void,
        capacity: i64,
        data: *const c_void,
    ) -> *mut c_void;
    fn CFDataAppendBytes(data: *mut c_void, bytes: *const u8, length: i64);
    fn CFDataDeleteBytes(data: *mut c_void, range: CFRange);
    fn CFDataGetLength(data: *const c_void) -> i64;
}

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const i8) -> *mut c_void;
    fn sel_registerName(name: *const i8) -> *mut c_void;
    fn objc_msgSend();
}

struct Event(CGEventRef);

impl Event {
    fn new() -> Option<Self> {
        Self::with_source(std::ptr::null_mut())
    }

    /// Create an event with an explicit `CGEventSource`. Some CGEvent
    /// fields are silently normalized when the event has a NULL source
    /// — the OS treats null-sourced gesture events as untrusted. Pass
    /// the persistent `kCGEventSourceStateCombinedSessionState` source
    /// from `Emitter` to keep the value the caller wrote.
    fn with_source(source: CGEventSourceRef) -> Option<Self> {
        let raw = unsafe { CGEventCreate(source) };
        if raw.is_null() {
            None
        } else {
            Some(Event(raw))
        }
    }

    fn from_raw(raw: CGEventRef) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            Some(Event(raw))
        }
    }

    fn set_int(&self, field: u32, value: i64) {
        unsafe { CGEventSetIntegerValueField(self.0, field, value) };
    }
    fn set_dbl(&self, field: u32, value: f64) {
        unsafe { CGEventSetDoubleValueField(self.0, field, value) };
    }

    /// Write one value using the legacy DockSwipe field's expected CGEvent
    /// storage slot. The Dock's field reader is typed; writing a value into
    /// the other slot leaves the event looking populated in diagnostics while
    /// the Dock sees zero.
    fn set_legacy_dock_field(&self, field: u32, value: f64) {
        match legacy_dock_field_storage(field) {
            LegacyDockFieldStorage::Double => self.set_dbl(field, value),
            LegacyDockFieldStorage::Integer => {
                let integer = if field == kCGEventScrollGestureFlagBits {
                    legacy_progress_bits(value)
                } else {
                    value as i64
                };
                self.set_int(field, integer);
            }
        }
    }
    fn apply_modifiers(&self, mods: u64) {
        // Keep non-keyboard event bits (for example the gesture
        // non-coalescing flag), but replace the four keyboard modifiers with
        // the snapshot that belongs to this event.
        const MODIFIER_FLAGS: u64 = crate::scroll_policy::KEYBOARD_MODIFIER_MASK;
        let existing = unsafe { CGEventGetFlags(self.0) };
        let merged = (existing & !MODIFIER_FLAGS) | (mods & MODIFIER_FLAGS);
        unsafe { CGEventSetFlags(self.0, merged) };
    }
    fn apply_current_modifiers(&self) {
        let mods = unsafe { CGEventSourceFlagsState(kCGEventSourceStateCombinedSessionState) };
        self.apply_modifiers(mods);
    }
    fn post(&self) {
        self.apply_current_modifiers();
        unsafe { CGEventPost(kCGHIDEventTap, self.0) };
    }
    fn post_to(&self, tap: u32) {
        self.apply_current_modifiers();
        unsafe { CGEventPost(tap, self.0) };
    }
    fn post_to_with_modifiers(&self, tap: u32, mods: u64) {
        self.apply_modifiers(mods);
        unsafe { CGEventPost(tap, self.0) };
    }
    /// Post without touching the event's existing flags. This is reserved
    /// for synthetic keyboard shortcuts whose modifier flags are part of the
    /// shortcut itself (for example Cmd+Ctrl+D and Control+Arrow). Pointer
    /// and gesture events should use `post`/`post_to` so their flags follow
    /// the live keyboard snapshot.
    fn post_raw(&self, tap: u32) {
        unsafe { CGEventPost(tap, self.0) };
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0 as *const c_void) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LegacyDockFieldStorage {
    Integer,
    Double,
}

fn legacy_dock_field_storage(field: u32) -> LegacyDockFieldStorage {
    match field {
        // Field 135 is the Float32 progress bit pattern carried in the
        // integer slot. Field 136 is the integer inversion flag.
        kCGEventScrollGestureFlagBits | kCGEventGestureInvertedFromDevice => {
            LegacyDockFieldStorage::Integer
        }
        _ => LegacyDockFieldStorage::Double,
    }
}

// ---------- Calftrail-style gesture synthesizer ----------
//
// `CGEventCreate(NULL); SetType(30); Set(field, value); Post()` produces an
// event AppKit's gesture-recognizer pipeline silently ignores: the
// `NSMagnificationGestureRecognizer` consumed by Photos / Apple Maps wants
// the event to carry an embedded IOHID payload (a digitizer-collection
// event + vendor token + per-touch digitizer events), not just the
// CGEvent fields. We can't get a properly-formed event from a public
// API, so we build one by:
//   1. asking CGEvent for the serialized form of an `NSEventTypeGesture`
//      (type 29) wrapper,
//   2. lopping off CGEvent's empty trailing field array,
//   3. appending a hand-rolled `IOHIDSystemQueueElement` containing a
//      digitizer-hand parent event and a vendor token,
//   4. appending the gesture's CGEvent fields (subtype, IOHID phase,
//      magnification/rotation/swipe value, plus the magic-zero fields
//      AppKit's parser walks),
//   5. and reconstituting via `CGEventCreateFromData`.
// Original C is calftrail/Touch's `tl_CGEventCreateFromGesture`
// (TouchSynthesis/TouchEvents.c); Hammerspoon's `newGesture` and
// jitouch use the same recipe. The IOHID structs are public from
// IOKit headers (IOHIDEventTypes.h / IOHIDEventData.h) but the
// CGEvent serialization layout — the trailing 24-byte trim, the
// {0x10,0x6D} marker, the BE field-encoding scheme, the magic
// zero fields at 0x6F/0x70/0x85/0x8B/0x8C — is reverse-engineered
// and undocumented.

/// `NSEventTypeGesture` — base wrapper for all calftrail-style private
/// gesture events. Subtype goes in field 0x6E.
const kCGEventGesture: u32 = 29;

const kIOHIDEventTypeVendorDefined: u32 = 1;
const kIOHIDEventTypeDigitizer: u32 = 11;
const kIOHIDEventOptionIsCollection: u32 = 0x02;
const kIOHIDDigitizerTransducerTypeHand: u32 = 0x23;
const kIOHIDDigitizerOrientationTypeQuality: u32 = 2;

/// Mirrors `_IOHIDDigitizerEventData` from `IOHIDEventData.h`. Layout
/// must match the C struct exactly: every field's offset and the total
/// `size_of::<DigitizerEventData>()` are read directly by the OS when
/// the appended bytes are demuxed back into events.
#[repr(C)]
struct DigitizerEventData {
    size: u32,
    ty: u32,
    timestamp: u64,
    options: u32,
    position_x: i32, // IOFixed (Q16.16)
    position_y: i32,
    position_z: i32,
    transducer_index: u32,
    transducer_type: u32,
    identity: u32,
    event_mask: u32,
    child_event_mask: u32,
    button_mask: u32,
    tip_pressure: i32,
    barrel_pressure: i32,
    twist: i32,
    orientation_type: u32,
    orientation_quality: i32,
    orientation_density: i32,
    orientation_irregularity: i32,
    orientation_major_radius: i32,
    orientation_minor_radius: i32,
}

/// Mirrors `_IOHIDVendorDefinedEventData`. The flexible `data[0]`
/// trailing array isn't part of `size_of`; payload bytes get appended
/// after this struct in the serialized stream.
#[repr(C)]
struct VendorDefinedEventData {
    size: u32,
    ty: u32,
    timestamp: u64,
    options: u32,
    usage_page: u16,
    usage: u16,
    version: u32,
    length: u32,
}

/// Mirrors `_IOHIDSystemQueueElement`. Trailing flexible
/// `events[]` is not part of `size_of`; child events get appended
/// after this struct.
#[repr(C)]
struct SystemQueueElement {
    timestamp: u64,
    device_id: u64,
    options: u32,
    event_count: u32,
}

/// Per-subtype payload value for the synthesized gesture event. Each
/// variant maps to a specific CGEvent field id (0x71/0x72). The
/// 0-value case is implicit for Began-/Ended-phase magnify and rotate
/// events.
enum GesturePayload {
    Magnification(f32),
    Rotation(f32),
}

/// Append a single CGEvent-serialized field entry: 2 bytes big-endian
/// `count`, 1 byte `type` (0x40 = uint32), 1 byte `field` id, then
/// `count` × 4 bytes of big-endian uint32 payload.
fn append_field_u32(data: *mut c_void, field: u8, value: u32) {
    let count: u16 = 1u16.to_be();
    let type_byte: u8 = 0x40;
    let value_be: u32 = value.to_be();
    unsafe {
        CFDataAppendBytes(data, &count as *const u16 as *const u8, 2);
        CFDataAppendBytes(data, &type_byte, 1);
        CFDataAppendBytes(data, &field, 1);
        CFDataAppendBytes(data, &value_be as *const u32 as *const u8, 4);
    }
}

/// As [`append_field_u32`] but with type 0xC0 (Float32 payload).
fn append_field_f32(data: *mut c_void, field: u8, value: f32) {
    let count: u16 = 1u16.to_be();
    let type_byte: u8 = 0xC0;
    let value_be: u32 = value.to_bits().to_be();
    unsafe {
        CFDataAppendBytes(data, &count as *const u16 as *const u8, 2);
        CFDataAppendBytes(data, &type_byte, 1);
        CFDataAppendBytes(data, &field, 1);
        CFDataAppendBytes(data, &value_be as *const u32 as *const u8, 4);
    }
}

/// Build a complete private gesture CGEvent ready to post. `subtype` is
/// one of `GESTURE_SUBTYPE_*` (8 = magnify, 5 = rotate, 16 = swipe).
/// `phase` is an `IOHIDEventPhaseBits` value (1=Began, 2=Changed,
/// 4=Ended). `payload` carries the subtype-specific value.
///
/// Implements `tl_CGEventCreateFromGesture` from calftrail/Touch with
/// no embedded child touches (Hammerspoon's `newGesture` does the same;
/// the parent digitizer-hand collection alone is enough for AppKit's
/// gesture-recognizer pipeline to bind to the event).
fn synthesize_gesture_event(
    subtype: u32,
    phase: u32,
    payload: GesturePayload,
    ts: Timestamp,
) -> Option<Event> {
    // `Timestamp` is `CLOCK_UPTIME_RAW`-based, which is the time base
    // IOHID stamps on real-trackpad events and what `tl_uptime()` in
    // calftrail returns via the deprecated
    // `AbsoluteToNanoseconds(UpTime())` path. Stamping the gesture
    // wrapper, queue element, parent digitizer event, and vendor token
    // with the same value mirrors what real events look like.
    let timestamp = ts.as_nanos();

    // 1. Base event: type=29 (NSEventTypeGesture) wrapper, magic 256
    //    flags, IOHID-aligned timestamp.
    let proto = unsafe { CGEventCreate(std::ptr::null_mut()) };
    if proto.is_null() {
        return None;
    }
    unsafe {
        CGEventSetType(proto, kCGEventGesture);
        CGEventSetFlags(proto, GESTURE_EVENT_FLAGS);
        CGEventSetTimestamp(proto, timestamp);
    }

    // 2. Serialize. CGEvent's serialized form ends with a 24-byte empty
    //    field-array placeholder we'll overwrite with our own payload.
    let base_data = unsafe { CGEventCreateData(std::ptr::null(), proto) };
    unsafe { CFRelease(proto as *const c_void) };
    if base_data.is_null() {
        return None;
    }
    let gesture_data = unsafe { CFDataCreateMutableCopy(std::ptr::null(), 0, base_data) };
    unsafe { CFRelease(base_data as *const c_void) };
    if gesture_data.is_null() {
        return None;
    }
    let len = unsafe { CFDataGetLength(gesture_data) };
    if len >= 24 {
        unsafe {
            CFDataDeleteBytes(
                gesture_data,
                CFRange {
                    location: len - 24,
                    length: 24,
                },
            )
        };
    }

    // 3. Append the IOHID payload header: a 16-bit big-endian total size
    //    plus the {0x10, 0x6D} marker that flags the rest as the
    //    serialized-events blob.
    let parent_size = std::mem::size_of::<DigitizerEventData>() as u32;
    let queue_size = std::mem::size_of::<SystemQueueElement>() as u32;
    let vendor_struct_size = std::mem::size_of::<VendorDefinedEventData>() as u32;
    let vendor_payload_size: u32 = 40;
    let vendor_total = vendor_struct_size + vendor_payload_size;
    let total_size: u16 = (queue_size + vendor_total + parent_size) as u16;
    unsafe {
        let total_be = total_size.to_be();
        CFDataAppendBytes(gesture_data, &total_be as *const u16 as *const u8, 2);
        let marker: [u8; 2] = [0x10, 0x6D];
        CFDataAppendBytes(gesture_data, marker.as_ptr(), 2);
    }

    // 4. Queue-element header (host-endian — these are raw IOHID
    //    structs, not CGEvent-serialized fields).
    let queue = SystemQueueElement {
        timestamp,
        device_id: 0,
        options: kIOHIDEventOptionIsCollection,
        event_count: 2, // parent digitizer + vendor token
    };
    unsafe {
        CFDataAppendBytes(
            gesture_data,
            &queue as *const _ as *const u8,
            queue_size as i64,
        )
    };

    // 5. Parent digitizer event — a "hand" collection with empty quality
    //    orientation. No real touches embedded; AppKit treats this as
    //    "synthetic 2F gesture from a multitouch device" via the hand
    //    transducer type and binds the recognizer accordingly.
    let parent = DigitizerEventData {
        size: parent_size,
        ty: kIOHIDEventTypeDigitizer,
        timestamp,
        options: kIOHIDEventOptionIsCollection,
        position_x: 0,
        position_y: 0,
        position_z: 0,
        transducer_index: 0,
        transducer_type: kIOHIDDigitizerTransducerTypeHand,
        identity: 0,
        event_mask: 0,
        child_event_mask: 0,
        button_mask: 0,
        tip_pressure: 0,
        barrel_pressure: 0,
        twist: 0,
        orientation_type: kIOHIDDigitizerOrientationTypeQuality,
        orientation_quality: 0,
        orientation_density: 0,
        orientation_irregularity: 0,
        orientation_major_radius: 0,
        orientation_minor_radius: 0,
    };
    unsafe {
        CFDataAppendBytes(
            gesture_data,
            &parent as *const _ as *const u8,
            parent_size as i64,
        )
    };

    // 6. Vendor token. usagePage 0xFF00 / usage 0x1777 is the magic
    //    pair calftrail discovered AppleMultitouchHIDService stamps on
    //    real-trackpad gesture events; the 40-byte payload is mostly
    //    zeros but the first 8 bytes hold a deviceID (0 here = "no
    //    specific device").
    let vendor_header = VendorDefinedEventData {
        size: vendor_total,
        ty: kIOHIDEventTypeVendorDefined,
        timestamp,
        options: 0,
        usage_page: 0xFF00,
        usage: 0x1777,
        version: 1,
        length: vendor_payload_size,
    };
    unsafe {
        CFDataAppendBytes(
            gesture_data,
            &vendor_header as *const _ as *const u8,
            vendor_struct_size as i64,
        );
        let payload = [0u8; 40];
        CFDataAppendBytes(gesture_data, payload.as_ptr(), vendor_payload_size as i64);
    }

    // 7. CGEvent fields (each 8-byte big-endian header + payload). The
    //    0x6F/0x70/0x85 zero fields aren't optional — the AppKit field
    //    walker expects them in this exact order. 0x8B/0x8C are
    //    likewise required-zero floats at the tail.
    append_field_u32(gesture_data, 0x6E, subtype); // gestureSubtype
    append_field_u32(gesture_data, 0x6F, 0);
    append_field_u32(gesture_data, 0x70, 0);
    append_field_u32(gesture_data, 0x84, phase); // gesturePhase
    append_field_u32(gesture_data, 0x85, 0);
    match payload {
        GesturePayload::Magnification(m) => append_field_f32(gesture_data, 0x71, m),
        GesturePayload::Rotation(r) => append_field_f32(gesture_data, 0x72, r),
    }
    append_field_f32(gesture_data, 0x8B, 0.0);
    append_field_f32(gesture_data, 0x8C, 0.0);

    // 8. Reconstitute the CGEvent.
    let synth = unsafe { CGEventCreateFromData(std::ptr::null(), gesture_data) };
    unsafe { CFRelease(gesture_data as *const c_void) };
    Event::from_raw(synth)
}

// ---------- Public API ----------

#[derive(Clone, Debug)]
pub struct Config {
    /// Screen pixels emitted per millimeter of finger motion in scroll mode.
    pub scroll_accel: f64,
    /// Natural scrolling: finger-down on the pad scrolls content down on
    /// the screen (the macOS default since 10.7). False for the legacy
    /// "wheel" convention where finger-down moves the scrollbar down /
    /// the content up.
    pub natural_scroll: bool,
    /// Preserve horizontal scroll deltas when true. macOS exposes this as
    /// `TrackpadHorizScroll`; vertical deltas remain available either way.
    pub horizontal_scroll: bool,
    /// Start a momentum-phase coast after a fast two-finger lift.
    pub momentum_scroll: bool,
    /// Quartz modifier mask that redirects a scroll session to magnification.
    /// macOS Accessibility Zoom normally selects one of Control, Option, or
    /// Command; the value is read from `HIDScrollZoomModifierMask`.
    pub modifier_zoom_mask: u64,
    /// Preserve native trackpad axis behavior when false. Set true only for
    /// the compatibility convention that maps Shift + vertical scroll to X.
    pub shift_scroll_horizontal: bool,
    /// Whether semantic haptic cues are enabled for this emitter.
    pub haptic_feedback: bool,
    /// Emit private CGEvent pinch events. Per-gesture gate evaluated at
    /// `Phase::Began` against the bundle ID under the cursor; the
    /// decision is held for the duration of the touch. Independent of
    /// [`Self::rotate`].
    pub pinch: GesturePolicy,
    /// Emit private CGEvent rotate events. Same gating model as
    /// [`Self::pinch`]; independent stream.
    pub rotate: GesturePolicy,
    /// Left/right 3F/4F swipes (Spaces / Full-Screen Apps).
    /// `SwipeBackend::Notification` isn't a meaningful option here (no
    /// Dock notification for "switch space"); it's silently treated as
    /// `Off`.
    pub horizontal_swipe: SwipeConfig,
    /// Up/down 3F/4F swipes (Mission Control / App Exposé).
    pub vertical_swipe: SwipeConfig,
}

/// Per-axis swipe configuration: `policy` gates the gesture against the
/// bundle ID under the cursor at `Phase::Began`; `backend` chooses the
/// wire path (live DockSwipe synthesis vs. discrete dock notification)
/// once the policy admits the gesture.
#[derive(Clone, Debug)]
pub struct SwipeConfig {
    pub policy: GesturePolicy,
    pub backend: SwipeBackend,
}

/// Per-gesture admission policy. Evaluated at `Phase::Began` against
/// the bundle ID of the application owning the topmost normal window
/// under the cursor; the resulting decision (allow / deny) is held for
/// the rest of the gesture so a mid-gesture window switch can't kill
/// its own gesture.
///
/// `Only` denies when no app is under the cursor (e.g. desktop /
/// menu bar). `Except` allows in that case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GesturePolicy {
    On,
    Off,
    Only(Vec<String>),
    Except(Vec<String>),
}

impl GesturePolicy {
    /// Decide whether to admit a gesture given a function that resolves
    /// the bundle ID under the cursor. The closure is only invoked for
    /// `Only` / `Except` so the on/off fast path stays free of system
    /// queries.
    pub fn evaluate(&self, lookup: impl FnOnce() -> Option<String>) -> bool {
        match self {
            GesturePolicy::On => true,
            GesturePolicy::Off => false,
            GesturePolicy::Only(list) => match lookup() {
                Some(id) => list.iter().any(|x| x == &id),
                None => false,
            },
            GesturePolicy::Except(list) => match lookup() {
                Some(id) => !list.iter().any(|x| x == &id),
                None => true,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeBackend {
    /// Synthesize trackpad DockSwipe events directly. Animates the
    /// rubber-band live; user can reverse or abort mid-gesture.
    Synthetic,
    /// Trigger the Dock's Mission Control / App Exposé via private
    /// `CoreDockSendNotification` on lift past a commit threshold.
    /// Discrete (no live animation), vertical-only — silently `Off` if
    /// selected for the horizontal axis.
    Notification,
    /// Suppress entirely.
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Phase {
    Began,
    Changed,
    Ended,
    Cancelled,
}

/// Semantic haptic cues. The macOS emitter maps these to Apple's system
/// performer; portable emitters intentionally treat them as no-ops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HapticKind {
    /// A discrete click or secondary click.
    Click,
    /// The moment a drag crosses its engage threshold.
    DragEngaged,
    /// A gesture reached a committed system action.
    GestureCommitted,
}

/// Axis along which a multi-finger swipe is accumulating progress.
/// Selected by the gesture engine on first significant motion (whichever
/// of horizontal/vertical dominates) and held for the rest of the
/// gesture so a wandering centroid doesn't flip the swipe sideways
/// mid-flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeAxis {
    Horizontal,
    Vertical,
}

/// Trait surface a gesture engine sees. Implemented by [`Emitter`]
/// against real CGEvents; in tests, swap in a recording fake. All
/// motion arguments are in physical millimeters of finger travel.
pub trait Output {
    /// Called by the gesture engine at the top of each
    /// [`crate::gesture::State::on_frame_at`] with the host-aligned
    /// scan timestamp. CGEvent-emitting implementations stamp every
    /// event posted from subsequent trait calls (until the next
    /// `set_event_time`) with this value. Out-of-band events not tied
    /// to a frame — `CFRunLoopTimer`-driven inertia ticks, `Drop`
    /// cleanup — fall back to `Timestamp::now()`. Default is a no-op
    /// so test fakes that don't care about timestamps don't have to
    /// implement it.
    fn set_event_time(&self, _ts: Timestamp) {}
    /// Request a system haptic cue. Implementations may suppress it when the
    /// current device has no haptic engine or the user has disabled feedback.
    fn haptic(&self, _kind: HapticKind) {}
    /// Whether pinch / rotate / per-axis swipe are currently admissible
    /// under the active output's policy. The gesture engine consults
    /// these once at gesture start (when it stages the per-gesture
    /// baseline) and uses the snapshot to gate which modes are
    /// candidates for the lock decision — so an app that doesn't allow
    /// pinch/rotate doesn't strand a 2F gesture in pinch+rotate when
    /// the user meant to scroll. Defaults are admissive so test fakes
    /// don't have to thread policy through.
    fn pinch_admissible_now(&self) -> bool {
        true
    }
    fn rotate_admissible_now(&self) -> bool {
        true
    }
    fn swipe_admissible_now(&self, _axis: SwipeAxis) -> bool {
        true
    }
    /// Post a cursor move with the given pixel deltas. The gesture
    /// engine has already applied any acceleration curve and rounded
    /// to integer pixels (carrying the sub-pixel residual across
    /// frames), so this is a thin wrapper around `CGEventCreateMouseEvent`
    /// — no scaling here.
    fn move_cursor_by(&self, dx_px: i32, dy_px: i32);
    fn click(&self, button: MouseButton);
    /// Return the currently held Quartz keyboard modifier bits. Portable
    /// fakes may return zero; macOS emitters sample the combined session
    /// state. Gesture recognition uses this only when an action is delayed
    /// past the frame that originally established it.
    fn current_modifiers(&self) -> u64 {
        0
    }
    /// Emit a click with a modifier snapshot captured at gesture time. The
    /// default keeps existing output fakes source-compatible.
    fn click_with_modifiers(&self, button: MouseButton, _modifiers: u64) {
        self.click(button);
    }
    /// Latch the integrated touchpad button. Driven by the firmware's
    /// PTP report bit (bit 0 = left), which in turn mirrors keymap-driven
    /// `MouseBtn1` presses. While held, [`Self::move_cursor_by`] should
    /// post `LeftMouseDragged` so apps see a real drag rather than a
    /// move-while-button-pressed mismatch. Implementations are expected
    /// to dedupe (called once per frame regardless of change) and only
    /// post the corresponding mouse-down/up CGEvents on actual edges.
    /// Successive press edges within the system double-click window
    /// should advance the click count so a fast hardware re-press
    /// delivers a real double/triple-click — both directly via this
    /// path and chained with [`Self::click`] (a tap followed by a
    /// quick hardware press counts as one multi-click sequence).
    fn set_left_button_held(&self, held: bool);
    /// Latch the left button for a gesture-owned drag. Unlike the physical
    /// button path above, this must not advance the click-count sequence:
    /// a three-finger drag is not a click and the next tap must remain a
    /// single click.
    fn set_drag_button_held(&self, held: bool) {
        self.set_left_button_held(held);
    }
    fn scroll(&self, dx_mm: f64, dy_mm: f64, phase: Phase);
    /// Seed scroll inertia from a just-ended pan. `vx_mm_per_sec` and
    /// `vy_mm_per_sec` are the EMA-smoothed centroid velocity at lift.
    /// Implementation drives a self-paced momentum-phase scroll stream
    /// that decays to zero. Called once per scroll session, after the
    /// final `scroll(.., Phase::Ended)`.
    fn scroll_inertia(&self, vx_mm_per_sec: f64, vy_mm_per_sec: f64);
    /// Cancel an in-flight inertia coast (typically because a new touch
    /// landed). Implementations should bracket the cancellation with a
    /// `MomentumPhase::Ended` event so apps stop their scroll animations.
    /// Returns `true` if a coast was actually live and got cancelled —
    /// callers use this to flag the new touch as "born during coast"
    /// (rmk-style) and suppress any tap derived from it: the user
    /// reached in to stop the fling, not to click.
    fn cancel_inertia(&self) -> bool;
    fn pinch(&self, delta: f64, phase: Phase);
    fn rotate(&self, delta_degrees: f64, phase: Phase);
    /// Drive a multi-finger swipe live. `signed_progress` is the
    /// cumulative finger displacement along `axis`, normalized so that
    /// ±1.0 ≈ "user has clearly committed to the swipe". Sign carries
    /// direction (positive = right / down). `velocity_mm_per_sec` is
    /// only meaningful at `Phase::Ended` (drives the Dock's
    /// commit-vs-rubber-band decision); pass 0 for other phases.
    /// Phase brackets the gesture exactly like `scroll`: Began on the
    /// first emit after axis lock, Changed each subsequent frame,
    /// Ended on lift / finger-count drop. Implementations may track
    /// state across calls; `Cancelled` clears any in-flight stream.
    fn swipe(&self, axis: SwipeAxis, signed_progress: f64, velocity_mm_per_sec: f64, phase: Phase);
    /// Trigger macOS dictionary look up (Cmd + Ctrl + D).
    fn look_up_dictionary(&self) {}
    /// Trigger macOS smart zoom (Smart Magnify).
    fn smart_magnify(&self) {}
    /// Trigger macOS notification center toggle.
    fn toggle_notification_center(&self) {}
    /// Trigger macOS Launchpad toggle.
    fn toggle_launchpad(&self) {}
    /// Trigger macOS Show Desktop toggle.
    fn toggle_show_desktop(&self) {}
    /// Trigger macOS App Exposé toggle.
    fn toggle_app_expose(&self) {}
    /// Trigger macOS Mission Control toggle.
    fn toggle_mission_control(&self) {}
}

/// Decorator over an [`Output`] that flashes a debug HUD on each
/// `Phase::Began` — the moments where the gesture engine commits to
/// scroll vs pinch+rotate, or locks a swipe axis. Admitted pinch/rotate
/// streams begin at the same lock instant (see `gesture.rs`'s
/// `TwoFingerPinchAndRotate` mode), so we flash on pinch only and elide
/// rotate to keep it to one badge per gesture.
pub struct OverlayOutput<O: Output> {
    inner: O,
    overlay: Box<crate::overlay::Overlay>,
    /// Per-flash counter. Same number is rendered on the HUD and logged
    /// at info level so a `grep '#42'` in the logs lands on the matching
    /// gesture-lock event. `Cell` is fine: all `Output` calls run on the
    /// IOHID/main-thread run loop.
    seq: Cell<u64>,
}

impl<O: Output> OverlayOutput<O> {
    pub fn new(inner: O, overlay: Box<crate::overlay::Overlay>) -> Self {
        Self {
            inner,
            overlay,
            seq: Cell::new(0),
        }
    }

    fn flash(&self, name: &str) {
        let n = self.seq.get().wrapping_add(1);
        self.seq.set(n);
        log::info!("overlay #{n}: {name}");
        self.overlay.flash(name, n);
    }
}

impl<O: Output> Output for OverlayOutput<O> {
    fn set_event_time(&self, ts: Timestamp) {
        self.inner.set_event_time(ts);
    }
    fn haptic(&self, kind: HapticKind) {
        self.inner.haptic(kind);
    }
    fn pinch_admissible_now(&self) -> bool {
        self.inner.pinch_admissible_now()
    }
    fn rotate_admissible_now(&self) -> bool {
        self.inner.rotate_admissible_now()
    }
    fn swipe_admissible_now(&self, axis: SwipeAxis) -> bool {
        self.inner.swipe_admissible_now(axis)
    }
    fn move_cursor_by(&self, dx_px: i32, dy_px: i32) {
        self.inner.move_cursor_by(dx_px, dy_px);
    }
    fn click(&self, button: MouseButton) {
        self.inner.click(button);
    }
    fn current_modifiers(&self) -> u64 {
        self.inner.current_modifiers()
    }
    fn click_with_modifiers(&self, button: MouseButton, modifiers: u64) {
        self.inner.click_with_modifiers(button, modifiers);
    }
    fn set_left_button_held(&self, held: bool) {
        self.inner.set_left_button_held(held);
    }
    fn set_drag_button_held(&self, held: bool) {
        self.inner.set_drag_button_held(held);
    }
    fn scroll(&self, dx_mm: f64, dy_mm: f64, phase: Phase) {
        if matches!(phase, Phase::Began) {
            self.flash("SCROLL");
        }
        self.inner.scroll(dx_mm, dy_mm, phase);
    }
    fn scroll_inertia(&self, vx_mm_per_sec: f64, vy_mm_per_sec: f64) {
        self.inner.scroll_inertia(vx_mm_per_sec, vy_mm_per_sec);
    }
    fn cancel_inertia(&self) -> bool {
        self.inner.cancel_inertia()
    }
    fn pinch(&self, delta: f64, phase: Phase) {
        if matches!(phase, Phase::Began) {
            self.flash("PINCH / ROTATE");
        }
        self.inner.pinch(delta, phase);
    }
    fn rotate(&self, delta_degrees: f64, phase: Phase) {
        // Paired with pinch at lock-in; intentionally no flash here.
        self.inner.rotate(delta_degrees, phase);
    }
    fn swipe(&self, axis: SwipeAxis, signed_progress: f64, velocity_mm_per_sec: f64, phase: Phase) {
        if matches!(phase, Phase::Began) {
            let label = match axis {
                SwipeAxis::Horizontal => "SWIPE  \u{2194}",
                SwipeAxis::Vertical => "SWIPE  \u{2195}",
            };
            self.flash(label);
        }
        self.inner
            .swipe(axis, signed_progress, velocity_mm_per_sec, phase);
    }
    fn look_up_dictionary(&self) {
        self.flash("LOOKUP");
        self.inner.look_up_dictionary();
    }
    fn smart_magnify(&self) {
        self.flash("SMART ZOOM");
        self.inner.smart_magnify();
    }
    fn toggle_notification_center(&self) {
        self.flash("NOTIFICATION CENTER");
        self.inner.toggle_notification_center();
    }
    fn toggle_launchpad(&self) {
        self.flash("LAUNCHPAD");
        self.inner.toggle_launchpad();
    }
    fn toggle_show_desktop(&self) {
        self.flash("SHOW DESKTOP");
        self.inner.toggle_show_desktop();
    }
    fn toggle_app_expose(&self) {
        self.flash("APP EXPOSÉ");
        self.inner.toggle_app_expose();
    }
    fn toggle_mission_control(&self) {
        self.flash("MISSION CONTROL");
        self.inner.toggle_mission_control();
    }
}

pub struct Emitter {
    cfg: Config,
    /// Persistent CGEventSource. Created with
    /// `kCGEventSourceStateCombinedSessionState` so apps see our scroll
    /// events as coming from a real input device — Chrome / WebKit
    /// gates rubber-band bounce on this. Held for the lifetime of the
    /// emitter; released on Drop.
    event_source: CGEventSourceRef,
    /// Most recent click that produced a CGEvent post: button + time +
    /// cursor location. The next click checks this to decide its
    /// `kCGMouseEventClickState` value — same button, within
    /// [`DOUBLE_CLICK_INTERVAL`], within [`DOUBLE_CLICK_DISTANCE_PX`] →
    /// increment; otherwise reset to 1. macOS doesn't auto-compute the
    /// click count for synthetic events, so triple-click etc. depend
    /// entirely on this field. Tap-derived `click()` and hardware-button
    /// presses share this state, so consecutive presses across the two
    /// sources count as one multi-click sequence.
    last_click: Cell<Option<(MouseButton, Timestamp, CGPoint)>>,
    click_count: Cell<i64>,
    /// Sub-pixel carry for active scroll, by axis. Each `scroll()` call's
    /// f64 pixel value gets accumulated; the integer part drives the
    /// CGEvent's PointDelta / wheel fields, and the fractional part rolls
    /// over to the next event. Without this, integer truncation drops up
    /// to one pixel per event, which on a 60+ Hz event stream is a
    /// noticeable drift on slow scrolls.
    scroll_carry_x_px: Cell<f64>,
    scroll_carry_y_px: Cell<f64>,
    /// Wall-clock time of the most recent `scroll()` call. Used to derive
    /// per-frame `dt` so the acceleration curve can run on velocity
    /// (mm/s) rather than raw per-frame mm — keeps feel consistent
    /// across pad frame rates.
    scroll_last_time: Cell<Option<Timestamp>>,
    /// Modifier-zoom decision captured at `scroll(Began)` and held until the
    /// corresponding Ended/Cancelled frame. This prevents a scroll stream
    /// from changing event families when the physical modifier is released
    /// just before the fingers lift.
    scroll_redirected: Cell<bool>,
    /// Inertia state plus its CFRunLoopTimer. Boxed for a stable address
    /// — the timer's C context holds a raw pointer back here. Allocated
    /// once at `new()`; the timer ref inside is None except while a
    /// coast is in flight.
    momentum: Box<Momentum>,
    /// Axis of the in-flight swipe, if any. Held so Drop can post a
    /// final Cancelled on the same axis if the process exits mid-
    /// gesture, sparing the Dock a stuck rubber-band. None when no
    /// swipe is active.
    swipe_axis: Cell<Option<SwipeAxis>>,
    /// Monotonically increasing handoff generation. Delayed DockSwipe Ended
    /// frames from an earlier gesture are discarded once a new Began arrives.
    swipe_generation: Arc<AtomicU64>,
    /// Host-aligned scan timestamp for the frame currently being
    /// processed by the gesture engine. Set via `set_event_time` at
    /// the top of `on_frame_at`; consulted by every per-frame emit
    /// site for `CGEventSetTimestamp`. None outside a frame — out-of-
    /// band emits (inertia ticks, Drop cleanup) fall back to
    /// `Timestamp::now()`.
    event_time: Cell<Option<Timestamp>>,
    /// `Some(count)` between `LeftMouseDown` and `LeftMouseUp` posts,
    /// `None` otherwise. While `Some`, `move_cursor_by` emits
    /// `LeftMouseDragged` instead of `MouseMoved` so apps see a real
    /// drag stream. Driven by [`Output::set_left_button_held`] for the
    /// firmware's PTP integrated button and [`Output::set_drag_button_held`]
    /// for gesture-owned drags. The stored count is the
    /// `kCGMouseEventClickState`
    /// assigned at press time and replayed verbatim on release, so an
    /// intervening tap-derived `click()` (which would mutate
    /// `click_count`) can't desync the down/up pair.
    left_button_held: Cell<Option<i64>>,
    /// State for the macOS 26/27+ SymbolicHotKey swipe fallback. The fallback
    /// is deliberately kept in the emitter, alongside the DockSwipe state,
    /// so a 3F drag can carry its held button through repeated Space changes.
    symbolic_swipe_active: Cell<bool>,
    symbolic_swipe_last_distance_mm: Cell<f64>,
    symbolic_swipe_pending_mm: Cell<f64>,
    symbolic_swipe_last_fire_at: Cell<Option<Timestamp>>,
    symbolic_swipe_last_motion_at: Cell<Option<Timestamp>>,
}

/// Inertia state. All cells are accessed only from the main run loop
/// thread (CFRunLoopTimer fires there), so `Cell` is sufficient.
struct Momentum {
    /// Mirror of `Config::scroll_accel` — the only Config field the
    /// momentum integrator reads. Narrowed so Momentum doesn't have to
    /// hold the whole (now `Vec<String>`-bearing) Config.
    scroll_accel: f64,
    /// Same persistent CGEventSource the Emitter holds. Aliased here
    /// (not retained separately) because the timer callback needs to
    /// post events but doesn't have the Emitter handy. Lifetime is the
    /// Emitter's — Drop invalidates the timer before releasing the
    /// source so the callback can't dangle.
    event_source: CGEventSourceRef,
    /// Most recent EMA velocity passed in via `scroll_inertia`, decayed
    /// each tick. Zero between coasts.
    vel_x_mm_per_sec: Cell<f64>,
    vel_y_mm_per_sec: Cell<f64>,
    /// Wall-clock time of the previous tick. Used to derive the per-tick
    /// integration step `dt`; tolerates jitter in timer scheduling.
    last_tick: Cell<Option<Timestamp>>,
    /// Fractional-pixel carry for momentum-phase events (separate from
    /// the active-scroll carry so a fresh coast doesn't inherit drift
    /// from the lift).
    carry_x_px: Cell<f64>,
    carry_y_px: Cell<f64>,
    /// Live timer ref while coasting. Null otherwise. Stored so
    /// `cancel()` can invalidate it.
    timer_ref: Cell<CFRunLoopTimerRef>,
    /// True after `MomentumPhase::Began` has been posted for the current
    /// coast; gates the corresponding `Ended` on cancel/stop.
    began_posted: Cell<bool>,
}

impl Emitter {
    pub fn new(cfg: Config) -> Self {
        let event_source = unsafe { CGEventSourceCreate(kCGEventSourceStateCombinedSessionState) };
        if event_source.is_null() {
            log::warn!(
                "CGEventSourceCreate(combinedSessionState) returned NULL; \
                 scroll bounce-back may not engage in WebKit / Chrome"
            );
        }
        let scroll_accel = cfg.scroll_accel;
        Self {
            cfg,
            event_source,
            last_click: Cell::new(None),
            click_count: Cell::new(0),
            scroll_carry_x_px: Cell::new(0.0),
            scroll_carry_y_px: Cell::new(0.0),
            scroll_last_time: Cell::new(None),
            scroll_redirected: Cell::new(false),
            momentum: Box::new(Momentum {
                scroll_accel,
                event_source,
                vel_x_mm_per_sec: Cell::new(0.0),
                vel_y_mm_per_sec: Cell::new(0.0),
                last_tick: Cell::new(None),
                carry_x_px: Cell::new(0.0),
                carry_y_px: Cell::new(0.0),
                timer_ref: Cell::new(std::ptr::null_mut()),
                began_posted: Cell::new(false),
            }),
            swipe_axis: Cell::new(None),
            swipe_generation: Arc::new(AtomicU64::new(0)),
            event_time: Cell::new(None),
            left_button_held: Cell::new(None),
            symbolic_swipe_active: Cell::new(false),
            symbolic_swipe_last_distance_mm: Cell::new(0.0),
            symbolic_swipe_pending_mm: Cell::new(0.0),
            symbolic_swipe_last_fire_at: Cell::new(None),
            symbolic_swipe_last_motion_at: Cell::new(None),
        }
    }

    /// Timestamp to stamp on the next emitted CGEvent. Returns the
    /// current frame's host-aligned scan time (set via
    /// [`Output::set_event_time`]) when invoked from within the
    /// gesture engine's `on_frame_at`; falls back to `Timestamp::now()`
    /// for out-of-band emits (inertia ticks, Drop cleanup).
    fn event_timestamp(&self) -> Timestamp {
        self.event_time.get().unwrap_or_else(Timestamp::now)
    }

    /// Use Apple's device-aware haptic performer. Unlike a Force Touch event,
    /// this API does not claim a pressure value or inject private HID data;
    /// macOS decides whether the current input device can provide feedback.
    fn perform_haptic(&self, kind: HapticKind) {
        if !self.cfg.haptic_feedback {
            return;
        }
        if objc2::MainThreadMarker::new().is_none() {
            log::debug!("haptic feedback skipped off the AppKit main thread");
            return;
        }
        let pattern = match kind {
            HapticKind::Click => NSHapticFeedbackPattern::Generic,
            HapticKind::DragEngaged => NSHapticFeedbackPattern::Alignment,
            HapticKind::GestureCommitted => NSHapticFeedbackPattern::LevelChange,
        };
        let performer = NSHapticFeedbackManager::defaultPerformer();
        performer
            .performFeedbackPattern_performanceTime(pattern, NSHapticFeedbackPerformanceTime::Now);
    }

    pub fn cursor(&self) -> CGPoint {
        // CGEventCreate(NULL) on a fresh event yields the current cursor
        // location via CGEventGetLocation.
        let Some(e) = Event::new() else {
            return CGPoint::new(0.0, 0.0);
        };
        unsafe { CGEventGetLocation(e.0) }
    }

    pub fn move_cursor_by(&self, dx_px: i32, dy_px: i32) {
        if dx_px == 0 && dy_px == 0 {
            return;
        }
        let from = self.cursor();
        let mut p = from;
        p.x += f64::from(dx_px);
        p.y += f64::from(dy_px);
        // If the proposed point lands off every display, clamp it to the
        // bounds of the source display so the *event location* sits exactly
        // on the edge. The auto-hidden full-screen menu bar reveals only
        // while the cursor sits at the menu-bar display's top edge (y ==
        // origin.y); a real input device hits that edge naturally because
        // the OS won't let it leave the screen, but a CGEvent posted at
        // y < origin.y just visually clamps the cursor without firing the
        // reveal. Same for Dock auto-reveal at the bottom edge. Delta fields
        // below stay at the user's requested value so apps see "pushing past
        // the edge" intent. When the proposed point lands on *any* display
        // (e.g. crossing to an adjacent monitor), post it as-is — clamping
        // to the source display's bounds would pin the cursor at the source
        // display's edge and prevent multi-monitor traversal entirely.
        // NB: `displays_with_point` returns a Vec preallocated to at least
        // length 1 even when no display matches — the real match count is
        // in the second tuple field. Don't use `ids.is_empty()` here.
        let on_a_display = CGDisplay::displays_with_point(p, 1)
            .map(|(_, count)| count > 0)
            .unwrap_or(false);
        if !on_a_display {
            let bounds = display_bounds_for(from);
            p.x =
                p.x.clamp(bounds.origin.x, bounds.origin.x + bounds.size.width - 1.0);
            p.y =
                p.y.clamp(bounds.origin.y, bounds.origin.y + bounds.size.height - 1.0);
        }
        let held = self.left_button_held.get().is_some();
        let event_type = if held {
            kCGEventLeftMouseDragged
        } else {
            kCGEventMouseMoved
        };
        let Some(e) = Event::from_raw(unsafe {
            CGEventCreateMouseEvent(self.event_source, event_type, p, kCGMouseButtonLeft)
        }) else {
            return;
        };
        e.set_int(kCGMouseEventDeltaX as u32, i64::from(dx_px));
        e.set_int(kCGMouseEventDeltaY as u32, i64::from(dy_px));
        unsafe { CGEventSetTimestamp(e.0, self.event_timestamp().as_nanos()) };
        log::trace!(
            "post: {} d=({:+},{:+})px to=({:.0},{:.0})",
            if held {
                "leftMouseDragged"
            } else {
                "mouseMoved"
            },
            dx_px,
            dy_px,
            p.x,
            p.y
        );
        e.post();
    }

    pub fn set_left_button_held(&self, held: bool) {
        self.set_button_held(held, true);
    }

    /// Post a drag-owned left-button edge without changing the click
    /// sequence used by ordinary taps and physical button presses.
    pub fn set_drag_button_held(&self, held: bool) {
        self.set_button_held(held, false);
    }

    fn set_button_held(&self, held: bool, count_as_click: bool) {
        let was_held = self.left_button_held.get().is_some();
        if was_held == held {
            return;
        }
        let p = self.cursor();
        let now = self.event_timestamp();
        // Physical-button presses chain into the same multi-click sequence
        // tap-derived clicks use (a tap quickly followed by a hardware
        // press counts as a double-click, etc.). Gesture-owned drags pass
        // `count_as_click = false` and always use click-state 1. Stash the
        // chosen count in `left_button_held` so the matching release can
        // stamp the same value, surviving any tap-derived `click()` that
        // mutates `click_count` mid-press.
        //
        // Release: replay the press's count verbatim — both halves of
        // one click carry the same count, matching macOS for natural
        // input.
        let (event_type, count) = if held {
            let c = if count_as_click {
                self.record_click(MouseButton::Left, now, p)
            } else {
                1
            };
            self.left_button_held.set(Some(c));
            (kCGEventLeftMouseDown, c)
        } else {
            let c = self.left_button_held.replace(None).unwrap_or(1);
            (kCGEventLeftMouseUp, c)
        };
        log::debug!(
            "post: leftMouse{} count={} at=({:.0},{:.0})",
            if held { "Down" } else { "Up" },
            count,
            p.x,
            p.y,
        );
        if let Some(e) = Event::from_raw(unsafe {
            CGEventCreateMouseEvent(self.event_source, event_type, p, kCGMouseButtonLeft)
        }) {
            e.set_int(kCGMouseEventClickState, count);
            unsafe { CGEventSetTimestamp(e.0, now.as_nanos()) };
            e.post();
        }
    }

    /// Decide the `kCGMouseEventClickState` for a fresh press of `button`
    /// at `p` and time `now`, applying the same time/distance windows
    /// macOS uses for natural input. Updates `last_click` and
    /// `click_count` as a side effect so the next call sees this press
    /// as the head of the sequence. Used by both the tap-derived
    /// `click()` and the hardware-button down path so they share one
    /// multi-click counter.
    fn record_click(&self, button: MouseButton, now: Timestamp, p: CGPoint) -> i64 {
        let count = match self.last_click.get() {
            Some((b, t, last_p))
                if b == button
                    && now.saturating_duration_since(t) < DOUBLE_CLICK_INTERVAL
                    && ((p.x - last_p.x).powi(2) + (p.y - last_p.y).powi(2)).sqrt()
                        < DOUBLE_CLICK_DISTANCE_PX =>
            {
                self.click_count.get() + 1
            }
            _ => 1,
        };
        self.click_count.set(count);
        self.last_click.set(Some((button, now, p)));
        count
    }

    pub fn click(&self, button: MouseButton) {
        let mods = unsafe { CGEventSourceFlagsState(kCGEventSourceStateCombinedSessionState) };
        self.click_with_modifiers(button, mods);
    }

    pub fn click_with_modifiers(&self, button: MouseButton, mods: u64) {
        let p = self.cursor();
        let now = self.event_timestamp();
        let count = self.record_click(button, now, p);

        let (down, up, raw_button) = match button {
            MouseButton::Left => (
                kCGEventLeftMouseDown,
                kCGEventLeftMouseUp,
                kCGMouseButtonLeft,
            ),
            MouseButton::Right => (
                kCGEventRightMouseDown,
                kCGEventRightMouseUp,
                kCGMouseButtonRight,
            ),
        };
        log::debug!(
            "post: click {:?} count={} mods=0x{mods:x} at=({:.0},{:.0})",
            button,
            count,
            p.x,
            p.y,
        );
        // Use the cached `now` from above (which is the frame
        // timestamp via `event_timestamp`) so the down + up CGEvents
        // and the `last_click` cache entry all share one time-base.
        let stamp_ns = now.as_nanos();
        if let Some(e) = Event::from_raw(unsafe {
            CGEventCreateMouseEvent(self.event_source, down, p, raw_button)
        }) {
            e.set_int(kCGMouseEventClickState, count);
            unsafe { CGEventSetTimestamp(e.0, stamp_ns) };
            e.post_to_with_modifiers(kCGHIDEventTap, mods);
        }
        if let Some(e) = Event::from_raw(unsafe {
            CGEventCreateMouseEvent(self.event_source, up, p, raw_button)
        }) {
            e.set_int(kCGMouseEventClickState, count);
            unsafe { CGEventSetTimestamp(e.0, stamp_ns) };
            e.post_to_with_modifiers(kCGHIDEventTap, mods);
        }
    }

    /// Phased smooth-pixel scroll. `phase` brackets the gesture so apps
    /// (Safari, Maps, etc.) can do rubber-banding and track the gesture
    /// as a continuous interaction rather than discrete wheel ticks.
    /// Per-frame mm is converted to mm/s via wall-clock dt so the
    /// acceleration curve runs on a frame-rate-independent velocity.
    pub fn scroll(&self, dx_mm: f64, dy_mm: f64, phase: Phase) {
        let sign = if self.cfg.natural_scroll { 1.0 } else { -1.0 };
        let now = self.event_timestamp();
        let mods = unsafe { CGEventSourceFlagsState(kCGEventSourceStateCombinedSessionState) };
        if matches!(phase, Phase::Began) {
            self.scroll_carry_x_px.set(0.0);
            self.scroll_carry_y_px.set(0.0);
            self.scroll_last_time.set(None);
            self.scroll_redirected
                .set(crate::scroll_policy::modifier_zoom_session_with_mask(
                    true,
                    mods,
                    self.scroll_redirected.get(),
                    self.cfg.modifier_zoom_mask,
                ));
        }

        if self.scroll_redirected.get() {
            // Command/Control + 2F slide: Convert the whole session to a
            // magnification stream, including Ended when the key was released
            // just before the fingers lifted.
            let mag_delta = -sign * dy_mm * 0.035;
            self.pinch(mag_delta, phase);
            return;
        }

        let (mapped_dx_mm, mapped_dy_mm) = crate::scroll_policy::shift_scroll_delta(
            mods,
            dx_mm,
            dy_mm,
            self.cfg.shift_scroll_horizontal,
        );
        // Apply the system's horizontal-scroll switch after any explicit
        // compatibility mapping so `horizontal = false` remains a hard
        // suppression for both native and Shift-remapped horizontal input.
        let eff_dx_mm = if self.cfg.horizontal_scroll {
            mapped_dx_mm
        } else {
            0.0
        };
        let eff_dy_mm = mapped_dy_mm;

        let prev_time = self.scroll_last_time.replace(Some(now));
        let dt = match prev_time {
            Some(t) => (now - t).as_secs_f64().clamp(0.001, 0.1),
            None => 1.0 / 60.0,
        };
        let vx = eff_dx_mm / dt;
        let vy = eff_dy_mm / dt;
        let dx_px = sign * accelerate_scroll(vx, self.cfg.scroll_accel) * dt;
        let dy_px = sign * accelerate_scroll(vy, self.cfg.scroll_accel) * dt;
        let total_x = self.scroll_carry_x_px.get() + dx_px;
        let total_y = self.scroll_carry_y_px.get() + dy_px;
        let int_x = total_x.trunc() as i32;
        let int_y = total_y.trunc() as i32;
        self.scroll_carry_x_px.set(total_x - int_x as f64);
        self.scroll_carry_y_px.set(total_y - int_y as f64);
        if matches!(phase, Phase::Changed) && int_x == 0 && int_y == 0 {
            return;
        }
        post_scroll_event(
            self.event_source,
            int_x,
            int_y,
            dx_px,
            dy_px,
            phase,
            /* momentum */ Phase::Cancelled,
            now,
        );
    }

    /// Seed inertia from the just-ended pan. Cancels any in-flight coast
    /// and starts a new one driven by a CFRunLoopTimer.
    pub fn scroll_inertia(&self, vx_mm_per_sec: f64, vy_mm_per_sec: f64) {
        if !self.cfg.momentum_scroll {
            log::debug!("scroll: momentum disabled by system/config policy");
            return;
        }
        if self.scroll_redirected.replace(false) {
            log::debug!("scroll: inertia suppressed for modifier-zoom session");
            return;
        }
        let vx_mm_per_sec = if self.cfg.horizontal_scroll {
            vx_mm_per_sec
        } else {
            0.0
        };
        let sign = if self.cfg.natural_scroll { 1.0 } else { -1.0 };
        // Apply direction sign here so the Momentum struct doesn't have
        // to know about natural_scroll — it just integrates a velocity.
        self.momentum
            .start(sign * vx_mm_per_sec, sign * vy_mm_per_sec);
    }

    /// Cancel any in-flight inertia. Returns `true` if a coast was
    /// active (so the caller knows the touch was cancelling a fling).
    pub fn cancel_inertia(&self) -> bool {
        self.momentum.cancel()
    }

    /// Emit a pinch (magnify) gesture. `delta` is the *change* in scale
    /// since the last event (e.g. 0.05 = 5% bigger). Phase brackets are
    /// required for apps to track the gesture.
    pub fn pinch(&self, delta: f64, phase: Phase) {
        let p = self.cursor();
        let ts = self.event_timestamp();
        if let Some(e) = synthesize_gesture_event(
            GESTURE_SUBTYPE_MAGNIFY,
            iohid_gesture_phase(phase),
            GesturePayload::Magnification(delta as f32),
            ts,
        ) {
            unsafe { CGEventSetLocation(e.0, p) };
            log::trace!(
                "post: pinch {:?} delta={:+.4} at=({:.0},{:.0})",
                phase,
                delta,
                p.x,
                p.y
            );
            // Gesture events are delivered through the session tap. Posting
            // the same object to both taps duplicates the responder call in
            // some apps and can make one pinch look like two.
            e.post_to(kCGSessionEventTap);
        }
    }

    /// Emit a rotate gesture. `delta_degrees` is the *change* in rotation
    /// since the last event, positive = counterclockwise (matching
    /// NSEvent.rotation semantics).
    pub fn rotate(&self, delta_degrees: f64, phase: Phase) {
        let ts = self.event_timestamp();
        let p = self.cursor();
        if let Some(e) = synthesize_gesture_event(
            GESTURE_SUBTYPE_ROTATE,
            iohid_gesture_phase(phase),
            GesturePayload::Rotation(delta_degrees as f32),
            ts,
        ) {
            unsafe { CGEventSetLocation(e.0, p) };
            e.post_to(kCGSessionEventTap);
        }
        log::trace!(
            "post: rotate {:?} delta={:+.2}deg at=({:.0},{:.0})",
            phase,
            delta_degrees,
            p.x,
            p.y
        );
    }

    /// Drive a 3F/4F swipe live, mirroring the per-frame motion the
    /// gesture engine sees. macOS routes these through the Dock as
    /// `kCGSEventDockControl` events (field 55 = 30) carrying
    /// `kIOHIDEventTypeDockSwipe` (110 = 23). Both horizontal Spaces
    /// and vertical-axis swipes use the same envelope; only the
    /// motion-axis field (123) and the sign of progress differ.
    ///
    /// The Dock plays its rubber-band animation in response to the
    /// Changed event stream at wall-clock pacing — a degenerate
    /// Begin→End pair commits without animating. The user can also
    /// reverse direction or release short to abort, just like a real
    /// trackpad, because progress is driven by their actual finger
    /// position.
    ///
    /// Input sign convention (gesture-engine): positive `signed_progress`
    /// = finger centroid moved in the +X (right) / +Y (down) direction
    /// from the gesture's start. The Dock's wire convention is the
    /// opposite on both axes — measured at the keyboard, fingers-right
    /// matches a negative horizontal `origin_offset` and fingers-up
    /// (Mission Control) matches a positive vertical `origin_offset` —
    /// so this function negates both before sending.
    pub fn swipe(
        &self,
        axis: SwipeAxis,
        signed_progress: f64,
        velocity_mm_per_sec: f64,
        phase: Phase,
    ) {
        let backend = match axis {
            SwipeAxis::Horizontal => self.cfg.horizontal_swipe.backend,
            SwipeAxis::Vertical => self.cfg.vertical_swipe.backend,
        };
        match (backend, axis) {
            (SwipeBackend::Off, _) | (SwipeBackend::Notification, SwipeAxis::Horizontal) => {
                log::trace!(
                    "post: swipe {:?} {:?} suppressed (backend={:?})",
                    axis,
                    phase,
                    backend,
                );
            }
            (SwipeBackend::Notification, SwipeAxis::Vertical) => {
                self.swipe_notification(signed_progress, phase);
            }
            (SwipeBackend::Synthetic, _) => {
                self.swipe_synthetic(axis, signed_progress, velocity_mm_per_sec, phase);
            }
        }
    }

    /// Live-animated DockSwipe synthesis. Posts an event pair per
    /// gesture-engine frame; user can reverse or abort mid-gesture.
    /// See [`post_dock_swipe_pair`] for the event shape and attribution.
    fn swipe_synthetic(
        &self,
        axis: SwipeAxis,
        signed_progress: f64,
        velocity_mm_per_sec: f64,
        phase: Phase,
    ) {
        if matches!(phase, Phase::Began) {
            self.swipe_generation.fetch_add(1, Ordering::Relaxed);
        }
        // macOS 26 can silently reject a third-party DockSwipe when it is
        // carrying a synthesized mouse drag. Mac Mouse Fix's tested
        // compatibility path uses WindowServer SymbolicHotKeys for this
        // exact modified-drag case. Keep standalone 4F swipes on the native
        // continuous path, but make the 3F-drag handoff reliable and avoid a
        // second Space switch if DockSwipe happens to be accepted.
        if !macos_27_or_later()
            && self.left_button_held.get().is_some()
            && !self.symbolic_swipe_active.get()
        {
            self.swipe_axis.set(None);
            self.swipe_symbolic_fallback(axis, signed_progress, velocity_mm_per_sec, phase);
            return;
        }
        // macOS 27+ rejects third-party DockSwipe payloads, so route that
        // release through the system's symbolic hot-key registry. macOS 26
        // keeps the original continuous path for standalone swipes. On 27+,
        // use SkyLight's SLEventSetIOHIDEvent when available and fall back to
        // a hot-key if the private setter is unavailable. The left-button
        // state is untouched in either case, so a three-finger window drag
        // remains attached while Spaces changes.
        // Keep trying the continuous DockSwipe/HIDEvent path until it
        // actually fails. On macOS 27+ the first frame must reach
        // `post_dock_swipe_pair`; only its false return arms the symbolic
        // fallback for the rest of this gesture. Standalone macOS 26 swipes
        // never enter this branch; carried drags are routed above.
        if self.symbolic_swipe_active.get() {
            // SymbolicHotKey gestures have no DockSwipe stream to cancel on
            // Drop. Clear any legacy axis left behind by a failed macOS 27
            // HIDEvent attempt before handing the phase to the fallback.
            if matches!(phase, Phase::Began | Phase::Ended | Phase::Cancelled) {
                self.swipe_axis.set(None);
            }
            self.swipe_symbolic_fallback(axis, signed_progress, velocity_mm_per_sec, phase);
            return;
        }
        // Gesture-engine convention: positive `signed_progress` = finger
        // centroid moved +X (right) / +Y (down) since gesture start.
        // Dock's wire convention is the opposite on both axes (measured
        // at the keyboard); negate to match. Flip a branch if a swipe
        // goes the wrong direction.
        let origin_offset = -signed_progress;
        let motion = match axis {
            SwipeAxis::Horizontal => SWIPE_MOTION_HORIZONTAL,
            SwipeAxis::Vertical => SWIPE_MOTION_VERTICAL,
        };
        let dock_phase = match phase {
            Phase::Began => kCGSGesturePhaseBegan,
            Phase::Changed => kCGSGesturePhaseChanged,
            Phase::Ended => kCGSGesturePhaseEnded,
            Phase::Cancelled => kCGSGesturePhaseCancelled,
        };
        match phase {
            Phase::Began => {
                self.swipe_axis.set(Some(axis));
            }
            Phase::Ended | Phase::Cancelled => self.swipe_axis.set(None),
            Phase::Changed => {}
        }
        // Velocity is meaningful at lift only; cap to keep fast lifts
        // from looking abrupt vs. real-trackpad feel. See
        // SWIPE_END_VELOCITY_MAX comment.
        let velocity = matches!(phase, Phase::Ended | Phase::Cancelled).then(|| {
            // Progress is inverted above to match Dock coordinates, so the
            // exit velocity must be inverted as well. Keep it on the same
            // axis as progress; putting the same scalar in X and Y makes
            // diagonal release velocity and rebound direction wrong.
            let v = (-velocity_mm_per_sec).clamp(-SWIPE_END_VELOCITY_MAX, SWIPE_END_VELOCITY_MAX);
            match axis {
                SwipeAxis::Horizontal => (v, 0.0),
                SwipeAxis::Vertical => (0.0, v),
            }
        });
        log::debug!(
            "post: swipe (synthetic) axis={:?} motion={} progress={:+.3} origin_offset={:+.3} v={:+.1} phase={:?}",
            axis,
            motion,
            signed_progress,
            origin_offset,
            velocity_mm_per_sec,
            phase,
        );
        let posted = post_dock_swipe_pair(
            self.event_source,
            motion,
            dock_phase,
            origin_offset,
            velocity,
            self.event_timestamp(),
        );
        if posted && !macos_27_or_later() && matches!(phase, Phase::Ended) {
            schedule_dock_swipe_resends(
                self.event_source,
                motion,
                origin_offset,
                velocity,
                Arc::clone(&self.swipe_generation),
                self.swipe_generation.load(Ordering::Relaxed),
            );
        }
        if !posted && macos_27_or_later() {
            // Keep a usable cross-Space drag when a future 27.x build moves
            // or removes the private HID setter. The Began call initializes
            // the symbolic state, and subsequent frames remain on that path
            // for the rest of this gesture.
            log::warn!(
                "DockSwipe HIDEvent path unavailable; switching this gesture to SymbolicHotKey fallback"
            );
            self.swipe_axis.set(None);
            self.swipe_symbolic_fallback(axis, signed_progress, velocity_mm_per_sec, phase);
            return;
        }
        // A shortcut fallback cannot be sent unconditionally: when Dock
        // accepts the synthetic stream it would switch two Spaces. Keep it
        // opt-in for diagnosing an OS version that rejects DockSwipe.
        if std::env::var_os("MTC_ENABLE_SPACE_KEY_FALLBACK").is_some()
            && matches!(axis, SwipeAxis::Horizontal)
            && matches!(phase, Phase::Ended)
            && signed_progress.abs() >= SWIPE_VERTICAL_COMMIT_PROGRESS
        {
            let key = if signed_progress < 0.0 { 124 } else { 123 };
            log::warn!(
                "post: horizontal swipe shortcut fallback enabled (key={key}); DockSwipe may already have committed"
            );
            post_ctrl_arrow_key(self.event_source, key, self.event_timestamp());
        }
    }

    /// Translate a continuous 3F/4F swipe into paced WindowServer symbolic
    /// hot-key presses on macOS 27 or when the macOS 27+ HID path is
    /// unavailable. This is the same compatibility strategy
    /// used by Mac Mouse Fix PR #1875: the system performs the Space/Expose
    /// transition while our synthesized mouse button remains held.
    fn swipe_symbolic_fallback(
        &self,
        axis: SwipeAxis,
        signed_progress: f64,
        velocity_mm_per_sec: f64,
        phase: Phase,
    ) {
        let now = self.event_timestamp();
        let distance_mm = (signed_progress * SWIPE_PROGRESS_REF_MM).clamp(-500.0, 500.0);
        match phase {
            Phase::Began => {
                self.symbolic_swipe_active.set(true);
                // `signed_progress` is already cumulative from the gesture
                // origin. Seed the accumulator from zero so the first frame
                // is not discarded when axis-lock happens at 3mm or more.
                self.symbolic_swipe_last_distance_mm.set(0.0);
                self.symbolic_swipe_pending_mm.set(0.0);
                self.symbolic_swipe_last_fire_at.set(None);
                self.symbolic_swipe_last_motion_at.set(None);
                log::debug!(
                    "swipe: SymbolicHotKey fallback began axis={axis:?} distance={distance_mm:+.1}mm drag_held={}",
                    self.left_button_held.get().is_some()
                );
                self.accumulate_symbolic_swipe(axis, distance_mm, now);
                self.symbolic_swipe_last_distance_mm.set(distance_mm);
            }
            Phase::Changed => {
                if !self.symbolic_swipe_active.get() {
                    // Be tolerant of a recorder or transport that starts at
                    // Changed; a later Ended still needs a clean reset.
                    self.symbolic_swipe_active.set(true);
                    self.symbolic_swipe_last_distance_mm.set(0.0);
                    self.symbolic_swipe_pending_mm.set(0.0);
                    self.accumulate_symbolic_swipe(axis, distance_mm, now);
                    self.symbolic_swipe_last_distance_mm.set(distance_mm);
                    return;
                }
                let previous = self.symbolic_swipe_last_distance_mm.replace(distance_mm);
                let delta = distance_mm - previous;
                if !delta.is_finite() {
                    return;
                }
                if delta.abs() > 1e-4 {
                    self.symbolic_swipe_last_motion_at.set(Some(now));
                }
                self.accumulate_symbolic_swipe(axis, delta, now);
            }
            Phase::Ended => {
                if self.symbolic_swipe_active.get() {
                    let previous = self.symbolic_swipe_last_distance_mm.replace(distance_mm);
                    let delta = distance_mm - previous;
                    if delta.is_finite() {
                        if delta.abs() > 1e-4 {
                            self.symbolic_swipe_last_motion_at.set(Some(now));
                        }
                        self.accumulate_symbolic_swipe(axis, delta, now);
                    }
                    // A fast lift can commit a switch before the distance
                    // threshold, matching Mac Mouse Fix's flick handling.
                    if !self.symbolic_switch_in_cooldown(now)
                        && self.symbolic_swipe_pending_mm.get().abs()
                            < self.symbolic_threshold_mm(axis)
                        && velocity_mm_per_sec.abs() >= SYMBOLIC_FLICK_VELOCITY_MM_PER_SEC
                        && self.symbolic_swipe_last_motion_at.get().is_some_and(|at| {
                            now.saturating_duration_since(at) <= Duration::from_millis(80)
                        })
                    {
                        self.fire_symbolic_swipe(axis, velocity_mm_per_sec.signum(), now);
                    }
                }
                self.reset_symbolic_swipe();
            }
            Phase::Cancelled => {
                self.reset_symbolic_swipe();
            }
        }
    }

    fn symbolic_threshold_mm(&self, axis: SwipeAxis) -> f64 {
        match axis {
            SwipeAxis::Horizontal => SYMBOLIC_SPACE_THRESHOLD_MM,
            SwipeAxis::Vertical => SYMBOLIC_EXPOSE_THRESHOLD_MM,
        }
    }

    fn symbolic_switch_in_cooldown(&self, now: Timestamp) -> bool {
        self.symbolic_swipe_last_fire_at
            .get()
            .is_some_and(|at| now.saturating_duration_since(at) < SYMBOLIC_SWITCH_COOLDOWN)
    }

    fn accumulate_symbolic_swipe(&self, axis: SwipeAxis, delta_mm: f64, now: Timestamp) {
        if self.symbolic_switch_in_cooldown(now) {
            // Do not bank motion while the Space animation is running. This
            // prevents one long finger slide from skipping several desktops.
            self.symbolic_swipe_pending_mm.set(0.0);
            return;
        }
        let pending = (self.symbolic_swipe_pending_mm.get() + delta_mm).clamp(-100.0, 100.0);
        self.symbolic_swipe_pending_mm.set(pending);
        let threshold = self.symbolic_threshold_mm(axis);
        if pending.abs() >= threshold {
            self.fire_symbolic_swipe(axis, pending.signum(), now);
            self.symbolic_swipe_pending_mm.set(0.0);
        }
    }

    fn fire_symbolic_swipe(&self, axis: SwipeAxis, direction: f64, now: Timestamp) {
        let hotkey = match axis {
            // Finger-left (negative X) advances to the Space on the right;
            // this matches the existing DockSwipe sign convention.
            SwipeAxis::Horizontal if direction < 0.0 => SHK_SPACE_RIGHT,
            SwipeAxis::Horizontal => SHK_SPACE_LEFT,
            // Finger-up (negative Y) opens Mission Control; finger-down
            // opens App Exposé, matching the documented 4F gestures.
            SwipeAxis::Vertical if direction < 0.0 => SHK_EXPOSE_ALL_WINDOWS,
            SwipeAxis::Vertical => SHK_EXPOSE_APPLICATION_WINDOWS,
        };
        if post_symbolic_hotkey(self.event_source, hotkey, now) {
            self.symbolic_swipe_last_fire_at.set(Some(now));
            log::debug!(
                "swipe: SymbolicHotKey {hotkey} fired axis={axis:?} direction={direction:+.0} drag_held={}",
                self.left_button_held.get().is_some()
            );
        }
    }

    fn reset_symbolic_swipe(&self) {
        self.symbolic_swipe_active.set(false);
        self.symbolic_swipe_last_distance_mm.set(0.0);
        self.symbolic_swipe_pending_mm.set(0.0);
        self.symbolic_swipe_last_fire_at.set(None);
        self.symbolic_swipe_last_motion_at.set(None);
    }

    /// `CoreDockSendNotification` path: discrete commit on lift past a
    /// threshold. Vertical-only — there's no Dock notification for
    /// horizontal Space switching.
    fn swipe_notification(&self, signed_progress: f64, phase: Phase) {
        if !matches!(phase, Phase::Ended) || signed_progress.abs() < SWIPE_VERTICAL_COMMIT_PROGRESS
        {
            log::trace!(
                "post: vertical swipe (notification) {:?} progress={:+.3} (no-op until Ended past ±{})",
                phase,
                signed_progress,
                SWIPE_VERTICAL_COMMIT_PROGRESS,
            );
            return;
        }
        let (notif, label) = if signed_progress < 0.0 {
            (DOCK_NOTIF_MISSION_CONTROL, "Mission Control")
        } else {
            (DOCK_NOTIF_APP_EXPOSE, "App Exposé")
        };
        log::debug!(
            "post: vertical swipe → {} via CoreDockSendNotification (progress={:+.3})",
            label,
            signed_progress,
        );
        send_dock_notification(notif);
    }
}

fn post_ctrl_arrow_key(source: CGEventSourceRef, keycode: u16, ts: Timestamp) {
    // This is an explicit compatibility shortcut (Control + Arrow), not an
    // App-facing pointer event. Keep the exact chord stable: adding a
    // user's unrelated Shift/Option/Command state makes WindowServer reject
    // the fallback on some macOS releases.
    const flags: u64 = crate::scroll_policy::MOD_CTRL;
    if let Some(down) =
        Event::from_raw(unsafe { CGEventCreateKeyboardEvent(source, keycode, true) })
    {
        unsafe {
            CGEventSetFlags(down.0, flags);
            CGEventSetTimestamp(down.0, ts.as_nanos());
        }
        down.post_raw(kCGHIDEventTap);
    }
    if let Some(up) = Event::from_raw(unsafe { CGEventCreateKeyboardEvent(source, keycode, false) })
    {
        unsafe {
            CGEventSetFlags(up.0, flags);
            CGEventSetTimestamp(up.0, ts.as_nanos());
        }
        up.post_raw(kCGHIDEventTap);
    }
}

fn post_cmd_ctrl_key(source: CGEventSourceRef, keycode: u16, ts: Timestamp) {
    const kVK_Command: u16 = 55;
    const kVK_Control: u16 = 59;
    const kCGEventFlagMaskCommand: u64 = 0x00100000;
    const kCGEventFlagMaskControl: u64 = 0x00040000;
    let flags = kCGEventFlagMaskCommand | kCGEventFlagMaskControl;

    let post_key = |code: u16, down: bool, f: u64, offset_ms: u64| {
        if let Some(ev) = Event::from_raw(unsafe { CGEventCreateKeyboardEvent(source, code, down) })
        {
            let ev_ts = ts + Duration::from_millis(offset_ms);
            unsafe {
                CGEventSetFlags(ev.0, f);
                CGEventSetTimestamp(ev.0, ev_ts.as_nanos());
            }
            ev.post_raw(kCGSessionEventTap);
            ev.post_raw(kCGHIDEventTap);
        }
    };

    post_key(kVK_Command, true, kCGEventFlagMaskCommand, 0);
    post_key(kVK_Control, true, flags, 1);
    post_key(keycode, true, flags, 2);
    post_key(keycode, false, flags, 15);
    post_key(kVK_Control, false, kCGEventFlagMaskCommand, 16);
    post_key(kVK_Command, false, 0, 17);
}

/// Trigger a system symbolic hotkey via SkyLight / CoreGraphicsServices.
///
/// Symbolic hotkeys are internal WindowServer action IDs:
/// - 163: Show Notification Center
/// - 32: Mission Control
/// - 33: Application Windows (Exposé)
/// - 36: Show Desktop
/// - 160: Launchpad
/// - 70: Look Up
///
/// Matches Mac Mouse Fix `SymbolicHotKeys.m` reverse-engineered implementation.
fn post_symbolic_hotkey(source: CGEventSourceRef, shk: u32, ts: Timestamp) -> bool {
    type CGSSetSymbolicHotKeyEnabledFn = unsafe extern "C" fn(hotkey: u32, enabled: bool) -> i32;
    type CGSGetSymbolicHotKeyValueFn = unsafe extern "C" fn(
        hotkey: u32,
        out_keq: *mut u16,
        out_vkc: *mut u16,
        out_mods: *mut u64,
    ) -> i32;
    type CGSSetSymbolicHotKeyValueFn =
        unsafe extern "C" fn(hotkey: u32, keq: u16, vkc: u16, mods: u64) -> i32;

    use std::sync::OnceLock;

    static SYMBOLS: OnceLock<
        Option<(
            CGSSetSymbolicHotKeyEnabledFn,
            CGSGetSymbolicHotKeyValueFn,
            CGSSetSymbolicHotKeyValueFn,
        )>,
    > = OnceLock::new();

    let syms = SYMBOLS.get_or_init(|| {
        let frameworks = [
            c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight",
            c"/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices",
            c"/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics",
        ];
        for fw in frameworks {
            let handle = unsafe { libc::dlopen(fw.as_ptr(), libc::RTLD_LAZY) };
            if handle.is_null() {
                continue;
            }
            let get_fn = unsafe { libc::dlsym(handle, c"CGSGetSymbolicHotKeyValue".as_ptr()) };
            let set_en_fn = unsafe { libc::dlsym(handle, c"CGSSetSymbolicHotKeyEnabled".as_ptr()) };
            let set_val_fn = unsafe { libc::dlsym(handle, c"CGSSetSymbolicHotKeyValue".as_ptr()) };

            if !get_fn.is_null() && !set_en_fn.is_null() && !set_val_fn.is_null() {
                return Some((
                    unsafe { std::mem::transmute(set_en_fn) },
                    unsafe { std::mem::transmute(get_fn) },
                    unsafe { std::mem::transmute(set_val_fn) },
                ));
            }
        }
        None
    });

    let Some((set_en, get_val, set_val)) = *syms else {
        log::warn!(
            "symbolic hotkey {shk} unavailable: SkyLight/CoreGraphics symbols could not be resolved"
        );
        return false;
    };

    const SHK_MODS: u64 = (1 << 21) | (1 << 23); // kCGSNumericPadKeyMask | kCGSFunctionKeyMask

    let (vkc_to_send, mods_to_send, has_native_binding) = {
        let mut keq: u16 = 65535;
        let mut vkc: u16 = 65535;
        let mut mods: u64 = 0;

        let err = unsafe { get_val(shk, &mut keq, &mut vkc, &mut mods) };
        if err == 0 && vkc != 65535 && vkc != 0 {
            let enable_err = unsafe { set_en(shk, true) };
            if enable_err != 0 {
                log::warn!("symbolic hotkey {shk} could not be enabled: {enable_err}");
                return false;
            }
            (vkc, mods, true)
        } else {
            // The registry can be missing a binding after a clean install.
            // Allocate the same out-of-reach virtual-key range used by Mac
            // Mouse Fix. Real hardware keycodes are below 128; using an F-key
            // slot here could trigger a user's physical shortcut as well as
            // the symbolic action.
            let vkc_out_of_reach = 400u16.saturating_add(shk as u16);
            unsafe {
                let enable_err = set_en(shk, true);
                let value_err = set_val(shk, 65535, vkc_out_of_reach, SHK_MODS);
                if enable_err != 0 || value_err != 0 {
                    log::warn!(
                        "symbolic hotkey {shk} binding failed: enable={enable_err} value={value_err}"
                    );
                    return false;
                }
            }
            (vkc_out_of_reach, SHK_MODS, false)
        }
    };

    let post_key = |code: u16, down: bool, offset_ms: u64| -> bool {
        // Pass NULL (std::ptr::null_mut()) so the event is created from the default system source, matching Mac Mouse Fix
        let raw = unsafe {
            let p = CGEventCreateKeyboardEvent(std::ptr::null_mut(), code, down);
            if p.is_null() {
                CGEventCreateKeyboardEvent(source, code, down)
            } else {
                p
            }
        };
        let Some(ev) = Event::from_raw(raw) else {
            return false;
        };
        let ev_ts = ts + Duration::from_millis(offset_ms);
        // The key-down carries the symbolic binding's modifiers. For
        // key-up, retain the flags that CoreGraphics put on the fresh
        // event so a user's live modifiers are not left latched by the
        // synthetic shortcut.
        let original_flags = unsafe { CGEventGetFlags(ev.0) };
        let flags = if down { mods_to_send } else { original_flags };
        unsafe {
            CGEventSetFlags(ev.0, flags);
            CGEventSetTimestamp(ev.0, ev_ts.as_nanos());
        }
        // `mods_to_send` comes from the WindowServer symbolic-hot-key
        // registry. Use one session-tap post, matching Mac Mouse Fix;
        // posting the same key to HID/annotated taps can execute a
        // Space/Exposé action more than once.
        ev.post_raw(kCGSessionEventTap);
        true
    };

    let sent_down = post_key(vkc_to_send, true, 0);
    let sent_up = post_key(vkc_to_send, false, 16);
    log::debug!(
        "post: symbolic hotkey {shk} sent via vkc={vkc_to_send} mods=0x{mods_to_send:x} (native_binding={has_native_binding})"
    );
    sent_down && sent_up
}

/// Function pointer signature for `CoreDockSendNotification`. Symbol
/// is exported from the dyld shared cache (no on-disk framework
/// binary). Resolved on first use via `dlsym(RTLD_DEFAULT, …)` and
/// cached. Returns 0 on success, nonzero CGError otherwise.
type CoreDockSendNotificationFn =
    unsafe extern "C" fn(notification: *const c_void, unknown: i32) -> i32;

/// Send a notification name to the Dock via the private
/// `CoreDockSendNotification` function — same path Hammerspoon's
/// `hs.spaces.toggleMissionControl` and `toggleAppExpose` use. Logs
/// and no-ops if the symbol can't be resolved or the call fails.
fn send_dock_notification(name: &str) -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use std::sync::OnceLock;

    static FN_PTR: OnceLock<Option<CoreDockSendNotificationFn>> = OnceLock::new();
    let f = FN_PTR.get_or_init(|| {
        // RTLD_DEFAULT only sees symbols from libraries already mapped
        // into the process. We don't link anything that drags this in,
        // so dlopen it explicitly first. Confirmed via
        // `dyld_info -all_dyld_cache -exports`: the symbol lives in
        // ApplicationServices.framework/Frameworks/HIServices.framework,
        // not SkyLight as the framework name might suggest. The
        // framework is in the dyld shared cache (no on-disk binary on
        // modern macOS); dlopen knows to look there for canonical
        // /System paths.
        let framework = c"/System/Library/Frameworks/ApplicationServices.framework/Frameworks/HIServices.framework/HIServices";
        let handle = unsafe { libc::dlopen(framework.as_ptr(), libc::RTLD_LAZY) };
        if handle.is_null() {
            let err = unsafe {
                let p = libc::dlerror();
                if p.is_null() {
                    "(no error)".to_string()
                } else {
                    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
                }
            };
            log::warn!(
                "dlopen SkyLight.framework failed ({err}); \
                 vertical swipe → Mission Control / Exposé will not fire"
            );
            return None;
        }
        let symbol = c"CoreDockSendNotification";
        let raw = unsafe { libc::dlsym(handle, symbol.as_ptr()) };
        if raw.is_null() {
            log::warn!(
                "CoreDockSendNotification not found in HIServices.framework; \
                 vertical swipe → Mission Control / Exposé will not fire"
            );
            None
        } else {
            // SAFETY: the symbol exists and matches the documented
            // signature (verified via Hammerspoon's
            // extensions/spaces/private.h).
            Some(unsafe { std::mem::transmute::<*mut c_void, CoreDockSendNotificationFn>(raw) })
        }
    });
    let Some(f) = f else {
        return false;
    };
    let cf_name = CFString::new(name);
    let rc = unsafe { f(cf_name.as_concrete_TypeRef() as *const c_void, 0) };
    if rc != 0 {
        log::warn!("CoreDockSendNotification({name:?}, 0) returned {rc}");
        false
    } else {
        true
    }
}

type SLEventSetIOHIDEventFn = unsafe extern "C" fn(CGEventRef, *const c_void);

const HID_EVENT_TYPE_VELOCITY: u32 = 9;
const HID_EVENT_TYPE_DOCK_SWIPE: u32 = 23;
const HID_GESTURE_FLAVOR_DOCK_PRIMARY: i64 = 3;
const HID_PHASE_SHIFT: u32 = 24;
const HID_PHASE_MASK: u32 = 0xff;

fn hid_field(ty: u32, index: u32) -> u32 {
    (ty << 16) | index
}

fn macos_major_version() -> Option<u32> {
    let mut bytes = [0u8; 64];
    let mut len = bytes.len();
    let rc = unsafe {
        libc::sysctlbyname(
            c"kern.osproductversion".as_ptr(),
            bytes.as_mut_ptr() as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len == 0 {
        return None;
    }
    std::str::from_utf8(&bytes[..len.min(bytes.len())])
        .ok()?
        .trim_end_matches('\0')
        .split('.')
        .next()?
        .parse()
        .ok()
}

fn macos_27_or_later() -> bool {
    use std::sync::OnceLock;
    static RESULT: OnceLock<bool> = OnceLock::new();
    // An unknown version is treated as modern so we never write the legacy
    // opaque CGEvent shape on a system whose ABI we failed to identify.
    *RESULT.get_or_init(|| macos_major_version().map_or(true, |major| major >= 27))
}

fn sky_light_set_hid_event() -> Option<SLEventSetIOHIDEventFn> {
    use std::sync::OnceLock;
    static FN_PTR: OnceLock<Option<SLEventSetIOHIDEventFn>> = OnceLock::new();
    *FN_PTR.get_or_init(|| {
        let path = c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight";
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            log::warn!("SkyLight.framework unavailable; macOS 27 DockSwipe payload disabled");
            return None;
        }
        let symbol = c"SLEventSetIOHIDEvent";
        let raw = unsafe { libc::dlsym(handle, symbol.as_ptr()) };
        if raw.is_null() {
            log::warn!("SkyLight SLEventSetIOHIDEvent unavailable; macOS 27 DockSwipe disabled");
            None
        } else {
            Some(unsafe { std::mem::transmute::<*mut c_void, SLEventSetIOHIDEventFn>(raw) })
        }
    })
}

unsafe fn objc_send0(target: *mut c_void, selector: *mut c_void) {
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { send(target, selector) };
}

unsafe fn objc_send_init(
    target: *mut c_void,
    selector: *mut c_void,
    ty: u32,
    timestamp: u64,
    sender_id: u64,
) -> *mut c_void {
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void, u32, u64, u64) -> *mut c_void =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { send(target, selector, ty, timestamp, sender_id) }
}

unsafe fn objc_send_options(target: *mut c_void, selector: *mut c_void, value: u32) {
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void, u32) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { send(target, selector, value) };
}

unsafe fn objc_send_integer(target: *mut c_void, selector: *mut c_void, field: u32, value: i64) {
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void, i64, u32) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { send(target, selector, value, field) };
}

unsafe fn objc_send_double(target: *mut c_void, selector: *mut c_void, field: u32, value: f64) {
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void, f64, u32) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { send(target, selector, value, field) };
}

unsafe fn objc_send_append(target: *mut c_void, selector: *mut c_void, child: *mut c_void) {
    let send: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { send(target, selector, child) };
}

/// Attach the macOS 27 HIDEvent representation to a DockSwipe CGEvent.
/// Returns false when the current OS cannot support this path; callers must
/// then fail closed rather than writing the pre-27 opaque CGEvent offsets.
fn attach_dock_hid_event(
    event: CGEventRef,
    phase: i64,
    motion: i64,
    progress: f64,
    velocity: Option<(f64, f64)>,
    timestamp: u64,
) -> bool {
    // Try the stable SkyLight setter on every macOS release. It is present
    // on older versions too and avoids depending on opaque CGEvent offsets;
    // the caller only permits legacy fields as a pre-27 fallback.
    let Some(set_hid_event) = sky_light_set_hid_event() else {
        return false;
    };
    let class = unsafe { objc_getClass(c"HIDEvent".as_ptr()) };
    if class.is_null() {
        log::warn!("HIDEvent Objective-C class unavailable; DockSwipe disabled");
        return false;
    }
    let alloc = unsafe {
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        send(class, sel_registerName(c"alloc".as_ptr()))
    };
    if alloc.is_null() {
        return false;
    }
    let hid = unsafe {
        objc_send_init(
            alloc,
            sel_registerName(c"initWithType:timestamp:senderID:".as_ptr()),
            HID_EVENT_TYPE_DOCK_SWIPE,
            timestamp,
            0,
        )
    };
    if hid.is_null() {
        return false;
    }
    unsafe {
        objc_send_options(
            hid,
            sel_registerName(c"setOptions:".as_ptr()),
            ((phase as u32) & HID_PHASE_MASK) << HID_PHASE_SHIFT,
        );
        objc_send_integer(
            hid,
            sel_registerName(c"setIntegerValue:forField:".as_ptr()),
            hid_field(HID_EVENT_TYPE_DOCK_SWIPE, 1),
            motion,
        );
        objc_send_integer(
            hid,
            sel_registerName(c"setIntegerValue:forField:".as_ptr()),
            hid_field(HID_EVENT_TYPE_DOCK_SWIPE, 5),
            HID_GESTURE_FLAVOR_DOCK_PRIMARY,
        );
        objc_send_double(
            hid,
            sel_registerName(c"setDoubleValue:forField:".as_ptr()),
            hid_field(HID_EVENT_TYPE_DOCK_SWIPE, 2),
            progress,
        );
    }

    if matches!(phase, kCGSGesturePhaseEnded | kCGSGesturePhaseCancelled) {
        if let Some((vx, vy)) = velocity {
            let alloc_velocity = unsafe {
                let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                send(class, sel_registerName(c"alloc".as_ptr()))
            };
            let velocity_event = unsafe {
                objc_send_init(
                    alloc_velocity,
                    sel_registerName(c"initWithType:timestamp:senderID:".as_ptr()),
                    HID_EVENT_TYPE_VELOCITY,
                    timestamp,
                    0,
                )
            };
            if !velocity_event.is_null() {
                unsafe {
                    let setter = sel_registerName(c"setDoubleValue:forField:".as_ptr());
                    objc_send_double(
                        velocity_event,
                        setter,
                        hid_field(HID_EVENT_TYPE_VELOCITY, 0),
                        vx,
                    );
                    objc_send_double(
                        velocity_event,
                        setter,
                        hid_field(HID_EVENT_TYPE_VELOCITY, 1),
                        vy,
                    );
                    objc_send_double(
                        velocity_event,
                        setter,
                        hid_field(HID_EVENT_TYPE_VELOCITY, 2),
                        0.0,
                    );
                    objc_send_append(
                        hid,
                        sel_registerName(c"appendEvent:".as_ptr()),
                        velocity_event,
                    );
                    objc_send0(velocity_event, sel_registerName(c"release".as_ptr()));
                }
            }
        }
    }

    unsafe {
        set_hid_event(event, hid as *const c_void);
        // SLEventSetIOHIDEvent retains/copies the HIDEvent. The setter's
        // ownership was round-tripped by Mac Mouse Fix on arm64 and Rosetta;
        // release our temporary Objective-C owner after attaching it.
        objc_send0(hid, sel_registerName(c"release".as_ptr()));
    }
    true
}

/// Synthesize and post the complete DockSwipe + companion Gesture pair on
/// `kCGSessionEventTap`. This drives the legacy rubber-band path for Mission
/// Control, App Exposé, Spaces, and full-screen apps.
///
/// `motion` selects horizontal (1) or vertical (2). `progress` is the
/// cumulative signed displacement since the gesture started; driving
/// Began→Changed→…→Ended walks the rubber-band.
/// `velocity` only matters at Ended/Cancelled (drives the Dock's
/// commit-vs-bounce-back decision); pass `None` mid-gesture. The tuple is
/// `(x, y)` in Dock coordinates and must use the same inversion as progress.
///
/// `source` should be a real `CGEventSource` (typically
/// `kCGEventSourceStateCombinedSessionState`). NULL-sourced events
/// get treated as untrusted and the OS rewrites some fields.
fn post_dock_swipe_pair(
    source: CGEventSourceRef,
    motion: i64,
    phase: i64,
    progress: f64,
    velocity: Option<(f64, f64)>,
    ts: Timestamp,
) -> bool {
    let Some(event) = Event::with_source(source) else {
        return false;
    };
    let modern = macos_27_or_later();

    // Both generations use the DockControl envelope type. The remaining
    // fields are generation-specific: macOS 27+ consumes the attached
    // HIDEvent and should not receive the pre-27 opaque field layout.
    // The legacy Dock reader consumes these header/axis/phase fields from
    // their floating-point slots. Only fields 135 and 136 are integer-slot
    // values; matching the public Mac Mouse Fix/dockswipe recipe here is
    // required on macOS 26 and earlier.
    event.set_legacy_dock_field(kCGSEventTypeField, kCGSEventDockControl as f64);
    if !modern {
        event.set_legacy_dock_field(kCGEventGestureHIDType, kIOHIDEventTypeDockSwipe as f64);
        event.set_legacy_dock_field(kCGEventGesturePhase, phase as f64);
        event.set_legacy_dock_field(kCGEventGesturePhaseDuplicate, phase as f64);
        event.set_legacy_dock_field(kCGEventScrollGestureFlagBits, progress);
        event.set_legacy_dock_field(kCGEventGestureSwipeMotion, motion as f64);
        event.set_legacy_dock_field(kCGEventGestureSwipeMotionDuplicate, motion as f64);
        event.set_legacy_dock_field(kCGEventGestureInvertedFromDevice, 0.0);
        let axis_marker = legacy_axis_marker(motion);
        event.set_legacy_dock_field(kCGEventGestureScrollY, axis_marker);
        event.set_legacy_dock_field(kCGEventGestureZoomDeltaX, axis_marker);
        event.set_legacy_dock_field(kCGEventGestureMarker, 33231.0);
        event.set_legacy_dock_field(kCGEventGestureSwipeProgress, progress);
        if let Some((vx, vy)) = velocity {
            event.set_legacy_dock_field(kCGEventGestureSwipeVelocityX, vx);
            event.set_legacy_dock_field(kCGEventGestureSwipeVelocityY, vy);
        }
    }
    unsafe { CGEventSetTimestamp(event.0, ts.as_nanos()) };
    let attached_hid =
        modern && attach_dock_hid_event(event.0, phase, motion, progress, velocity, ts.as_nanos());
    if modern && !attached_hid {
        log::warn!(
            "DockSwipe HID payload could not be attached on macOS 27+; suppressing incompatible legacy event"
        );
        return false;
    }
    if !modern {
        log::trace!("DockSwipe using validated pre-27 CGEvent field path");
    }
    event.post_to(kCGSessionEventTap);
    // The modern HIDEvent path carries its own event record. The legacy path
    // needs the companion NSEventTypeGesture marker used by Mac Mouse Fix.
    if !attached_hid {
        let Some(companion) = Event::with_source(source) else {
            return true;
        };
        companion.set_dbl(kCGSEventTypeField, kCGEventGesture as f64);
        companion.set_dbl(kCGEventGestureMarker, 33231.0);
        unsafe { CGEventSetTimestamp(companion.0, ts.as_nanos()) };
        companion.post_to(kCGSessionEventTap);
    }
    true
}

/// Repost the terminal legacy DockSwipe frame at the delays used by the
/// established Mac Mouse Fix implementation. Each worker retains the event
/// source until it exits; a later swipe generation invalidates stale workers
/// before they can send an Ended frame into a new gesture.
fn schedule_dock_swipe_resends(
    source: CGEventSourceRef,
    motion: i64,
    progress: f64,
    velocity: Option<(f64, f64)>,
    generation: Arc<AtomicU64>,
    expected_generation: u64,
) {
    for delay in DOCK_SWIPE_RESEND_DELAYS {
        let source_addr = if source.is_null() {
            0
        } else {
            unsafe { CFRetain(source as *const c_void) as usize }
        };
        let generation = Arc::clone(&generation);
        let spawn = std::thread::Builder::new()
            .name("dock-swipe-resend".into())
            .spawn(move || {
                std::thread::sleep(delay);
                if generation.load(Ordering::Relaxed) == expected_generation {
                    let posted = post_dock_swipe_pair(
                        source_addr as CGEventSourceRef,
                        motion,
                        kCGSGesturePhaseEnded,
                        progress,
                        velocity,
                        Timestamp::now(),
                    );
                    log::debug!(
                        "DockSwipe Ended resend after {}ms posted={posted} progress={progress:+.3}",
                        delay.as_millis()
                    );
                } else {
                    log::trace!(
                        "DockSwipe Ended resend after {}ms skipped (stale generation)",
                        delay.as_millis()
                    );
                }
                if source_addr != 0 {
                    unsafe { CFRelease(source_addr as *const c_void) };
                }
            });
        if spawn.is_err() && source_addr != 0 {
            unsafe { CFRelease(source_addr as *const c_void) };
        }
    }
}

/// Post a single phased scroll event. Exactly one of `scroll_phase` and
/// `momentum_phase` should carry the active phase; the other goes on
/// the wire as `PHASE_NONE` (encoded by passing `Phase::Cancelled` for
/// the unused field). The integer pixel values drive line-equivalent
/// and point-delta fields; the float values drive the high-precision
/// `FixedPtDelta` field, so smooth-scroll-aware apps see sub-pixel
/// motion that integer truncation would otherwise drop.
///
/// Posted to `kCGSessionEventTap` (not the HID tap) so
/// AppleMultitouchHIDService doesn't merge the event into our PTP
/// device's gesture state, and using the persistent
/// combinedSessionState `source` so apps like Chrome accept this as a
/// real-trackpad fling worthy of rubber-band bounce.
fn post_scroll_event(
    source: CGEventSourceRef,
    int_x_px: i32,
    int_y_px: i32,
    float_x_px: f64,
    float_y_px: f64,
    scroll_phase: Phase,
    momentum_phase: Phase,
    ts: Timestamp,
) {
    let Some(e) = Event::from_raw(unsafe {
        CGEventCreateScrollWheelEvent2(source, kCGScrollEventUnitPixel, 2, int_y_px, int_x_px, 0)
    }) else {
        return;
    };
    // `Phase::Cancelled` is the sentinel for "this field is unused on this
    // event"; cg_*_phase encodes that as 0 (none).
    let scroll_mask = cg_scroll_phase(scroll_phase);
    let momentum_mask = cg_momentum_phase(momentum_phase);
    e.set_int(kCGScrollWheelEventScrollPhase, scroll_mask);
    e.set_int(kCGScrollWheelEventMomentumPhase, momentum_mask);
    e.set_int(kCGScrollWheelEventIsContinuous, 1);
    // High-precision deltas. Q16.16 fixed-point (1.0 == 0x10000), capped
    // at i32 range. Apps that look at `scrollingDeltaY` (NSEvent) read
    // the FixedPt value rather than the integer, so this keeps fractional
    // pixels from disappearing on slow scrolls.
    let fp_y = (float_y_px * 65536.0).clamp(i32::MIN as f64, i32::MAX as f64) as i64;
    let fp_x = (float_x_px * 65536.0).clamp(i32::MIN as f64, i32::MAX as f64) as i64;
    e.set_int(kCGScrollWheelEventFixedPtDeltaAxis1, fp_y);
    e.set_int(kCGScrollWheelEventFixedPtDeltaAxis2, fp_x);
    e.set_int(kCGScrollWheelEventPointDeltaAxis1, int_y_px as i64);
    e.set_int(kCGScrollWheelEventPointDeltaAxis2, int_x_px as i64);
    unsafe { CGEventSetTimestamp(e.0, ts.as_nanos()) };
    log::trace!(
        "post: scroll s={:?} m={:?} px=({:+},{:+}) precise=({:+.2},{:+.2})",
        scroll_phase,
        momentum_phase,
        int_x_px,
        int_y_px,
        float_x_px,
        float_y_px,
    );
    // Scroll is a session-level event. A second HID-tap post is consumed as
    // a duplicate by some AppKit clients and may be merged with the PTP
    // stream instead of routed to the view under the cursor.
    e.post_to(kCGSessionEventTap);
}

impl Momentum {
    /// Begin coasting at the given velocity. Cancels any in-flight coast
    /// first so a quick re-flick replaces the seed cleanly.
    fn start(&self, vx_mm_per_sec: f64, vy_mm_per_sec: f64) {
        self.cancel();
        let speed = (vx_mm_per_sec * vx_mm_per_sec + vy_mm_per_sec * vy_mm_per_sec).sqrt();
        if speed < MOMENTUM_SEED_MM_PER_SEC {
            log::debug!(
                "scroll: inertia skipped (speed={:.0}mm/s below seed threshold {:.0})",
                speed,
                MOMENTUM_SEED_MM_PER_SEC,
            );
            return;
        }
        self.vel_x_mm_per_sec.set(vx_mm_per_sec);
        self.vel_y_mm_per_sec.set(vy_mm_per_sec);
        self.last_tick.set(None);
        self.carry_x_px.set(0.0);
        self.carry_y_px.set(0.0);
        self.began_posted.set(false);
        let mut ctx = CFRunLoopTimerContext {
            version: 0,
            info: self as *const Momentum as *mut c_void,
            retain: None,
            release: None,
            copyDescription: None,
        };
        let now_abs = unsafe { CFAbsoluteTimeGetCurrent() };
        let timer = unsafe {
            CFRunLoopTimerCreate(
                std::ptr::null_mut(),
                now_abs + MOMENTUM_TICK_INTERVAL,
                MOMENTUM_TICK_INTERVAL,
                0,
                0,
                momentum_tick,
                &mut ctx,
            )
        };
        if timer.is_null() {
            log::warn!("scroll: CFRunLoopTimerCreate returned NULL; inertia disabled");
            return;
        }
        unsafe {
            CFRunLoopAddTimer(
                CFRunLoop::get_current().as_concrete_TypeRef() as *mut _,
                timer,
                kCFRunLoopDefaultMode,
            );
        }
        self.timer_ref.set(timer);
        log::debug!(
            "scroll: inertia started v=({:+.0},{:+.0})mm/s",
            vx_mm_per_sec,
            vy_mm_per_sec,
        );
    }

    /// Stop coasting (if active), post a momentum-Ended bracket so apps
    /// can finalize their scroll animation, and release the timer.
    /// Returns `true` if a coast was actually active.
    fn cancel(&self) -> bool {
        let t = self.timer_ref.replace(std::ptr::null_mut());
        if t.is_null() {
            return false;
        }
        unsafe {
            CFRunLoopTimerInvalidate(t);
            CFRelease(t as *const c_void);
        }
        self.vel_x_mm_per_sec.set(0.0);
        self.vel_y_mm_per_sec.set(0.0);
        self.last_tick.set(None);
        self.carry_x_px.set(0.0);
        self.carry_y_px.set(0.0);
        if self.began_posted.replace(false) {
            post_scroll_event(
                self.event_source,
                0,
                0,
                0.0,
                0.0,
                Phase::Cancelled,
                Phase::Ended,
                Timestamp::now(),
            );
        }
        log::debug!("scroll: inertia cancelled");
        true
    }

    /// One timer tick: integrate velocity over the elapsed interval,
    /// post a momentum-phase event if the integer-pixel quantum is
    /// non-zero, decay the velocity, and stop if we're below the
    /// stop threshold.
    fn tick(&self) {
        let now = Timestamp::now();
        let dt = match self.last_tick.replace(Some(now)) {
            Some(prev) => (now - prev).as_secs_f64().clamp(0.001, 0.1),
            None => MOMENTUM_TICK_INTERVAL,
        };

        // Decay velocity exponentially toward zero. `MOMENTUM_DECAY_PER_SEC`
        // is the multiplier per second; scale to dt with `^dt`.
        let factor = MOMENTUM_DECAY_PER_SEC.powf(dt);
        let vx = self.vel_x_mm_per_sec.get() * factor;
        let vy = self.vel_y_mm_per_sec.get() * factor;
        self.vel_x_mm_per_sec.set(vx);
        self.vel_y_mm_per_sec.set(vy);

        let speed = (vx * vx + vy * vy).sqrt();
        if speed < MOMENTUM_STOP_MM_PER_SEC {
            self.cancel();
            return;
        }

        // Integrate to per-tick pixel displacement, applying the same
        // power curve as the active-scroll path so the user feels a
        // continuous deceleration from flick → coast (rather than a
        // step at lift from "amplified" to "linear").
        let dx_px = accelerate_scroll(vx, self.scroll_accel) * dt;
        let dy_px = accelerate_scroll(vy, self.scroll_accel) * dt;
        let total_x = self.carry_x_px.get() + dx_px;
        let total_y = self.carry_y_px.get() + dy_px;
        let int_x = total_x.trunc() as i32;
        let int_y = total_y.trunc() as i32;
        self.carry_x_px.set(total_x - int_x as f64);
        self.carry_y_px.set(total_y - int_y as f64);

        let phase = if self.began_posted.replace(true) {
            Phase::Changed
        } else {
            Phase::Began
        };
        post_scroll_event(
            self.event_source,
            int_x,
            int_y,
            dx_px,
            dy_px,
            Phase::Cancelled,
            phase,
            Timestamp::now(),
        );
    }
}

extern "C" fn momentum_tick(_timer: CFRunLoopTimerRef, info: *mut c_void) {
    // Safety: `info` was set to `&Momentum` in `Momentum::start`, the
    // Momentum lives in a Box owned by the Emitter, and the Emitter's
    // Drop invalidates the timer before the Box is dropped. So the
    // pointer is live for every callback.
    let m = unsafe { &*(info as *const Momentum) };
    m.tick();
}

impl Drop for Emitter {
    fn drop(&mut self) {
        // Invalidate detached Ended-resend workers before releasing the
        // emitter's event source. Their retained source is released by the
        // worker after it observes this generation change.
        self.swipe_generation.fetch_add(1, Ordering::Relaxed);
        // Invalidate the timer before the Momentum box is dropped so
        // an in-flight callback can't dereference a freed pointer.
        // `cancel` may post a final MomentumPhase::Ended via the event
        // source, so the source release has to come after.
        self.momentum.cancel();
        // Bracket any in-flight synthesized swipe with a Cancelled on
        // the same axis so the Dock doesn't leave the rubber-band
        // half-open across our shutdown. Notification-mode swipes
        // don't need this — they're fire-and-forget on lift.
        if let Some(axis) = self.swipe_axis.take()
            && !self.symbolic_swipe_active.replace(false)
        {
            let motion = match axis {
                SwipeAxis::Horizontal => SWIPE_MOTION_HORIZONTAL,
                SwipeAxis::Vertical => SWIPE_MOTION_VERTICAL,
            };
            post_dock_swipe_pair(
                self.event_source,
                motion,
                kCGSGesturePhaseCancelled,
                0.0,
                Some((0.0, 0.0)),
                Timestamp::now(),
            );
        }
        if !self.event_source.is_null() {
            unsafe { CFRelease(self.event_source as *const c_void) };
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

impl Output for Emitter {
    fn set_event_time(&self, ts: Timestamp) {
        self.event_time.set(Some(ts));
    }
    fn haptic(&self, kind: HapticKind) {
        self.perform_haptic(kind);
    }
    fn pinch_admissible_now(&self) -> bool {
        let admit = self
            .cfg
            .pinch
            .evaluate(crate::app_context::bundle_id_under_cursor);
        if !admit {
            log::debug!("admit: pinch denied by policy {:?}", self.cfg.pinch);
        }
        admit
    }
    fn rotate_admissible_now(&self) -> bool {
        let admit = self
            .cfg
            .rotate
            .evaluate(crate::app_context::bundle_id_under_cursor);
        if !admit {
            log::debug!("admit: rotate denied by policy {:?}", self.cfg.rotate);
        }
        admit
    }
    fn swipe_admissible_now(&self, axis: SwipeAxis) -> bool {
        let policy = match axis {
            SwipeAxis::Horizontal => &self.cfg.horizontal_swipe.policy,
            SwipeAxis::Vertical => &self.cfg.vertical_swipe.policy,
        };
        let admit = policy.evaluate(crate::app_context::bundle_id_under_cursor);
        if !admit {
            log::debug!("admit: swipe.{:?} denied by policy {:?}", axis, policy);
        }
        admit
    }
    fn move_cursor_by(&self, dx_px: i32, dy_px: i32) {
        Emitter::move_cursor_by(self, dx_px, dy_px);
    }
    fn click(&self, button: MouseButton) {
        Emitter::click(self, button);
    }
    fn current_modifiers(&self) -> u64 {
        unsafe { CGEventSourceFlagsState(kCGEventSourceStateCombinedSessionState) }
    }
    fn click_with_modifiers(&self, button: MouseButton, modifiers: u64) {
        Emitter::click_with_modifiers(self, button, modifiers);
    }
    fn set_left_button_held(&self, held: bool) {
        Emitter::set_left_button_held(self, held);
    }
    fn set_drag_button_held(&self, held: bool) {
        Emitter::set_drag_button_held(self, held);
    }
    fn scroll(&self, dx_mm: f64, dy_mm: f64, phase: Phase) {
        Emitter::scroll(self, dx_mm, dy_mm, phase);
    }
    fn scroll_inertia(&self, vx_mm_per_sec: f64, vy_mm_per_sec: f64) {
        Emitter::scroll_inertia(self, vx_mm_per_sec, vy_mm_per_sec);
    }
    fn cancel_inertia(&self) -> bool {
        Emitter::cancel_inertia(self)
    }
    fn pinch(&self, delta: f64, phase: Phase) {
        Emitter::pinch(self, delta, phase);
    }
    fn rotate(&self, delta_degrees: f64, phase: Phase) {
        Emitter::rotate(self, delta_degrees, phase);
    }
    fn swipe(&self, axis: SwipeAxis, signed_progress: f64, velocity_mm_per_sec: f64, phase: Phase) {
        Emitter::swipe(self, axis, signed_progress, velocity_mm_per_sec, phase);
    }
    fn look_up_dictionary(&self) {
        log::debug!("post: look up dictionary via Cmd+Ctrl+D");
        const kVK_ANSI_D: u16 = 2;
        post_cmd_ctrl_key(self.event_source, kVK_ANSI_D, self.event_timestamp());
    }
    fn smart_magnify(&self) {
        log::debug!("post: smart zoom via Mac Mouse Fix exact formula (field 55=29, field 110=22)");
        // Keep one event shape and one tap. Mac Mouse Fix's public
        // TouchSimulator reference uses a type-29 gesture with subtype 22
        // on the HID tap. Posting additional type-32/session variants can
        // make Safari/Preview interpret one double-tap more than once.
        if let Some(event) = Event::with_source(self.event_source) {
            event.set_int(55, 29); // NSEventTypeGesture
            event.set_int(110, 22); // kIOHIDEventTypeZoomToggle (22)
            event.post_to(kCGHIDEventTap);
        }
    }
    fn toggle_notification_center(&self) {
        log::debug!("post: toggle notification center via SkyLight SymbolicHotKey 163");
        let ts = self.event_timestamp();
        post_symbolic_hotkey(self.event_source, 163, ts);
    }
    fn toggle_launchpad(&self) {
        log::debug!("post: toggle launchpad via CoreDock (fallback to SkyLight 160)");
        if !send_dock_notification("com.apple.launchpad.toggle") {
            post_symbolic_hotkey(self.event_source, 160, self.event_timestamp());
        }
    }
    fn toggle_show_desktop(&self) {
        log::debug!("post: toggle show desktop via CoreDock (fallback to SkyLight 36)");
        if !send_dock_notification("com.apple.showdesktop.awake") {
            post_symbolic_hotkey(self.event_source, 36, self.event_timestamp());
        }
    }
    fn toggle_app_expose(&self) {
        log::debug!("post: toggle app expose via CoreDock (fallback to SkyLight 33)");
        if !send_dock_notification("com.apple.expose.front.awake") {
            post_symbolic_hotkey(self.event_source, 33, self.event_timestamp());
        }
    }
    fn toggle_mission_control(&self) {
        log::debug!("post: toggle mission control via CoreDock (fallback to SkyLight 32)");
        if !send_dock_notification("com.apple.expose.awake") {
            post_symbolic_hotkey(self.event_source, 32, self.event_timestamp());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_dock_swipe_uses_capture_field_storage_slots() {
        for field in [
            kCGSEventTypeField,
            kCGEventGestureHIDType,
            kCGEventGesturePhase,
            kCGEventGesturePhaseDuplicate,
            kCGEventGestureSwipeMotion,
            kCGEventGestureSwipeMotionDuplicate,
            kCGEventGestureScrollY,
            kCGEventGestureZoomDeltaX,
            kCGEventGestureMarker,
            kCGEventGestureSwipeProgress,
            kCGEventGestureSwipeVelocityX,
            kCGEventGestureSwipeVelocityY,
        ] {
            assert_eq!(
                legacy_dock_field_storage(field),
                LegacyDockFieldStorage::Double,
                "legacy DockSwipe field {field} must use its double slot"
            );
        }
        for field in [
            kCGEventScrollGestureFlagBits,
            kCGEventGestureInvertedFromDevice,
        ] {
            assert_eq!(
                legacy_dock_field_storage(field),
                LegacyDockFieldStorage::Integer,
                "legacy DockSwipe field {field} must use its integer slot"
            );
        }
    }

    #[test]
    fn legacy_progress_keeps_float_bits_in_integer_slot() {
        assert_eq!(legacy_progress_bits(0.178), (0.178_f32).to_bits() as i64);
        assert_eq!(legacy_progress_bits(-0.413), (-0.413_f32).to_bits() as i64);
    }

    #[test]
    fn policy_on_admits_without_calling_lookup() {
        let mut called = false;
        let admit = GesturePolicy::On.evaluate(|| {
            called = true;
            Some("com.apple.Safari".into())
        });
        assert!(admit);
        assert!(!called, "lookup should be skipped on the fast path");
    }

    #[test]
    fn policy_off_denies_without_calling_lookup() {
        let mut called = false;
        let admit = GesturePolicy::Off.evaluate(|| {
            called = true;
            None
        });
        assert!(!admit);
        assert!(!called);
    }

    #[test]
    fn policy_only_admits_match_denies_other() {
        let allow = vec!["com.apple.Safari".into()];
        let p = GesturePolicy::Only(allow);
        assert!(p.evaluate(|| Some("com.apple.Safari".into())));
        assert!(!p.evaluate(|| Some("com.apple.Terminal".into())));
        assert!(!p.evaluate(|| None), "no app under cursor → Only denies");
    }

    #[test]
    fn policy_except_denies_match_admits_other() {
        let deny = vec!["com.apple.Terminal".into()];
        let p = GesturePolicy::Except(deny);
        assert!(!p.evaluate(|| Some("com.apple.Terminal".into())));
        assert!(p.evaluate(|| Some("com.apple.Safari".into())));
        assert!(p.evaluate(|| None), "no app under cursor → Except admits");
    }
}
