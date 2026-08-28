//! Per-frame gesture classifier.
//!
//! Tracks contacts across frames, distinguishes 1-finger (cursor/tap),
//! 2-finger (pan/pinch/rotate, mode-locked on first significant motion),
//! 3-finger swipe, and 4-finger swipe. Pure logic — depends only on
//! [`crate::report::Frame`] and an [`Output`] sink — so the heuristics
//! can be unit-tested.

use crate::output::{HapticKind, MouseButton, Output, Phase, SwipeAxis};
use crate::report::{Contact, Frame};
use crate::time::Timestamp;
use std::collections::HashMap;
use std::time::Duration;

// ---- Tunables ----
//
// Distance thresholds are in physical millimeters. They translate
// directly across pads of any density / aspect ratio because contacts
// arrive in mm (the decoder applies per-axis chip-px → mm scaling
// using `Layout::physical_*_max_mm`). Numbers are calibrated to human
// finger ergonomics, not pad fractions.

/// Max distance a contact may drift from its landing point during a
/// short touch and still count as a tap. 1 mm covers finger roll on
/// landing without admitting deliberate cursor moves — and it has to
/// stay below the point where a 2F gesture's per-finger motion becomes
/// meaningful, because `dispatch_two` uses this same budget to decide
/// when a 2F touch stops being tap-eligible and may lock a mode.
const TAP_MAX_MOVE_MM: f64 = 1.0;
/// Max touch duration to count as a tap (240 ms).
const TAP_MAX_DURATION: Duration = Duration::from_millis(240);
/// Centroid motion below this between frames is treated as jitter.
const MOTION_DEAD_ZONE_MM: f64 = 0.04;

/// Centroid pan distance needed to lock 2F mode = pan.
const PAN_LOCK_MM: f64 = 0.4;
/// Cosine-of-angle floor for the per-finger motion-direction gate in
/// `pan_qualified`. Above this, both fingers are moving in essentially
/// the same direction (within ~14°) and we treat the gesture as pan
/// even when one finger lags the other. Picked between the anchored-
/// rotate test's worst-case alignment (0.925) and the user's slow-
/// scroll-with-lag observation (0.997). On the SoflePLUS2's small
/// trackpad fingers crowd close enough that one lagging the other is
/// the common case for a slow careful scroll.
const PAN_ALIGNMENT_COS_MIN: f64 = 0.97;
/// Distance-change ratio needed to lock 2F mode = pinch (unitless).
const PINCH_LOCK_RATIO: f64 = 0.04;
/// Angle change (radians) past which the 2F lock check considers
/// rotation present. Pinch and rotate fire as concurrent streams once
/// locked, so this no longer races against pinch — but it still
/// determines how soon the locked-2F mode fires on a primarily-rotate
/// gesture. 3.0° provides fast, natural rotation onset without false locks.
const ROTATE_LOCK_RAD: f64 = 3.0_f64 * std::f64::consts::PI / 180.0;
/// Per-finger displacement (mm) below which a contact is considered
/// essentially anchored. Sits between two real hardware observations:
/// genuine anchored-pinch traces show the "still" finger at
/// ≤ 0.27 mm of slow same-direction drift across the lock window
/// (chip noise + finger settling), while the user's misclassification
/// logs show trailing fingers at 0.47–0.67 mm of opposite-direction
/// motion. 0.3 mm separates the two without a false admit on either
/// side. Used in the pinch/rotate lock-admission gate to distinguish
/// anchored-rotate from the ambiguous "leader committed, trailer in
/// noise band" case.
const ANCHORED_FINGER_FLOOR_MM: f64 = 0.15;

/// Two contacts closer together than this cannot be two distinct
/// fingers — a capacitive panel splitting one fat contact into two
/// blobs is the only way it happens. Such a "2F" touch must never
/// resolve to a right-click; it falls back to being evaluated as the
/// single-finger tap the user actually made. Measured against the
/// inter-contact distance at the moment the second contact appeared,
/// not against motion, so a genuine pinch that ends with the fingers
/// touching is unaffected.
const FAT_FINGER_SPLIT_MM: f64 = 8.0;

/// Minimum number of two-finger frames observed before any lock
/// decision may fire. The frame a second finger lands on has one
/// contact fresh and one mid-glide, which makes the
/// common/differential decomposition meaningless — see
/// `gesture-tuning-ideas.md` idea #5. Two frames costs one chip frame
/// (~8 ms on a 125 Hz pad) of onset latency, below the perceptual
/// floor, and removes the whole class of "locked scroll on the landing
/// frame, then immediately switched to pinch" churn.
const TWO_FINGER_MIN_FRAMES: u32 = 2;

/// A tap-drag's second contact has to stay down at least this long
/// before it counts as a drag. Lifting sooner means the user was
/// double-clicking, so the second contact is dispatched as the second
/// click of a double-click instead of as a press-and-drag. Apple's
/// tap-drag has the same shape: the second tap must be *held*.
const TAP_DRAG_CONFIRM: Duration = Duration::from_millis(200);

/// Window before lift whose peak centroid speed seeds scroll inertia.
/// Fingers decelerate as they leave the surface, so the last frame's
/// instantaneous velocity systematically under-reports the throw the
/// user actually made; the peak over the tail is what they felt.
const INERTIA_PEAK_WINDOW: Duration = Duration::from_millis(50);

/// Centroid travel needed to lock the swipe axis (horizontal vs
/// vertical). Below this, the gesture is still ambiguous; we wait
/// rather than picking an axis off centroid jitter.
const SWIPE_AXIS_LOCK_MM: f64 = 3.0;
/// Physical finger travel (mm) along the locked swipe axis that
/// corresponds to ±1.0 progress in the synthesized DockSwipe event.
/// The Dock's commit threshold is around ±0.5, so a ~25 mm swipe
/// (half of the
/// reference) reliably commits — matches what feels natural on a
/// 50 mm-tall trackpad without making short swipes accidentally
/// trigger. Tunable.
const SWIPE_PROGRESS_REF_MM: f64 = 50.0;

/// Maximum interval between first tap lift and second tap landing for tap-drag.
/// Apple standard is ~200-220ms. Longer windows cause accidental text selection drags.
const TAP_DRAG_INTERVAL: Duration = Duration::from_millis(220);

/// Maximum interval between two 2F taps for double-tap smart zoom.
/// Under Apple standard (~220-250ms), a second tap inside this window resolves to smart zoom;
/// otherwise single tap resolves to secondary click (Right Click).
const TWO_FINGER_DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(220);

/// Centroid travel needed to engage a tap-drag before posting the
/// synthetic left-button hold. 0.8mm ensures natural micro-jitter doesn't latch.
const DRAG_ENGAGE_MM: f64 = 0.80;

/// Window after a 2F → 1F partial-lift transition during which a
/// subsequent 1F → 2F re-arrival is treated as a continuation of the
/// same 2F gesture (preserving lock state, tap-eligibility motion
/// budget, and `started_at`), rather than starting a fresh 2F
/// classification. The TPS65 in the SoflePLUS2 commonly reports brief
/// single-frame contact drop-outs during one physical scroll; each
/// drop-out used to reset the 2F baseline, catching one finger
/// mid-glide on the next frame and causing pinch+rotate misclassification.
/// 80 ms covers the observed drop-out span (35–55 ms typical) with
/// margin, but is far below intentional re-grip time (~150 ms+).
const PARTIAL_LIFT_REJOIN_WINDOW: Duration = Duration::from_millis(80);
/// Max distance (mm) the surviving (non-lifted) contact may have
/// travelled during the 1F gap before we refuse to treat the next 2F
/// as a continuation. Mostly a sanity guard against the chip dropping
/// the surviving ID and re-assigning the same ID to a different finger
/// (which would surface as a teleport). 10 mm is generous enough for
/// fast continuous scrolls (~150 mm/s × 80 ms = 12 mm worst case, but
/// the user has lifted their other finger so they're typically slower).
const PARTIAL_LIFT_REJOIN_DRIFT_MM: f64 = 10.0;

/// EMA weight on the freshest velocity sample during 2F pan, in [0, 1].
/// 0.4 ≈ 2.5-frame averaging window on a ~125 Hz pad — fast enough to
/// catch a flick, slow enough that one noisy chip frame doesn't dominate
/// the inertia seed. Mirrors rmk's `VEL_EMA_NUM/VEL_EMA_DEN = 96/256`.
const SCROLL_VELOCITY_ALPHA: f64 = 0.4;

/// Fallback frame `dt` for the very first sampled frame of a gesture,
/// before we have a previous frame to subtract from. ~8 ms matches a
/// 125 Hz PTP pad, which both supported keyboards run at.
const DEFAULT_FRAME_DT: Duration = Duration::from_micros(8000);

/// Upper bound on plausible fingertip speed across a trackpad. Sprinters
/// of the trackpad world manage a few hundred mm/s; 1200 leaves generous
/// headroom while still catching the data faults that would otherwise
/// teleport the pointer.
const MAX_FINGER_SPEED_MM_S: f64 = 1200.0;

/// Power-curve cursor acceleration parameters. The curve is
/// `pixels_per_sec = c · |v|^E` (in finger mm/s → screen px/s), with
/// `c` chosen so that at `v == ref_mm_per_sec` the result equals the
/// linear `px_per_mm_at_ref · v`. Below `ref` → sub-linear gain (more
/// precision for slow movements); above `ref` → super-linear gain
/// (faster cross-screen movement). `exponent == 1.0` reduces to plain
/// linear (`px_per_mm_at_ref` regardless of speed) so the curve is
/// off-by-default.
#[derive(Clone, Copy, Debug)]
pub struct CursorAccel {
    pub px_per_mm_at_ref: f64,
    pub exponent: f64,
    pub ref_mm_per_sec: f64,
}

impl Default for CursorAccel {
    fn default() -> Self {
        Self {
            px_per_mm_at_ref: 28.0,
            exponent: 1.35,
            ref_mm_per_sec: 70.0,
        }
    }
}

/// Apply the cursor-acceleration curve to a scalar velocity, returning
/// pixels per second. Kept as a reference helper for calibration tests.
#[allow(dead_code)]
fn accelerate_cursor(v_mm_per_sec: f64, cfg: CursorAccel) -> f64 {
    let mag = v_mm_per_sec.abs();
    if mag == 0.0 {
        return 0.0;
    }
    let linear = cfg.px_per_mm_at_ref * cfg.ref_mm_per_sec.powf(1.0 - cfg.exponent);
    v_mm_per_sec.signum() * linear * mag.powf(cfg.exponent)
}

/// Apply one acceleration gain derived from the velocity magnitude to both
/// axes. Native pointer acceleration is direction-preserving: diagonal
/// motion must not receive a different gain merely because its energy is
/// split between X and Y. Live cursor motion uses this vector form.
fn accelerate_cursor_vector(
    vx_mm_per_sec: f64,
    vy_mm_per_sec: f64,
    cfg: CursorAccel,
) -> (f64, f64) {
    let speed = vx_mm_per_sec.hypot(vy_mm_per_sec);
    if speed == 0.0 {
        return (0.0, 0.0);
    }
    let linear = cfg.px_per_mm_at_ref * cfg.ref_mm_per_sec.powf(1.0 - cfg.exponent);
    let gain = linear * speed.powf(cfg.exponent - 1.0);
    (vx_mm_per_sec * gain, vy_mm_per_sec * gain)
}

#[derive(Clone, Copy, Debug)]
struct Tracked {
    x: f64,
    y: f64,
    prev_x: f64,
    prev_y: f64,
    down_x: f64,
    down_y: f64,
    down_at: Timestamp,
    max_move_sq: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GestureKind {
    Idle,
    OneFinger,
    TwoFingerUnclassified,
    TwoFingerPan,
    /// Locked 2F gesture family. Pan/scroll is mutually exclusive with
    /// this family; each admitted transform stream receives a complete
    /// Began/Changed/Ended sequence.
    TwoFingerPinchAndRotate,
    ThreeFingerLive,
    /// 拖移样式 = 三指拖移 mode. While active, three-finger motion drags
    /// (left-button held + accelerated cursor) instead of firing Dock
    /// swipes; four fingers retain the full swipe surface. Survives
    /// asynchronous lifts down to a single finger — native behavior —
    /// and releases the button when the pad empties.
    ThreeFingerDrag,
    FourFingerLive,
    /// Latched after a swipe fires, until all fingers lift.
    SwipeLatched,
}

/// Transform deltas are relative AppKit values. These limits protect the
/// event stream from a dropped/reordered frame becoming a huge jump while
/// retaining the sign and allowing normal motion at common scan rates.
const PINCH_MAX_RATE: f64 = 3.0;
const PINCH_MAX_FRAME_DELTA: f64 = 0.08;
const ROTATE_MAX_RATE_RAD: f64 = 12.0;
const ROTATE_MAX_FRAME_RAD: f64 = 15.0_f64.to_radians();
const TRANSFORM_MIN_DELTA: f64 = 1e-3;
const ROTATE_EMIT_DEADZONE_RAD: f64 = 0.1_f64.to_radians();

#[derive(Clone, Copy, Debug)]
struct TwoFingerBaseline {
    initial_distance: f64,
    initial_angle: f64,
    /// Per-finger initial positions, keyed by contact ID so the lock
    /// check can compute per-finger displacement (and the
    /// common/differential motion decomposition) even if `active[0]`
    /// and `active[1]` swap order between frames. The classifier needs
    /// these to disqualify pan when one finger contributes most of the
    /// motion: asymmetric pinch/rotate around a near-anchored finger
    /// drifts the centroid as a *side effect* rather than as a real
    /// pan signal.
    initial_a: (u8, (f64, f64)),
    initial_b: (u8, (f64, f64)),
    last_centroid: (f64, f64),
    /// Number of 2F frames dispatched against this baseline. No lock may
    /// fire until at least [`TWO_FINGER_MIN_FRAMES`] have been seen, so
    /// the frame a second finger lands on can never decide the mode.
    frames_observed: u32,
    /// Highest instantaneous centroid velocity sampled during 2F pan and
    /// when it was taken. Fingers decelerate as they leave the surface,
    /// so the final frame's velocity under-reports the throw; the peak
    /// inside [`INERTIA_PEAK_WINDOW`] before lift is what seeds inertia.
    peak_velocity: (f64, f64),
    peak_velocity_at: Option<Timestamp>,
    /// Previous-frame scale and angle, refreshed every frame so per-frame
    /// pinch and rotate deltas are always one-frame. Each admitted stream
    /// is emitted independently; a stream's deadzone only suppresses its
    /// own noise without affecting the other stream.
    prev_scale: f64,
    prev_angle: f64,
    /// EMA-smoothed centroid velocity in mm/sec, sampled while in
    /// `TwoFingerPan`. Seeds inertia at lift via `Output::scroll_inertia`
    /// — modeled on rmk's `TrackpadProcessor` velocity track.
    scroll_velocity: (f64, f64),
    /// Time of the most recent scroll-event emission. Combined with the
    /// new event's timestamp to compute the per-frame dt that turns a
    /// per-frame mm delta into a mm/sec velocity sample.
    last_scroll_time: Option<Timestamp>,
    /// Timestamp used to rate-limit relative pinch/rotate deltas. Kept
    /// separate from scroll timing so a Pan -> transform transition cannot
    /// inherit an old scroll interval.
    prev_transform_at: Option<Timestamp>,
    /// Set when a frame's pinch or rotate score crossed lock threshold
    /// but a lenient pan signal (basic `common > differential * 1.2`,
    /// ignoring the per-finger gate) was stronger — likely a slow scroll
    /// where one finger trails the other and the strict balance gate
    /// hasn't passed yet. We defer the pinch+rot lock by one frame to
    /// give the slower finger a chance to catch up; the next pinch/rot
    /// crossing commits regardless of pan's status. Bounded to one frame
    /// so a real pinch only feels ~one chip-frame slower to onset.
    pinch_rot_lock_pending: bool,
    /// Whether pinch / rotate are admissible for the app under the
    /// cursor at the moment this 2F gesture started. Sampled once via
    /// [`Output::pinch_admissible_now`] / [`Output::rotate_admissible_now`]
    /// and held for the duration of the touch. When `false`, the
    /// corresponding score is forced to zero in the lock decision so a
    /// 2F gesture in an app that doesn't support pinch (e.g. iTerm2)
    /// can't lock into TwoFingerPinchAndRotate when the user meant
    /// scroll. With both `false`, only TwoFingerPan is reachable.
    pinch_admitted: bool,
    rotate_admitted: bool,
    cumulative_dx: f64,
    right_edge_candidate: bool,
    right_edge_latched: bool,
}

#[derive(Clone, Copy, Debug)]
struct MultiBaseline {
    _initial_centroid: (f64, f64),
    /// Number of fingers tracked in the current cluster. When this
    /// changes (e.g. 4 -> 3 or 3 -> 4 on a capacitive screen), the raw
    /// centroid jumps because the geometry changed, not because the hand
    /// moved. We re-anchor last_centroid without adding the jump to
    /// cumulative motion.
    finger_count: usize,
    /// Cumulative centroid motion (mm) along X and Y since gesture start,
    /// immune to finger-count drop/rejoin jumps.
    cumulative_dx: f64,
    cumulative_dy: f64,
    /// Locked swipe axis. None until cumulative centroid motion
    /// crosses [`SWIPE_AXIS_LOCK_MM`]; after that, the dominant
    /// component (whichever of horizontal/vertical is larger at the
    /// moment of lock) is held for the rest of the gesture so a
    /// wandering centroid near the diagonal doesn't flip the swipe
    /// sideways mid-flight.
    axis: Option<SwipeAxis>,
    /// True after `Output::swipe(.., Phase::Began)` has been posted
    /// for the current stream. Gates the corresponding Ended on lift /
    /// finger-count drop so we don't emit an orphaned Ended on a
    /// gesture that never crossed the axis-lock threshold.
    began_posted: bool,
    /// Per-axis admission for swipes, sampled at the start of the 3F/4F
    /// gesture. Used in axis-lock to refuse the dominant axis if it
    /// isn't admitted under the cursor — the gesture stays unlocked
    /// rather than firing a swipe the policy would have suppressed.
    swipe_horizontal_admitted: bool,
    swipe_vertical_admitted: bool,
    /// Most recent centroid sample. Used to derive the per-frame
    /// motion delta for velocity tracking; avoids re-deriving from
    /// each contact's previous-frame state.
    last_centroid: (f64, f64),
    /// Wall-clock time of `last_centroid`. None on the first frame
    /// (no meaningful dt yet).
    last_centroid_time: Option<Timestamp>,
    /// EMA-smoothed centroid velocity in mm/s along (X, Y). Carried
    /// to the Ended event as the lift-velocity signal that the Dock
    /// uses to decide commit-vs-rubber-band.
    velocity: (f64, f64),
    /// Mean distance of all contacts from centroid at touchdown.
    initial_radial_spread: f64,
    /// Latch for 4-finger radial gestures (Launchpad/Show Desktop) so they fire once per touch session.
    radial_action_latched: bool,
}

fn compute_radial_spread(active: &[Contact], centroid: (f64, f64)) -> f64 {
    if active.is_empty() {
        return 0.0;
    }
    let sum: f64 = active
        .iter()
        .map(|c| ((c.x - centroid.0).powi(2) + (c.y - centroid.1).powi(2)).sqrt())
        .sum();
    sum / active.len() as f64
}

/// Runtime-tunable behavior switches. Passed via [`State::with_options`];
/// `State::new` uses the defaults (all features off — upstream HID
/// behavior untouched).
#[derive(Clone, Copy, Debug)]
pub struct GestureOptions {
    /// Whether a short one-finger touch produces a left click.
    pub tap_to_click: bool,
    /// Whether a short two-finger touch produces a secondary click.
    pub secondary_click: bool,
    /// Whether a two-finger double tap invokes Smart Zoom.
    pub smart_zoom: bool,
    /// Whether a three-finger tap invokes Dictionary Lookup.
    pub dictionary_lookup: bool,
    /// Whether two-finger translation emits scroll events.
    pub scroll_enabled: bool,
    /// Whether a right-edge two-finger swipe toggles Notification Center.
    pub right_edge_swipe: bool,
    /// 拖移样式 = 三指拖移: three-finger motion drags instead of firing
    /// Dock swipes. Four fingers keep the full swipe surface.
    pub three_finger_drag: bool,
    /// Drag-lock style delayed release: how long after the last finger
    /// leaves before the left button is actually released. Landing a
    /// finger inside the window cancels the release and continues the
    /// same drag. Keep this at zero for Apple's Three-Finger Drag mode,
    /// which stops as soon as the fingers lift; positive values model the
    /// separate accessibility "With Drag Lock" behavior.
    pub release_delay_ms: u64,
    /// Whether 1-finger double-tap-to-drag is enabled. When disabled
    /// (default, matching macOS when Three-Finger Drag is active),
    /// a quick tap followed by cursor movement moves the cursor normally
    /// instead of locking the left mouse button into a drag.
    pub one_finger_tap_drag: bool,
    /// Whether single-finger press-and-hold (stationary hold >= 450ms) latches the left mouse button.
    pub press_and_hold_drag: bool,
}

impl Default for GestureOptions {
    fn default() -> Self {
        Self {
            tap_to_click: true,
            secondary_click: true,
            smart_zoom: true,
            dictionary_lookup: true,
            scroll_enabled: true,
            right_edge_swipe: true,
            three_finger_drag: true,
            release_delay_ms: 500,
            one_finger_tap_drag: true,
            press_and_hold_drag: false,
        }
    }
}

/// Baseline for the live three-finger drag. Tracks the centroid the
/// same way [`MultiBaseline`] does for swipes, but its output is cursor
/// motion while the left button is held (synthesized press), so it
/// carries an engage gate instead of a swipe-axis lock.
#[derive(Clone, Copy, Debug)]
struct DragBaseline {
    initial_centroid: (f64, f64),
    last_centroid: (f64, f64),
    /// Finger count observed on the previous frame. When it changes
    /// (async lift/addition mid-drag), the centroid jumps because the
    /// mean is taken over a different cluster — that transitional
    /// frame drops its delta instead of teleporting the cursor.
    finger_count: usize,
    /// True once cumulative centroid motion crossed
    /// [`DRAG_ENGAGE_MM`] and `set_drag_button_held(true)` was posted.
    engaged: bool,
    /// Host timestamp when a re-grip touchdown frame was observed.
    regrip_started_at: Option<Timestamp>,
}

/// Carry-over state from a 2F touch that briefly dropped to 1F (a
/// "partial lift" — one contact disappeared for a chip frame or two,
/// most commonly because the trackpad chip momentarily lost it, not
/// because the user lifted intentionally). Captured at the
/// `TwoFinger* → OneFinger` transition; consumed at the next
/// `OneFinger → TwoFingerUnclassified` transition if it happens
/// within [`PARTIAL_LIFT_REJOIN_WINDOW`] AND the surviving contact's
/// ID still matches AND its position hasn't drifted past
/// [`PARTIAL_LIFT_REJOIN_DRIFT_MM`]. While set, `dispatch_one` also
/// holds back cursor motion — the 1F gap is the tail of the 2F
/// gesture, not a brief cursor intent.
#[derive(Clone, Copy, Debug)]
struct TwoFingerRecent {
    baseline: TwoFingerBaseline,
    /// The locked 2F kind at lift time. On rejoin within the window
    /// the engine resumes this kind directly (Pan or PinchAndRotate)
    /// instead of re-classifying; for Unclassified we restore the
    /// budget but seed per-finger initial positions to the rejoin
    /// frame so the next lock decision sees motion from a clean
    /// baseline (the lifted-and-returned finger has no accumulated
    /// motion to compare against the surviving finger's).
    kind: GestureKind,
    /// `State::started_at` from the pre-lift gesture. Restored so the
    /// tap-eligibility duration math still measures from the user's
    /// first contact, not from the rejoin.
    started_at: Timestamp,
    /// `State::max_move_sq` from the pre-lift gesture. Restored so
    /// tap-eligibility motion math doesn't lose what the pre-lift 2F
    /// already accumulated. The surviving contact's per-contact
    /// max_move keeps accumulating across the gap (tracking logic in
    /// `on_frame_at` preserves its `Tracked` entry), so this is mainly
    /// load-bearing when the *re-arriving* finger had the largest
    /// pre-lift motion.
    max_move_sq: f64,
    /// Contact ID that remained on-pad through the gap.
    surviving_id: u8,
    /// Surviving contact's position at the moment of the lift
    /// transition. Used together with [`PARTIAL_LIFT_REJOIN_DRIFT_MM`]
    /// to reject rejoins where the surviving contact teleported
    /// (likely an ID-collision after a double drop-out, not a real
    /// continuation).
    surviving_pos: (f64, f64),
    /// Timestamp of the 2F → 1F transition. Used to enforce
    /// [`PARTIAL_LIFT_REJOIN_WINDOW`].
    lift_time: Timestamp,
}

/// Carry-over state from a 2F touch whose fingers lifted asynchronously
/// (one before the other). Captured at the
/// TwoFingerUnclassified → OneFinger transition; consumed at the
/// subsequent OneFinger → Idle transition. While set, the residual 1F
/// touch is not eligible to fire its own left-click — it's the tail of
/// the 2F gesture, not a fresh 1F tap.
#[derive(Clone, Copy, Debug)]
struct PendingTwoFingerTap {
    started_at: Timestamp,
    max_move_sq: f64,
    /// Inter-contact distance when the 2F touch started. Below
    /// [`FAT_FINGER_SPLIT_MM`] the "two fingers" were one fat contact
    /// the panel split, and the pending right-click is bogus.
    initial_distance: f64,
}

#[derive(Clone, Copy, Debug)]
struct PendingThreeFingerTap {
    started_at: Timestamp,
    max_move_sq: f64,
}

pub struct State<O: Output> {
    out: O,
    contacts: HashMap<u8, Tracked>,
    kind: GestureKind,
    started_at: Timestamp,
    /// Worst-case movement of any contact since the gesture started.
    max_move_sq: f64,
    two_baseline: Option<TwoFingerBaseline>,
    multi_baseline: Option<MultiBaseline>,
    drag_baseline: Option<DragBaseline>,
    /// True between posting `set_drag_button_held(true)` and its
    /// matching release. Kept on the State (not inside DragBaseline) so
    /// a close-out can never lose track of an open press even if the
    /// baseline is cleared.
    drag_button_held: bool,
    options: GestureOptions,
    /// Drag-lock pending release. Set when the pad empties during an
    /// engaged drag; the actual button-up fires via [`State::tick`]
    /// once `release_delay_ms` elapses. Any finger landing inside the
    /// window cancels it (grip adjustment continues the same drag).
    pending_drag_release: Option<Timestamp>,
    /// One-frame deferred cursor motion. `dispatch_one` emits the
    /// previous frame's value and stages the current frame's; on
    /// transition out of `OneFinger` (most importantly to `Idle` on
    /// lift) the buffered value is discarded. Mirrors rmk's
    /// `TrackpadProcessor::pending_motion` — the chip's last
    /// with-finger frame commonly carries a centroid-shift artifact
    /// (the contact patch shrinks asymmetrically as the finger rolls
    /// off) that, if emitted, teleports the cursor on release. Costs
    /// ~one chip cycle of cursor latency for not getting that jump.
    /// Stored with the frame `dt` at sample time so the acceleration
    /// curve runs on the right velocity when the value is finally
    /// emitted on the *next* frame.
    pending_motion: Option<(f64, f64, Duration)>,
    /// Cursor-acceleration curve parameters. Held by the gesture
    /// engine (not the platform output) so `Output::move_cursor_by`
    /// only sees integer pixel deltas — keeps the curve testable
    /// without faking out CGEvent.
    cursor_accel: CursorAccel,
    /// Sub-pixel residuals carried across frames for cursor motion.
    /// Without these, integer truncation of `pixels_per_sec · dt`
    /// drops up to one pixel per frame, which on slow precise
    /// movement is the difference between motion and a stuck cursor.
    /// Reset on transitions out of `OneFinger`.
    cursor_carry_x_px: f64,
    cursor_carry_y_px: f64,
    /// Timestamp of the previous `on_frame_at` call. Used to derive
    /// per-frame `dt`, which is stamped onto `pending_motion` so the
    /// curve sees the actual sample interval (matters when chip
    /// frames stretch — e.g. coalesced reports under load).
    prev_frame_at: Option<Timestamp>,
    pending_two_finger_tap: Option<PendingTwoFingerTap>,
    pending_three_finger_tap: Option<PendingThreeFingerTap>,
    /// See [`TwoFingerRecent`]. Set on a 2F → 1F partial lift, taken
    /// at the top of the next `transition` call to decide whether to
    /// restore. Auto-cleared if it goes stale (window expired) while
    /// still in OneFinger via the dispatch_one cursor-suppression
    /// path. Always None outside the partial-lift gap.
    two_finger_recent: Option<TwoFingerRecent>,
    /// Set on `Idle → non-Idle` transitions when `Output::cancel_inertia`
    /// reports a coast was actually live. Persists for the duration of
    /// the new gesture session and suppresses any tap derived from it
    /// (1F left, 2F right, deferred right via `pending_two_finger_tap`).
    /// Mirrors rmk's `TouchSession::born_during_coast`: the touch's
    /// purpose was to stop the fling, not to click. Cleared on the
    /// next `… → Idle` transition.
    born_during_coast: bool,
    /// Set on any 2F-locked → OneFinger transition (pan, pinch, rotate,
    /// or unclassified-but-not-tap-eligible). The residual 1F is the
    /// tail of an asynchronous lift, not a fresh single-finger tap; the
    /// next `OneFinger → Idle` must NOT fire a Left click. Cleared by
    /// the consuming OneFinger close-out. Distinct from
    /// `pending_two_finger_tap`, which carries a *deferred* right-click
    /// from a tap-eligible 2F session — the suppress flag has no such
    /// payload, it just blocks the residual's own click path.
    suppress_one_finger_click: bool,
    /// Last seen value of `Frame::button`. The PTP integrated button bit
    /// originates upstream (firmware mirrors keymap-driven `MouseBtn1`),
    /// so all this layer does is detect edges and forward them via
    /// `Output::set_left_button_held` — which the emitter then turns
    /// into LeftMouseDown/Up CGEvents and uses to switch cursor moves
    /// over to LeftMouseDragged. Treated independently of finger
    /// gestures (taps/scroll still classify normally while held).
    prev_button: bool,
    last_1f_tap: Option<Timestamp>,
    pending_right_click: Option<Timestamp>,
    tap_drag_candidate: bool,
    tap_drag_active: bool,
    /// Set when a second tap lands inside the tap-drag window, and
    /// cleared once that contact commits one way or the other. While it
    /// is set the gesture is genuinely ambiguous — the user is either
    /// starting a drag or completing a double-click — so the button is
    /// not pressed and the pointer is pinned. Pressing on the landing
    /// frame (which is what the engine did before) makes every
    /// double-click arrive as a click plus an unrelated press/release
    /// pair, which is why double-clicks stopped registering.
    tap_drag_pending_since: Option<Timestamp>,
    hold_latched: bool,
}

impl<O: Output> State<O> {
    #[allow(dead_code)]
    pub fn new(out: O, cursor_accel: CursorAccel) -> Self {
        Self::with_options(out, cursor_accel, GestureOptions::default())
    }

    pub fn with_options(out: O, cursor_accel: CursorAccel, options: GestureOptions) -> Self {
        let now = Timestamp::now();
        Self {
            out,
            contacts: HashMap::new(),
            kind: GestureKind::Idle,
            started_at: now,
            max_move_sq: 0.0,
            two_baseline: None,
            multi_baseline: None,
            drag_baseline: None,
            drag_button_held: false,
            options,
            pending_drag_release: None,
            pending_motion: None,
            cursor_accel,
            cursor_carry_x_px: 0.0,
            cursor_carry_y_px: 0.0,
            prev_frame_at: None,
            pending_two_finger_tap: None,
            pending_three_finger_tap: None,
            two_finger_recent: None,
            born_during_coast: false,
            suppress_one_finger_click: false,
            prev_button: false,
            last_1f_tap: None,
            pending_right_click: None,
            tap_drag_candidate: false,
            tap_drag_active: false,
            tap_drag_pending_since: None,
            hold_latched: false,
        }
    }

    fn emit_tap_click(&self, button: MouseButton) {
        let allowed = match button {
            MouseButton::Left => self.options.tap_to_click,
            MouseButton::Right => self.options.secondary_click,
        };
        if allowed {
            self.out.haptic(HapticKind::Click);
            self.out.click(button);
        } else {
            log::debug!("tap click suppressed by macOS/config policy: {button:?}");
        }
    }

    fn emit_smart_zoom(&self) {
        if self.options.smart_zoom {
            self.out.haptic(HapticKind::GestureCommitted);
            self.out.smart_magnify();
        } else {
            log::debug!("smart zoom suppressed by macOS/config policy");
        }
    }

    fn emit_dictionary_lookup(&self) {
        if self.options.dictionary_lookup {
            self.out.haptic(HapticKind::GestureCommitted);
            self.out.look_up_dictionary();
        } else {
            log::debug!("dictionary lookup suppressed by macOS/config policy");
        }
    }

    fn emit_scroll(&self, dx_mm: f64, dy_mm: f64, phase: Phase) {
        if self.options.scroll_enabled {
            self.out.scroll(dx_mm, dy_mm, phase);
        }
    }

    fn emit_scroll_inertia(&self, vx_mm_per_sec: f64, vy_mm_per_sec: f64) {
        if self.options.scroll_enabled {
            self.out.scroll_inertia(vx_mm_per_sec, vy_mm_per_sec);
        }
    }

    /// Convenience wrapper that stamps the frame with the current
    /// host time. Used only by tests where timing isn't load-bearing
    /// (e.g. integrated-button edge cases). Production goes through
    /// [`Self::on_frame_at`] directly with a scan-time-derived
    /// timestamp from [`crate::scan_clock::ScanTimeClock`].
    #[cfg(test)]
    pub fn on_frame(&mut self, frame: Frame) {
        self.on_frame_at(frame, Timestamp::now());
    }

    /// Process one decoded touchpad frame at host time `now`. The HID
    /// layer (`hid::on_input_report`) computes `now` via
    /// [`crate::scan_clock::ScanTimeClock`], which converts the chip's
    /// `scan_time_100us` into a host-aligned `Timestamp` whose per-frame
    /// deltas reflect the chip's scan cadence rather than report
    /// delivery cadence. Tests inject their own `now` directly.
    pub fn on_frame_at(&mut self, frame: Frame, now: Timestamp) {
        // A transport normally drives delayed drag release from its idle
        // heartbeat. Check here too so a frame that arrives just after the
        // deadline cannot resurrect an already-expired drag lock.
        self.tick(now);

        // Frame interval used by the cursor-acceleration curve. `now`
        // comes from `ScanTimeClock`, so the dt reflects the chip's
        // actual scan cadence rather than report-delivery jitter.
        // Clamp to a sane range so a long stall (or wall-clock jump)
        // can't produce a one-frame velocity blowup or a near-zero
        // dt that explodes the velocity. First-frame fallback matches
        // a 125 Hz pad — the velocity that frame is dominated by the
        // could-still-tap gate anyway, so the exact value barely
        // matters.
        let frame_dt = match self.prev_frame_at.replace(now) {
            Some(prev) => now
                .saturating_duration_since(prev)
                .clamp(Duration::from_millis(1), Duration::from_millis(100)),
            None => DEFAULT_FRAME_DT,
        };

        // Hand the frame timestamp to the output so per-frame CGEvents
        // (cursor moves, button down/up, scrolls, etc.) get stamped
        // with the host-aligned scan time rather than wall-clock now,
        // matching the time base the gesture engine itself runs on.
        self.out.set_event_time(now);

        // Forward integrated-button edges before the contact-driven
        // gesture pipeline runs, so a press that arrives in the same
        // frame as a finger movement turns into a real drag (the
        // emitter promotes the subsequent move to `LeftMouseDragged`).
        if frame.button != self.prev_button {
            self.out.set_left_button_held(frame.button);
            self.prev_button = frame.button;
        }

        let active: Vec<Contact> = frame.contacts.iter().copied().filter(|c| c.tip).collect();

        // Refresh tracked-contact state (prev → current).
        let mut next: HashMap<u8, Tracked> = HashMap::with_capacity(active.len());
        for c in &active {
            let prev = self.contacts.get(&c.id).copied();
            let (prev_x, prev_y, down_x, down_y, down_at, prior_max) = match prev {
                Some(p) => (p.x, p.y, p.down_x, p.down_y, p.down_at, p.max_move_sq),
                None => (c.x, c.y, c.x, c.y, now, 0.0),
            };
            let dx = c.x - down_x;
            let dy = c.y - down_y;
            let m = (dx * dx + dy * dy).max(prior_max);
            next.insert(
                c.id,
                Tracked {
                    x: c.x,
                    y: c.y,
                    prev_x,
                    prev_y,
                    down_x,
                    down_y,
                    down_at,
                    max_move_sq: m,
                },
            );
            if m > self.max_move_sq {
                self.max_move_sq = m;
            }
        }
        self.contacts = next;

        let new_kind = self.classify(active.len());
        if new_kind != self.kind {
            self.transition(new_kind, &active, now);
        }
        if !active.is_empty() {
            self.dispatch(&active, now, frame_dt);
        }
    }

    /// End the current touch because the link went silent, not because
    /// the user lifted. A synthesized lift carries no evidence about
    /// intent: the contacts may still be on the pad with their frames
    /// lost or throttled. Treating it as a normal lift is what turned
    /// link stalls into phantom clicks — a real capture shows a
    /// `dur=0ms` "tap" landing 648 ms after a real one, which macOS then
    /// coalesced into a double-click the user never made.
    ///
    /// Everything that depends on a deliberate lift (taps, tap-drag
    /// resolution, smart-magnify pairing) is suppressed; everything that
    /// must not be left latched (held buttons, phased event streams) is
    /// closed out exactly as a real lift would.
    #[allow(dead_code)]
    pub fn cancel_touch(&mut self, now: Timestamp) {
        if matches!(self.kind, GestureKind::Idle) {
            if self.drag_button_held {
                self.out.set_event_time(now);
                self.out.set_drag_button_held(false);
                self.drag_button_held = false;
            }
            if self.prev_button {
                self.out.set_event_time(now);
                self.out.set_left_button_held(false);
                self.prev_button = false;
            }
            return;
        }
        log::info!(
            "touch canceled by link timeout while in {:?} — closing out without tap evaluation",
            self.kind,
        );
        // Suppress every tap path the Idle close-out could take.
        self.born_during_coast = true;
        self.pending_two_finger_tap = None;
        self.pending_three_finger_tap = None;
        self.tap_drag_pending_since = None;
        self.tap_drag_candidate = false;
        self.last_1f_tap = None;
        self.pending_right_click = None;
        self.out.set_event_time(now);
        // A link timeout is not evidence of a deliberate lift. Close
        // every live stream with Cancelled so scroll consumers do not
        // start momentum and the Dock does not commit a partial swipe.
        match self.kind {
            GestureKind::TwoFingerPan => {
                self.emit_scroll(0.0, 0.0, Phase::Cancelled);
                let _ = self.out.cancel_inertia();
            }
            GestureKind::TwoFingerPinchAndRotate => {
                let (pinch, rotate) = self
                    .two_baseline
                    .map(|b| (b.pinch_admitted, b.rotate_admitted))
                    .unwrap_or((true, true));
                if pinch {
                    self.out.pinch(0.0, Phase::Cancelled);
                }
                if rotate {
                    self.out.rotate(0.0, Phase::Cancelled);
                }
            }
            GestureKind::ThreeFingerLive | GestureKind::FourFingerLive => {
                if let Some(base) = self.multi_baseline
                    && base.began_posted
                    && let Some(axis) = base.axis
                {
                    self.out.swipe(axis, 0.0, 0.0, Phase::Cancelled);
                }
            }
            GestureKind::ThreeFingerDrag => {}
            _ => {}
        }
        // A drag button may be carried into FourFingerLive so a window can
        // travel across Spaces. Link loss must release it regardless of the
        // current gesture kind; otherwise the next physical click inherits a
        // stuck synthetic left-button hold.
        if self.drag_button_held {
            self.out.set_drag_button_held(false);
            self.drag_button_held = false;
        }
        if self.prev_button || self.hold_latched {
            self.out.set_left_button_held(false);
            self.prev_button = false;
            self.hold_latched = false;
        }
        // A canceled touch must not leave a drag-lock timer armed: the
        // fingers whose return would cancel it are not coming back.
        self.pending_drag_release = None;
        self.contacts.clear();
        self.pending_motion = None;
        self.two_finger_recent = None;
        self.kind = GestureKind::Idle;
        self.started_at = now;
        self.max_move_sq = 0.0;
        self.two_baseline = None;
        self.multi_baseline = None;
        self.drag_baseline = None;
        self.cursor_carry_x_px = 0.0;
        self.cursor_carry_y_px = 0.0;
        self.prev_frame_at = Some(now);
        self.born_during_coast = false;
    }

    /// Advance time-based gesture state. Called from the transport
    /// loops' idle heartbeat (they poll on a ~50 ms cadence even with
    /// zero traffic), which drives 2F tap right-click confirmation and
    /// three-finger-drag lock release.
    #[allow(dead_code)]
    pub fn tick(&mut self, now: Timestamp) {
        if let Some(tap_time) = self.pending_right_click {
            if now.saturating_duration_since(tap_time) >= TWO_FINGER_DOUBLE_TAP_WINDOW {
                self.pending_right_click = None;
                log::debug!(
                    "2f tap: confirmed after double-tap window ({}ms) -> click Right",
                    TWO_FINGER_DOUBLE_TAP_WINDOW.as_millis()
                );
                self.emit_tap_click(MouseButton::Right);
            }
        }
        let Some(expires_at) = self.pending_drag_release else {
            return;
        };
        if now < expires_at {
            return;
        }
        self.pending_drag_release = None;
        if !self.drag_button_held {
            return; // shouldn't happen: release implies hold, but never strand a press either way
        }
        log::debug!("3f drag: drag-lock expired — releasing left button");
        self.out.set_event_time(now);
        self.out.set_drag_button_held(false);
        self.drag_button_held = false;
        // Manual full close-out to Idle (mirrors transition()'s reset
        // block; bypassing transition() here avoids re-entering the
        // ThreeFingerDrag arm that just ran).
        self.kind = GestureKind::Idle;
        self.started_at = now;
        self.max_move_sq = 0.0;
        self.two_baseline = None;
        self.multi_baseline = None;
        self.drag_baseline = None;
        self.pending_motion = None;
        self.cursor_carry_x_px = 0.0;
        self.cursor_carry_y_px = 0.0;
        self.born_during_coast = false;
    }

    fn flush_pending_right_click(&mut self) {
        if let Some(_) = self.pending_right_click.take() {
            log::debug!("2f tap: flushing pending right-click -> click Right");
            self.emit_tap_click(MouseButton::Right);
        }
    }

    fn on_two_finger_tap_lift(&mut self, now: Timestamp) {
        self.last_1f_tap = None;
        self.tap_drag_candidate = false;
        if let Some(prev) = self.pending_right_click.take() {
            if now.saturating_duration_since(prev) <= TWO_FINGER_DOUBLE_TAP_WINDOW {
                log::debug!(
                    "2f double tap: smart zoom / smart magnify (interval={}ms)",
                    now.saturating_duration_since(prev).as_millis()
                );
                self.emit_smart_zoom();
                return;
            } else {
                log::debug!("2f tap: previous right-click expired, flushing");
                self.emit_tap_click(MouseButton::Right);
            }
        }
        log::debug!("2f tap: pending right-click (debouncing for double tap smart zoom)");
        self.pending_right_click = Some(now);
    }

    fn classify(&self, n: usize) -> GestureKind {
        // Drag-lock: while a delayed release is pending, resume the 3-finger drag
        // ONLY if 3 or more fingers land! If 1 or 2 fingers land, let them fall
        // through to OneFinger / TwoFinger to release drag-lock immediately.
        if self.pending_drag_release.is_some() {
            if n >= 4 {
                return GestureKind::FourFingerLive;
            } else if n == 3 || n == 0 {
                return GestureKind::ThreeFingerDrag;
            }
            // n == 1 or n == 2 fall through to normal classification below
        }
        // Once a swipe has fired, stay latched until every finger leaves
        // the pad.
        if matches!(self.kind, GestureKind::SwipeLatched) && n > 0 {
            return GestureKind::SwipeLatched;
        }
        // 三指拖移 mode: an actively engaged drag (mouse button held) outlives
        // async lifts (3 → 2 → 1 → 0). Keep classifying as ThreeFingerDrag while fingers
        // are on pad so async lifts do not reclassify to OneFinger!
        // BUT if user puts down a 4th finger (n >= 4), seamlessly transition to 4-finger desktop swipe!
        if matches!(self.kind, GestureKind::ThreeFingerDrag) && self.drag_button_held {
            if n >= 4 {
                return GestureKind::FourFingerLive;
            } else if n > 0 {
                return GestureKind::ThreeFingerDrag;
            }
        }
        match n {
            0 => GestureKind::Idle,
            1 => GestureKind::OneFinger,
            2 => match self.kind {
                GestureKind::TwoFingerPan
                | GestureKind::TwoFingerPinchAndRotate
                | GestureKind::TwoFingerUnclassified => self.kind,
                _ => GestureKind::TwoFingerUnclassified,
            },
            3 => match self.kind {
                GestureKind::ThreeFingerLive | GestureKind::FourFingerLive => self.kind,
                GestureKind::ThreeFingerDrag => self.kind,
                GestureKind::TwoFingerPan | GestureKind::TwoFingerPinchAndRotate => {
                    GestureKind::ThreeFingerLive
                }
                _ if self.options.three_finger_drag => GestureKind::ThreeFingerDrag,
                _ => GestureKind::ThreeFingerLive,
            },
            _ => GestureKind::FourFingerLive,
        }
    }

    fn transition(&mut self, new_kind: GestureKind, active: &[Contact], now: Timestamp) {

        // First contact after Idle cancels any in-flight scroll inertia.
        // `SwipeLatched → Idle → ...` doesn't count: a deliberate new
        // touch has to come from no-fingers, and the user wants their
        // touch to stop a fling rather than blend into it. Record
        // whether the cancel actually stopped a live coast so the new
        // session is excluded from tap evaluation (rmk-style
        // `born_during_coast`).
        if matches!(self.kind, GestureKind::Idle)
            && !matches!(new_kind, GestureKind::Idle | GestureKind::SwipeLatched)
        {
            if self.out.cancel_inertia() {
                self.born_during_coast = true;
                log::debug!("touch born during coast — suppressing taps for this session");
            }
        }
        if matches!(new_kind, GestureKind::ThreeFingerLive | GestureKind::FourFingerLive | GestureKind::ThreeFingerDrag) {
            self.pending_right_click = None;
        }
        if matches!(self.kind, GestureKind::Idle) && matches!(new_kind, GestureKind::OneFinger) {
            self.tap_drag_active = false;
            if self.options.one_finger_tap_drag {
                if let Some(last_tap) = self.last_1f_tap {
                    let elapsed = now.saturating_duration_since(last_tap);
                    if elapsed <= TAP_DRAG_INTERVAL {
                        // Ambiguous on purpose: this contact is either
                        // the start of a drag or the second half of a
                        // double-click, and nothing observable yet
                        // distinguishes them. Defer the press; the
                        // pointer stays pinned meanwhile so a press
                        // that does come lands on the intended target.
                        self.tap_drag_candidate = true;
                        self.tap_drag_pending_since = Some(now);
                        log::debug!(
                            "1f tap-drag: second tap down (elapsed={}ms) — press deferred pending hold or motion",
                            elapsed.as_millis(),
                        );
                    } else {
                        self.tap_drag_candidate = false;
                    }
                } else {
                    self.tap_drag_candidate = false;
                }
            } else {
                self.tap_drag_candidate = false;
            }
        }
        // Snapshot before the close-out potentially clears it. We want
        // the close-out's tap branches to see the flag the way they were
        // when the lift came in.
        let bc = self.born_during_coast;
        // Snapshot any prior partial-lift continuation state before the
        // close-out runs (so a fresh save on 2F → 1F doesn't get mixed
        // up with a stale rejoin candidate from an earlier partial
        // lift). If we don't end up consuming this for a rejoin, it
        // just gets dropped at the end of this call.
        let prior_recent = self.two_finger_recent.take();
        // Close out the old gesture.
        match self.kind {
            GestureKind::OneFinger => {
                // Drop any deferred cursor motion. On a transition to
                // Idle this is the chip's last with-finger frame's
                // motion (often a centroid-shift artifact); on a
                // transition to TwoFinger* it's stale single-finger
                // motion that's no longer meaningful.
                let dropped = self.pending_motion.take();
                // Reset the sub-pixel carry so a fresh OneFinger
                // session can't inherit a residual pixel from the
                // previous one — without this, a long slow movement
                // followed by a quick second touch could see a
                // visible "jump-on-arrival" worth up to one pixel.
                self.cursor_carry_x_px = 0.0;
                self.cursor_carry_y_px = 0.0;
                // A pending two-finger tap that doesn't get consumed by
                // an Idle transition (e.g. the residual finger gets
                // joined by a third — back to a 2F gesture) must be
                // discarded; the 2F lift sequence is over.
                let pending_2f = self.pending_two_finger_tap.take();
                let pending_3f = self.pending_three_finger_tap.take();
                let suppress_residual = std::mem::take(&mut self.suppress_one_finger_click);
                if matches!(new_kind, GestureKind::Idle) {
                    if self.hold_latched {
                        log::debug!("1f press-and-hold: release left button on lift");
                        self.out.set_left_button_held(false);
                        self.hold_latched = false;
                        self.tap_drag_active = false;
                        self.tap_drag_candidate = false;
                        self.tap_drag_pending_since = None;
                        self.last_1f_tap = None;
                    } else if self.tap_drag_active {
                        if self.drag_button_held {
                            self.out.set_drag_button_held(false);
                            self.drag_button_held = false;
                            log::debug!("1f tap-drag: ended (released drag button)");
                        }
                        self.tap_drag_active = false;
                        self.tap_drag_candidate = false;
                        self.tap_drag_pending_since = None;
                        self.last_1f_tap = None;
                    } else if bc {
                        // Born during coast: nothing this session does
                        // counts as a click. Whether the residual was
                        // also a 2F-tail or a fresh 1F is irrelevant.
                        log::debug!("1f lift, click suppressed (born during coast)");
                        self.last_1f_tap = None;
                        self.tap_drag_candidate = false;
                        self.tap_drag_pending_since = None;
                    } else if self.tap_drag_pending_since.take().is_some() {
                        // The second contact of a tap pair lifted before
                        // committing to a drag — the user double-clicked.
                        // The first tap already posted one click; this is
                        // the second, at the same point and within the
                        // system's double-click interval, so downstream
                        // coalesces the pair.
                        let dur = now - self.started_at;
                        log::debug!(
                            "1f tap-drag: second tap lifted undecided after {}ms — dispatching double-click",
                            dur.as_millis(),
                        );
                        self.emit_tap_click(MouseButton::Left);
                        self.last_1f_tap = None;
                        self.tap_drag_candidate = false;
                    } else if let Some(p3) = pending_3f {
                        self.last_1f_tap = None;
                        self.tap_drag_candidate = false;
                        let total_dur = now - p3.started_at;
                        let combined_max_move = p3.max_move_sq.max(self.max_move_sq).sqrt();
                        if total_dur < Duration::from_millis(420) && combined_max_move < 2.8 {
                            log::debug!(
                                "3f tap (split lift): look up dictionary via Cmd+Ctrl+D (total_dur={}ms combined_max_move={:.2}mm)",
                                total_dur.as_millis(),
                                combined_max_move,
                            );
                            self.emit_dictionary_lookup();
                        }
                    } else if let Some(p) = pending_2f {
                        self.last_1f_tap = None;
                        self.tap_drag_candidate = false;
                        let total_dur = now - p.started_at;
                        let combined_max_move = p.max_move_sq.max(self.max_move_sq).sqrt();
                        if p.initial_distance < FAT_FINGER_SPLIT_MM {
                            // One fat contact the panel reported as two,
                            // then merged back. Resolve it as the single
                            // tap the user actually made.
                            let dur_1f = now - self.started_at;
                            if total_dur < TAP_MAX_DURATION && combined_max_move < TAP_MAX_MOVE_MM {
                                log::debug!(
                                    "2f split-lift reclassified as 1f: contacts only {:.1}mm apart (fat-finger split) — click Left (total_dur={}ms)",
                                    p.initial_distance,
                                    total_dur.as_millis(),
                                );
                                self.emit_tap_click(MouseButton::Left);
                                self.last_1f_tap = Some(now);
                            } else {
                                log::debug!(
                                    "fat-finger split lift, no tap: total_dur={}ms combined_max_move={:.2}mm dur_1f={}ms",
                                    total_dur.as_millis(),
                                    combined_max_move,
                                    dur_1f.as_millis(),
                                );
                            }
                        } else if total_dur < TAP_MAX_DURATION
                            && combined_max_move < TAP_MAX_MOVE_MM
                        {
                            self.on_two_finger_tap_lift(now);
                        } else {
                            // If total duration exceeded or moved, it was NOT a 2F tap!
                            // Fallback to evaluating the residual 1F touch as a normal 1F tap so single taps are never lost!
                            let dur_1f = now - self.started_at;
                            let max_move_1f = self.max_move_sq.sqrt();
                            if dur_1f < TAP_MAX_DURATION && max_move_1f < TAP_MAX_MOVE_MM {
                                log::debug!(
                                    "1f tap after canceled 2f split: click Left (dur={}ms)",
                                    dur_1f.as_millis()
                                );
                                self.emit_tap_click(MouseButton::Left);
                                self.last_1f_tap = Some(now);
                            }
                        }
                    } else if suppress_residual {
                        self.last_1f_tap = None;
                        self.tap_drag_candidate = false;
                        // Residual 1F is the tail of a non-tap 2F (a
                        // pan, pinch, rotate, or motion-disqualified
                        // unclassified). User didn't intend a 1F tap.
                        log::debug!("1f lift, click suppressed (residual after 2f gesture)");
                    } else {
                        let dur = now - self.started_at;
                        let max_move = self.max_move_sq.sqrt();
                        if dur < TAP_MAX_DURATION && max_move < TAP_MAX_MOVE_MM {
                            log::debug!(
                                "1f tap: click Left (dur={}ms max_move={:.2}mm{})",
                                dur.as_millis(),
                                max_move,
                                if dropped.is_some() {
                                    ", dropped lift-frame motion"
                                } else {
                                    ""
                                },
                            );
                            self.emit_tap_click(MouseButton::Left);
                            self.last_1f_tap = Some(now);
                        } else {
                            self.last_1f_tap = None;
                            log::debug!(
                                "1f lift, no tap: dur={}ms max_move={:.2}mm (limits dur<{}ms move<{:.2}mm)",
                                dur.as_millis(),
                                max_move,
                                TAP_MAX_DURATION.as_millis(),
                                TAP_MAX_MOVE_MM,
                            );
                        }
                        self.tap_drag_candidate = false;
                    }
                }
            }
            GestureKind::TwoFingerPan => {
                // Seed from the peak velocity inside the tail window, not
                // the final frame's. Fingers decelerate on their way off
                // the surface — real logs show a scroll that peaked at
                // 560 mm/s reporting 7 mm/s on the frame it ended, which
                // is below every sane inertia threshold, so a genuine
                // flick coasted nowhere. A peak older than the window is
                // ignored: the user slowed down deliberately.
                let (vx, vy) = self
                    .two_baseline
                    .map(|b| {
                        let fresh_peak = b
                            .peak_velocity_at
                            .map(|t| now.saturating_duration_since(t) <= INERTIA_PEAK_WINDOW)
                            .unwrap_or(false);
                        if fresh_peak {
                            b.peak_velocity
                        } else {
                            b.scroll_velocity
                        }
                    })
                    .unwrap_or((0.0, 0.0));
                let speed = (vx * vx + vy * vy).sqrt();
                log::debug!(
                    "scroll: ended (v=({:+.0},{:+.0})mm/s speed={:.0}mm/s)",
                    vx,
                    vy,
                    speed,
                );
                self.emit_scroll(0.0, 0.0, Phase::Ended);
                // Seed inertia from the lift velocity. `Output` decides
                // whether the seed is fast enough to coast on; gesture-side
                // we always offer it.
                self.emit_scroll_inertia(vx, vy);
                if matches!(new_kind, GestureKind::Idle) {
                    if let Some(base) = self.two_baseline {
                        if base.right_edge_candidate && base.cumulative_dx <= -5.0 {
                            log::debug!(
                                "2f right edge swipe: toggle notification center (cumulative_dx={:.2}mm)",
                                base.cumulative_dx
                            );
                            self.out.toggle_notification_center();
                        }
                    }
                }
                if matches!(
                    new_kind,
                    GestureKind::ThreeFingerLive | GestureKind::FourFingerLive
                ) {
                    // A third/fourth contact is a new multi-finger
                    // gesture, not a request to keep the old scroll
                    // momentum running underneath it.
                    let _ = self.out.cancel_inertia();
                }
                // Async lift: if one finger lifted before the other,
                // the residual goes 2F-pan → 1F. That residual is the
                // tail of the gesture, not a fresh tap. Capture
                // continuation state in case the chip's drop-out is
                // momentary (typical on TPS65) and the next frame or
                // two restore 2F — we want to resume the scroll lock
                // rather than start a fresh classification that
                // catches one finger mid-glide.
                if matches!(new_kind, GestureKind::OneFinger) {
                    self.suppress_one_finger_click = true;
                    self.capture_partial_lift(active, now);
                }
            }
            GestureKind::TwoFingerPinchAndRotate => {
                log::debug!("pinch+rotate: ended");
                if self
                    .two_baseline
                    .map(|b| b.pinch_admitted)
                    .unwrap_or(true)
                {
                    self.out.pinch(0.0, Phase::Ended);
                }
                if self
                    .two_baseline
                    .map(|b| b.rotate_admitted)
                    .unwrap_or(true)
                {
                    self.out.rotate(0.0, Phase::Ended);
                }
                if matches!(new_kind, GestureKind::OneFinger) {
                    self.suppress_one_finger_click = true;
                    self.capture_partial_lift(active, now);
                }
            }
            GestureKind::TwoFingerUnclassified => {
                let dur = now - self.started_at;
                let max_move = self.max_move_sq.sqrt();
                let tap_eligible = dur < TAP_MAX_DURATION && max_move < TAP_MAX_MOVE_MM;
                // Two contacts that were never far enough apart to be two
                // fingers are one fat contact the panel split in two. Such
                // a touch must never produce a right-click; the user made
                // an ordinary single-finger tap.
                let split_distance = self
                    .two_baseline
                    .map(|b| b.initial_distance)
                    .unwrap_or(f64::INFINITY);
                let fat_finger_split = split_distance < FAT_FINGER_SPLIT_MM;
                // A three-finger tap that unloads 3 → 2 → 0 lands here,
                // not in the OneFinger arm, so the pending lookup has to
                // be consumed on this path too. If transitioning to OneFinger (3 -> 2 -> 1),
                // keep pending_three_finger_tap intact so the subsequent OneFinger -> Idle can consume it!
                let pending_3f = if matches!(new_kind, GestureKind::Idle) {
                    self.pending_three_finger_tap.take()
                } else {
                    None
                };
                if matches!(new_kind, GestureKind::Idle) {
                    if let Some(p3) = pending_3f {
                        let total_dur = now - p3.started_at;
                        let combined_max_move = p3.max_move_sq.max(self.max_move_sq).sqrt();
                        if !bc && total_dur < Duration::from_millis(420) && combined_max_move < 2.8
                        {
                            log::debug!(
                                "3f tap (split lift via 2f): look up dictionary via Cmd+Ctrl+D (total_dur={}ms combined_max_move={:.2}mm)",
                                total_dur.as_millis(),
                                combined_max_move,
                            );
                            self.emit_dictionary_lookup();
                        } else {
                            log::debug!(
                                "3f split lift via 2f, no lookup: total_dur={}ms combined_max_move={:.2}mm",
                                total_dur.as_millis(),
                                combined_max_move,
                            );
                        }
                    } else if bc {
                        log::debug!(
                            "2f lift, click suppressed (born during coast; dur={}ms max_move={:.2}mm)",
                            dur.as_millis(),
                            max_move,
                        );
                    } else if tap_eligible && fat_finger_split {
                        log::debug!(
                            "2f tap reclassified as 1f: contacts only {:.1}mm apart (fat-finger split) — click Left",
                            split_distance,
                        );
                        self.emit_tap_click(MouseButton::Left);
                        self.last_1f_tap = Some(now);
                        self.pending_right_click = None;
                    } else if tap_eligible {
                        self.on_two_finger_tap_lift(now);
                    } else {
                        log::debug!(
                            "2f lift, no tap: dur={}ms max_move={:.2}mm",
                            dur.as_millis(),
                            max_move,
                        );
                    }
                } else if matches!(new_kind, GestureKind::OneFinger) {
                    if self.pending_three_finger_tap.is_some() {
                        log::debug!(
                            "2f → 1f partial lift: preserving pending 3f tap lookup pipeline"
                        );
                    } else if bc || !tap_eligible || fat_finger_split {
                        log::debug!(
                            "2f → 1f partial lift (dur={}ms max_move={:.2}mm fat_split={}); suppressing residual click{}",
                            dur.as_millis(),
                            max_move,
                            fat_finger_split,
                            if bc { " (born during coast)" } else { "" },
                        );
                        if !fat_finger_split {
                            self.suppress_one_finger_click = true;
                        }
                        if !bc && !fat_finger_split {
                            self.capture_partial_lift(active, now);
                        }
                    } else {
                        log::debug!(
                            "2f → 1f partial lift (dur={}ms max_move={:.2}mm); pending right-click",
                            dur.as_millis(),
                            max_move,
                        );
                        self.pending_two_finger_tap = Some(PendingTwoFingerTap {
                            started_at: self.started_at,
                            max_move_sq: self.max_move_sq,
                            initial_distance: split_distance,
                        });
                    }
                }
            }
            GestureKind::ThreeFingerDrag => {
                let dur = now - self.started_at;
                let max_move = self.max_move_sq.sqrt();
                let tap_eligible = dur < Duration::from_millis(380) && max_move < 2.5;
                if !self.drag_button_held && tap_eligible && !bc {
                    if matches!(new_kind, GestureKind::Idle) {
                        log::debug!(
                            "3f tap: look up dictionary via Cmd+Ctrl+D (dur={}ms max_move={:.2}mm)",
                            dur.as_millis(),
                            max_move
                        );
                        self.emit_dictionary_lookup();
                    } else {
                        log::debug!(
                            "3f drag → partial lift: pending 3f tap lookup (dur={}ms max_move={:.2}mm)",
                            dur.as_millis(),
                            max_move
                        );
                        self.pending_three_finger_tap = Some(PendingThreeFingerTap {
                            started_at: self.started_at,
                            max_move_sq: self.max_move_sq,
                        });
                        self.suppress_one_finger_click = true;
                    }
                }
                // If transitioning to Idle (all fingers lifted from pad), arm the drag-lock delay.
                if matches!(new_kind, GestureKind::Idle)
                    && self.drag_button_held
                    && self.options.release_delay_ms > 0
                {
                    let expiry = now + Duration::from_millis(self.options.release_delay_ms);
                    if self.pending_drag_release.is_none() {
                        log::debug!(
                            "3f drag: fingers lifted — release armed at +{}ms",
                            self.options.release_delay_ms
                        );
                    }
                    self.pending_drag_release = Some(expiry);
                    return; // keep kind, baselines, held state intact
                }
                // If transitioning to FourFingerLive (e.g. 4F swipe to switch desktop),
                // KEEP the left button held so the dragged window travels across Spaces with the cursor!
                if matches!(new_kind, GestureKind::FourFingerLive) {
                    if self.drag_button_held {
                        log::debug!(
                            "3f drag → 4f swipe: keeping drag button held to carry window across Spaces"
                        );
                    }
                } else if self.drag_button_held {
                    self.out.set_drag_button_held(false);
                    self.drag_button_held = false;
                    log::debug!("3f drag: ended (button released, next: {:?})", new_kind);
                }
                self.pending_drag_release = None;
                self.drag_baseline = None;
                self.pending_motion = None;
                if !matches!(new_kind, GestureKind::Idle | GestureKind::ThreeFingerDrag) {
                    self.suppress_one_finger_click = true;
                }
            }
            GestureKind::ThreeFingerLive | GestureKind::FourFingerLive => {
                if self.drag_button_held
                    && matches!(new_kind, GestureKind::Idle | GestureKind::SwipeLatched)
                {
                    self.out.set_drag_button_held(false);
                    self.drag_button_held = false;
                    log::debug!("4f swipe lift: released carried drag button in new Space");
                }
                let swipe_in_flight = self
                    .multi_baseline
                    .as_ref()
                    .map(|b| b.began_posted)
                    .unwrap_or(false);
                if let Some(b) = self.multi_baseline
                    && b.began_posted
                    && let Some(axis) = b.axis
                {
                    let cumulative_mm = match axis {
                        SwipeAxis::Horizontal => b.cumulative_dx,
                        SwipeAxis::Vertical => b.cumulative_dy,
                    };
                    let progress = (cumulative_mm / SWIPE_PROGRESS_REF_MM).clamp(-1.0, 1.0);
                    let velocity = match axis {
                        SwipeAxis::Horizontal => b.velocity.0,
                        SwipeAxis::Vertical => b.velocity.1,
                    };
                    log::debug!(
                        "swipe ({:?}): ended progress={:+.3} v={:+.1}",
                        axis,
                        progress,
                        velocity
                    );
                    self.out.swipe(axis, progress, velocity, Phase::Ended);
                    self.kind = GestureKind::SwipeLatched;
                    self.started_at = now;
                    self.max_move_sq = 0.0;
                    self.two_baseline = None;
                    self.multi_baseline = None;
                    return;
                } else if matches!(self.kind, GestureKind::ThreeFingerLive) && !swipe_in_flight {
                    let dur = now - self.started_at;
                    let max_move = self.max_move_sq.sqrt();
                    let tap_eligible = dur < Duration::from_millis(380) && max_move < 2.5;
                    if tap_eligible && !bc {
                        if matches!(new_kind, GestureKind::Idle) {
                            log::debug!(
                                "3f tap (live): look up dictionary via Cmd+Ctrl+D (dur={}ms max_move={:.2}mm)",
                                dur.as_millis(),
                                max_move
                            );
                            self.emit_dictionary_lookup();
                        } else {
                            log::debug!(
                                "3f live → partial lift: pending 3f tap lookup (dur={}ms max_move={:.2}mm)",
                                dur.as_millis(),
                                max_move
                            );
                            self.pending_three_finger_tap = Some(PendingThreeFingerTap {
                                started_at: self.started_at,
                                max_move_sq: self.max_move_sq,
                            });
                            self.suppress_one_finger_click = true;
                        }
                    }
                }
            }
            _ => {}
        }

        // A tap-drag candidacy only survives while the contact that
        // opened it is still the whole gesture. Anything else — a second
        // finger joining, a full lift — resolves it.
        if !matches!(new_kind, GestureKind::OneFinger) {
            self.tap_drag_pending_since = None;
        }

        self.kind = new_kind;
        self.started_at = now;
        self.max_move_sq = 0.0;
        self.two_baseline = None;
        self.multi_baseline = None;
        self.drag_baseline = None;
        // Sub-pixel carries belong to whichever mode streams cursor
        // motion (`OneFinger` / `ThreeFingerDrag`); a kind switch means
        // a fresh stream either way.
        self.cursor_carry_x_px = 0.0;
        self.cursor_carry_y_px = 0.0;
        // `born_during_coast` is a session-level flag. Clear it once
        // the user has fully lifted; surviving gesture sub-transitions
        // (e.g. OneFinger → TwoFingerUnclassified during a roll-on) is
        // what keeps post-coast taps suppressed across kind changes.
        if matches!(new_kind, GestureKind::Idle) {
            self.born_during_coast = false;
            if self.drag_button_held {
                self.out.set_drag_button_held(false);
                self.drag_button_held = false;
                log::debug!("all fingers lifted: released held drag button (failsafe)");
            }
        }

        match new_kind {
            GestureKind::TwoFingerUnclassified if active.len() == 2 => {
                // Try to continue the previous 2F gesture if this is a
                // rejoin within the partial-lift window. Falls back to
                // fresh-baseline classification if not eligible.
                if let Some(recent) = prior_recent
                    && self.try_restore_partial_lift(recent, active, now)
                {
                    return;
                }
                let a = active[0];
                let b = active[1];
                let centroid = ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let dist = (dx * dx + dy * dy).sqrt().max(1e-9);
                let ang = dy.atan2(dx);
                let pinch_admitted = self.out.pinch_admissible_now();
                let rotate_admitted = self.out.rotate_admissible_now();
                if !pinch_admitted || !rotate_admitted {
                    log::debug!(
                        "2F gesture start: admit pinch={pinch_admitted} rotate={rotate_admitted}"
                    );
                }
                let right_edge_candidate =
                    a.x >= 28.0 || b.x >= 28.0 || (a.x >= 0.65 && a.x <= 1.0) || (b.x >= 0.65 && b.x <= 1.0);
                self.two_baseline = Some(TwoFingerBaseline {
                    initial_distance: dist,
                    initial_angle: ang,
                    initial_a: (a.id, (a.x, a.y)),
                    initial_b: (b.id, (b.x, b.y)),
                    last_centroid: centroid,
                    frames_observed: 0,
                    peak_velocity: (0.0, 0.0),
                    peak_velocity_at: None,
                    prev_scale: 1.0,
                    prev_angle: ang,
                    scroll_velocity: (0.0, 0.0),
                    last_scroll_time: None,
                    prev_transform_at: None,
                    pinch_rot_lock_pending: false,
                    pinch_admitted,
                    rotate_admitted,
                    cumulative_dx: 0.0,
                    right_edge_candidate,
                    right_edge_latched: false,
                });
            }
            GestureKind::ThreeFingerDrag => {
                let cx: f64 = active.iter().map(|c| c.x).sum::<f64>() / active.len() as f64;
                let cy: f64 = active.iter().map(|c| c.y).sum::<f64>() / active.len() as f64;
                log::debug!("3f drag: session start (centroid=({cx:.2},{cy:.2})mm)");
                self.drag_baseline = Some(DragBaseline {
                    initial_centroid: (cx, cy),
                    last_centroid: (cx, cy),
                    finger_count: active.len(),
                    engaged: false,
                    regrip_started_at: None,
                });
            }
            GestureKind::ThreeFingerLive | GestureKind::FourFingerLive => {
                let cx: f64 = active.iter().map(|c| c.x).sum::<f64>() / active.len() as f64;
                let cy: f64 = active.iter().map(|c| c.y).sum::<f64>() / active.len() as f64;
                let swipe_horizontal_admitted =
                    self.out.swipe_admissible_now(SwipeAxis::Horizontal);
                let swipe_vertical_admitted = self.out.swipe_admissible_now(SwipeAxis::Vertical);
                if !swipe_horizontal_admitted || !swipe_vertical_admitted {
                    log::debug!(
                        "multi-finger gesture start: admit swipe.h={swipe_horizontal_admitted} \
                         swipe.v={swipe_vertical_admitted}"
                    );
                }
                let initial_radial_spread = compute_radial_spread(active, (cx, cy));
                self.multi_baseline = Some(MultiBaseline {
                    _initial_centroid: (cx, cy),
                    finger_count: active.len(),
                    cumulative_dx: 0.0,
                    cumulative_dy: 0.0,
                    axis: None,
                    began_posted: false,
                    last_centroid: (cx, cy),
                    last_centroid_time: None,
                    velocity: (0.0, 0.0),
                    swipe_horizontal_admitted,
                    swipe_vertical_admitted,
                    initial_radial_spread,
                    radial_action_latched: false,
                });
            }
            _ => {}
        }
    }

    /// Stash continuation state at a 2F → 1F transition so the next
    /// 1F → 2F can resume the gesture (see [`TwoFingerRecent`]).
    /// Called from the close-out branches of TwoFingerPan,
    /// TwoFingerPinchAndRotate, and non-tap-eligible
    /// TwoFingerUnclassified when the new kind is OneFinger.
    fn capture_partial_lift(&mut self, active: &[Contact], now: Timestamp) {
        if active.len() != 1 {
            return;
        }
        let Some(baseline) = self.two_baseline else {
            return;
        };
        let surviving = active[0];
        self.two_finger_recent = Some(TwoFingerRecent {
            baseline,
            kind: self.kind,
            started_at: self.started_at,
            max_move_sq: self.max_move_sq,
            surviving_id: surviving.id,
            surviving_pos: (surviving.x, surviving.y),
            lift_time: now,
        });
    }

    /// Resume a 2F gesture across a brief 1F gap. Returns true if the
    /// rejoin candidate is fresh enough and the surviving contact
    /// matches; on success this also sets `self.kind`,
    /// `self.two_baseline`, `self.started_at`, `self.max_move_sq` and
    /// re-emits Began for any locked stream that was ended at lift.
    fn try_restore_partial_lift(
        &mut self,
        recent: TwoFingerRecent,
        active: &[Contact],
        now: Timestamp,
    ) -> bool {
        if active.len() != 2 {
            return false;
        }
        let age = now.saturating_duration_since(recent.lift_time);
        if age > PARTIAL_LIFT_REJOIN_WINDOW {
            log::debug!(
                "partial-lift rejoin rejected: age={}ms > {}ms window",
                age.as_millis(),
                PARTIAL_LIFT_REJOIN_WINDOW.as_millis(),
            );
            return false;
        }
        let a = active[0];
        let b = active[1];
        let surviving = if a.id == recent.surviving_id {
            a
        } else if b.id == recent.surviving_id {
            b
        } else {
            log::debug!(
                "partial-lift rejoin rejected: surviving id={} absent from active=[{}, {}]",
                recent.surviving_id,
                a.id,
                b.id,
            );
            return false;
        };
        let drift = {
            let dx = surviving.x - recent.surviving_pos.0;
            let dy = surviving.y - recent.surviving_pos.1;
            (dx * dx + dy * dy).sqrt()
        };
        if drift > PARTIAL_LIFT_REJOIN_DRIFT_MM {
            log::debug!(
                "partial-lift rejoin rejected: surviving drift={:.2}mm > {:.2}mm",
                drift,
                PARTIAL_LIFT_REJOIN_DRIFT_MM,
            );
            return false;
        }

        let mut baseline = recent.baseline;
        let centroid = ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let dist = (dx * dx + dy * dy).sqrt().max(1e-9);
        let ang = dy.atan2(dx);
        // Reset all per-finger / inter-finger anchors to the rejoin
        // frame so the first post-rejoin frame is a one-frame delta,
        // not a teleport from the pre-lift geometry. For locked kinds
        // (Pan, PinchAndRotate) the dispatch path doesn't read
        // `initial_a`/`initial_b` anyway — only `last_centroid` (Pan)
        // and `initial_distance`/`initial_angle`/`prev_*` (PinchAndRotate).
        // For Unclassified, the lock-decision math *does* read them;
        // resetting both means we evaluate motion from a clean
        // baseline rather than letting the surviving finger's
        // accumulated pre-lift displacement swamp the rearriving
        // finger's near-zero one.
        baseline.initial_a = (a.id, (a.x, a.y));
        baseline.initial_b = (b.id, (b.x, b.y));
        baseline.initial_distance = dist;
        baseline.initial_angle = ang;
        baseline.last_centroid = centroid;
        // A rejoin re-enters classification for an unclassified gesture,
        // so the landing-frame grace applies again. Locked kinds never
        // consult this counter.
        baseline.frames_observed = 0;
        baseline.prev_scale = 1.0;
        baseline.prev_angle = ang;
        baseline.pinch_rot_lock_pending = false;
        // EMA velocity carries across the gap unchanged — if scroll
        // resumes immediately, the inertia seed should reflect the
        // pre-lift motion, not start from zero.

        self.kind = recent.kind;
        self.started_at = recent.started_at;
        if recent.max_move_sq > self.max_move_sq {
            self.max_move_sq = recent.max_move_sq;
        }
        self.two_baseline = Some(baseline);

        log::debug!(
            "partial-lift rejoin: resumed kind={:?} (gap={}ms surviving_drift={:.2}mm)",
            recent.kind,
            age.as_millis(),
            drift,
        );

        // Locked kinds emitted Phase::Ended at the partial-lift save
        // (close-out branches for TwoFingerPan / TwoFingerPinchAndRotate
        // unconditionally emit Ended), so downstream needs a fresh
        // Began to continue the stream. Pan also kicked
        // `scroll_inertia`; cancel any live coast so the resumed
        // scroll doesn't compete with it.
        match recent.kind {
            GestureKind::TwoFingerPan => {
                let _ = self.out.cancel_inertia();
                self.emit_scroll(0.0, 0.0, Phase::Began);
            }
            GestureKind::TwoFingerPinchAndRotate => {
                self.out.pinch(0.0, Phase::Began);
                self.out.rotate(0.0, Phase::Began);
            }
            _ => {}
        }
        true
    }

    fn dispatch(&mut self, active: &[Contact], now: Timestamp, frame_dt: Duration) {
        match self.kind {
            GestureKind::Idle | GestureKind::SwipeLatched => {}
            GestureKind::OneFinger => self.dispatch_one(active, now, frame_dt),
            GestureKind::TwoFingerUnclassified
            | GestureKind::TwoFingerPan
            | GestureKind::TwoFingerPinchAndRotate => self.dispatch_two(active, now),
            GestureKind::ThreeFingerDrag => self.dispatch_drag(active, now, frame_dt),
            GestureKind::ThreeFingerLive | GestureKind::FourFingerLive => {
                self.dispatch_swipe(active, now)
            }
        }
    }

    /// 三指拖移 streaming. Centroid deltas drive the same accelerated
    /// cursor pipeline as single-finger motion; once cumulative centroid
    /// travel crosses [`DRAG_ENGAGE_MM`], the left button posts held and
    /// every subsequent delta rides out as a mouse-drag.
    fn dispatch_drag(&mut self, active: &[Contact], now: Timestamp, frame_dt: Duration) {
        if active.is_empty() {
            return;
        }
        // Fingers re-landed while a delayed release was armed: grip
        // adjustment — cancel the release and continue the same drag.
        let was_regrip = self.pending_drag_release.take().is_some();
        let Some(mut base) = self.drag_baseline else {
            return;
        };
        let cx = active.iter().map(|c| c.x).sum::<f64>() / active.len() as f64;
        let cy = active.iter().map(|c| c.y).sum::<f64>() / active.len() as f64;

        // There was a real empty-pad gap between these samples. Reset the
        // centroid anchor and discard any pre-gap state so a re-grip at a
        // different place does not teleport the pointer when drag-lock
        // resumes.
        if was_regrip {
            base.last_centroid = (cx, cy);
            base.finger_count = active.len();
            base.regrip_started_at = Some(now);
            self.pending_motion = None;
            self.cursor_carry_x_px = 0.0;
            self.cursor_carry_y_px = 0.0;
            self.drag_baseline = Some(base);
            return;
        }

        // If only 1 finger touched down during a regrip:
        if base.finger_count == 1 && active.len() == 1 {
            if let Some(re_t) = base.regrip_started_at {
                let dur = now.saturating_duration_since(re_t);
                let drift = ((cx - base.last_centroid.0).powi(2)
                    + (cy - base.last_centroid.1).powi(2))
                .sqrt();
                if dur > Duration::from_millis(50) || drift > TAP_MAX_MOVE_MM {
                    log::debug!("3f drag: 1-finger interrupt during regrip — releasing drag lock");
                    if self.drag_button_held {
                        self.out.set_drag_button_held(false);
                        self.drag_button_held = false;
                    }
                    self.drag_baseline = None;
                    self.kind = GestureKind::OneFinger;
                    self.started_at = now;
                    return;
                }
            }
            return;
        }

        // Per-frame centroid delta from ALL surviving fingers. When the
        // finger count changes (async lift 3 → 2 → 1), the centroid mean
        // jumps because the cluster changed shape. This has to be handled
        // *before* the engage gate, not after: the jump is routinely
        // several millimetres — lifting one of three fingers spaced 15 mm
        // apart moves the mean by 7.5 mm — so an un-engaged drag would
        // read it as travel and press the button on an async lift the
        // user meant as a three-finger tap. Re-anchor both the engage
        // reference and the streaming reference, then stream cleanly
        // against the new cluster from the next frame.
        if base.finger_count != active.len() {
            base.finger_count = active.len();
            base.initial_centroid = (cx, cy);
            base.last_centroid = (cx, cy);
            base.regrip_started_at = None;
            self.drag_baseline = Some(base);
            return;
        }

        if !base.engaged {
            let travel = ((cx - base.initial_centroid.0).powi(2)
                + (cy - base.initial_centroid.1).powi(2))
            .sqrt();
            // Pre-engage jitter pins the baseline to the landing
            // centroid, so none of it leaks into the first emitted
            // delta once we cross.
            if travel >= DRAG_ENGAGE_MM {
                log::debug!(
                    "3f drag: engage at travel={travel:.2}mm (centroid=({cx:.2},{cy:.2})mm)"
                );
                self.out.haptic(HapticKind::DragEngaged);
                self.out.set_drag_button_held(true);
                self.drag_button_held = true;
                base.engaged = true;
                base.last_centroid = (cx, cy);
            }
            self.drag_baseline = Some(base);
            if !base.engaged {
                return;
            }
        }

        let dx = cx - base.last_centroid.0;
        let dy = cy - base.last_centroid.1;
        base.last_centroid = (cx, cy);
        self.drag_baseline = Some(base);

        // Dragging is already committed, so there is no tap/lift artifact
        // to protect against. Emit this frame immediately; deferring it
        // would add latency and would drop the final useful delta on lift.
        if dx.abs() > MOTION_DEAD_ZONE_MM || dy.abs() > MOTION_DEAD_ZONE_MM {
            let (dx_px, dy_px) = self.cursor_pixels_for(dx, dy, frame_dt);
            if dx_px != 0 || dy_px != 0 {
                log::debug!("3f drag: emit d=({dx:+.3},{dy:+.3})mm → ({dx_px:+},{dy_px:+})px");
                self.out.move_cursor_by(dx_px, dy_px);
            }
        }
    }

    fn dispatch_one(&mut self, active: &[Contact], now: Timestamp, frame_dt: Duration) {
        let c = active[0];
        let Some(tr) = self.contacts.get(&c.id).copied() else {
            return;
        };

        // While inside the partial-lift rejoin window the residual 1F
        // is the tail of a 2F gesture — emitting cursor motion would
        // jump the pointer mid-scroll. Once the window expires
        // without a rejoin, clear the stash so subsequent frames
        // resume normal cursor behavior, but drop the in-window
        // motion entirely (it belongs to the 2F gesture, not the 1F).
        if let Some(recent) = self.two_finger_recent {
            if now.saturating_duration_since(recent.lift_time) <= PARTIAL_LIFT_REJOIN_WINDOW {
                self.pending_motion = None;
                return;
            }
            self.two_finger_recent = None;
        }

        // State-leak safeguard: if single finger moved (> 0.4mm) or stayed on screen (> 150ms),
        // it is unmistakably an independent 1F session. Clear any stale pending 2F tap or suppression!
        let move_1f = tr.max_move_sq.sqrt();
        let dur_1f = now.saturating_duration_since(tr.down_at);
        if move_1f > 0.4 || dur_1f > Duration::from_millis(150) {
            self.flush_pending_right_click();
            if self.pending_two_finger_tap.is_some() {
                log::debug!(
                    "1f motion/hold: cleared stale pending 2f tap (move={:.2}mm dur={}ms)",
                    move_1f,
                    dur_1f.as_millis(),
                );
                self.pending_two_finger_tap = None;
            }
            self.suppress_one_finger_click = false;
        }

        // Tap-drag candidate: commit to the drag once this contact has
        // either travelled far enough to be a drag or been held long
        // enough to rule out a double-click. `DRAG_ENGAGE_MM` rather
        // than the tap budget keeps the pointer within a third of a
        // millimetre of where the user aimed, which is what the
        // press-on-landing-frame change was trying to achieve.
        if self.tap_drag_candidate && !self.drag_button_held {
            let max_move = tr.max_move_sq.sqrt();
            let held = self
                .tap_drag_pending_since
                .map(|t| now.saturating_duration_since(t))
                .unwrap_or_default();
            if max_move >= DRAG_ENGAGE_MM || held >= TAP_DRAG_CONFIRM {
                self.out.haptic(HapticKind::DragEngaged);
                self.out.set_drag_button_held(true);
                self.drag_button_held = true;
                self.tap_drag_active = true;
                self.tap_drag_pending_since = None;
                log::debug!(
                    "1f tap-drag: engaged (max_move={max_move:.2}mm held={}ms)",
                    held.as_millis(),
                );
            } else if self.tap_drag_pending_since.is_some() {
                // Still undecided. Drop this frame's motion so the
                // pointer does not creep off the target while we wait.
                self.pending_motion = None;
                return;
            }
        }

        // Press-and-hold drag detection:
        // Stationary single finger held >= HOLD_TIME (450ms) latches left button
        const HOLD_TIME: Duration = Duration::from_millis(450);
        if self.options.press_and_hold_drag
            && !self.hold_latched
            && !self.tap_drag_candidate
            && !self.tap_drag_active
            && self.contacts.len() == 1
        {
            let dur = now.saturating_duration_since(tr.down_at);
            let max_move = tr.max_move_sq.sqrt();
            if dur >= HOLD_TIME && max_move <= TAP_MAX_MOVE_MM {
                log::debug!("1f press-and-hold: latched left button held=true");
                self.out.set_left_button_held(true);
                self.hold_latched = true;
            }
        }

        let dx = tr.x - tr.prev_x;
        let dy = tr.y - tr.prev_y;
        // Emit the previous frame's deferred motion (if any), then
        // stash this frame's. On lift the `transition` arm clears
        // `pending_motion` without emitting it — that's what drops
        // the centroid-shift jump that capacitive trackpads commonly
        // report on the last with-finger frame.
        if let Some((bdx, bdy, bdt)) = self.pending_motion.take() {
            if bdx.abs() > MOTION_DEAD_ZONE_MM || bdy.abs() > MOTION_DEAD_ZONE_MM {
                let (dx_px, dy_px) = self.cursor_pixels_for(bdx, bdy, bdt);
                if dx_px != 0 || dy_px != 0 {
                    log::debug!(
                        "cursor: emit deferred d=({:+.3},{:+.3})mm → ({:+},{:+})px \
                         (cur frame at=({:.2},{:.2})mm)",
                        bdx,
                        bdy,
                        dx_px,
                        dy_px,
                        c.x,
                        c.y,
                    );
                    self.out.move_cursor_by(dx_px, dy_px);
                }
            }
        }
        self.pending_motion = Some((dx, dy, frame_dt));
    }

    /// Run the cursor-acceleration curve on a per-frame mm delta and
    /// return integer pixel deltas, carrying the sub-pixel residual
    /// across frames in `cursor_carry_*_px`. The gain is computed from the
    /// vector magnitude and applied to both components, matching native
    /// direction-preserving pointer acceleration.
    fn cursor_pixels_for(&mut self, dx_mm: f64, dy_mm: f64, dt: Duration) -> (i32, i32) {
        let dt_s = dt.as_secs_f64();
        // dt is clamped in `on_frame_at`, so this can't be zero. But
        // if a future caller threads a different value through, fall
        // back to a no-op rather than producing NaN.
        if dt_s <= 0.0 {
            return (0, 0);
        }
        // A fingertip cannot cross a trackpad faster than roughly a
        // metre per second. Anything above that is a data fault — a
        // dropped frame the transport didn't flag, a contact-id reuse,
        // or a client sending a stale coordinate — and feeding it to the
        // acceleration curve throws the pointer across the screen (one
        // real capture: a 10.4 mm single-frame delta became 1666 px).
        // Scale such a frame back to the speed limit and say so, so the
        // log identifies which path produced it instead of the user just
        // seeing the cursor vanish.
        let speed = (dx_mm * dx_mm + dy_mm * dy_mm).sqrt() / dt_s;
        let (dx_mm, dy_mm) = if speed > MAX_FINGER_SPEED_MM_S {
            let scale = MAX_FINGER_SPEED_MM_S / speed;
            log::warn!(
                "cursor: implausible frame d=({dx_mm:+.3},{dy_mm:+.3})mm over {:.1}ms \
                 = {speed:.0}mm/s — clamped to {MAX_FINGER_SPEED_MM_S:.0}mm/s",
                dt_s * 1000.0,
            );
            (dx_mm * scale, dy_mm * scale)
        } else {
            (dx_mm, dy_mm)
        };
        let vx = dx_mm / dt_s;
        let vy = dy_mm / dt_s;
        let (px_per_sec_x, px_per_sec_y) = accelerate_cursor_vector(vx, vy, self.cursor_accel);
        let px_x = px_per_sec_x * dt_s;
        let px_y = px_per_sec_y * dt_s;
        let total_x = self.cursor_carry_x_px + px_x;
        let total_y = self.cursor_carry_y_px + px_y;
        let int_x = total_x.trunc() as i32;
        let int_y = total_y.trunc() as i32;
        self.cursor_carry_x_px = total_x - f64::from(int_x);
        self.cursor_carry_y_px = total_y - f64::from(int_y);
        (int_x, int_y)
    }

    fn dispatch_two(&mut self, active: &[Contact], now: Timestamp) {
        if active.len() != 2 {
            return;
        }
        let Some(mut base) = self.two_baseline else {
            return;
        };
        let (a, b) = if active[0].id == base.initial_a.0 {
            (active[0], active[1])
        } else if active[1].id == base.initial_a.0 {
            (active[1], active[0])
        } else if active[0].id <= active[1].id {
            (active[0], active[1])
        } else {
            (active[1], active[0])
        };
        let centroid = ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let dist = (dx * dx + dy * dy).sqrt().max(1e-9);
        let ang = dy.atan2(dx);

        // Lock mode if not yet locked. Same could-still-tap gate as
        // dispatch_one: PAN_LOCK_MM (0.4) sits below TAP_MAX_MOVE_MM
        // (1.0), so without this check a 2F tap with synchronized
        // sub-mm centroid drift would lock pan mid-tap and start
        // emitting scroll events — and the right-click would never
        // fire on lift, since the kind would no longer be
        // TwoFingerUnclassified. `self.max_move_sq` tracks the worst
        // per-contact drift across the gesture, so it correctly gates
        // on either finger crossing the tap budget.
        if matches!(self.kind, GestureKind::TwoFingerUnclassified) {
            base.frames_observed = base.frames_observed.saturating_add(1);
            let max_move = self.max_move_sq.sqrt();
            let dur = now - self.started_at;
            // Decompose per-finger motion into common (centroid drift,
            // a.k.a. pan) and differential (relative-motion, the
            // pinch+rotate signal) components, looked up by contact ID
            // so order swaps in `active` don't matter. Pan only locks
            // if the common component strictly dominates the
            // differential — otherwise the gesture is asymmetric
            // pinch/rotate where one finger contributes most of the
            // motion, and the centroid drift is a *side effect* of
            // that asymmetry, not a real pan. Without this gate, an
            // anchored-finger pinch (especially a slow one with
            // contacts far apart, where 4% distance change in mm is
            // larger than the 0.4mm pan threshold) locks pan before
            // the distance ratio crosses `PINCH_LOCK_RATIO`. The
            // strictly-greater comparison correctly rejects the
            // boundary case of a fully-anchored finger
            // (|common| = |differential|).
            let (init_a, init_b) = if a.id == base.initial_a.0 {
                (base.initial_a.1, base.initial_b.1)
            } else {
                (base.initial_b.1, base.initial_a.1)
            };
            let da = (a.x - init_a.0, a.y - init_a.1);
            let db = (b.x - init_b.0, b.y - init_b.1);
            let common = ((da.0 + db.0) * 0.5, (da.1 + db.1) * 0.5);
            let differential = ((da.0 - db.0) * 0.5, (da.1 - db.1) * 0.5);
            let common_mag = (common.0.powi(2) + common.1.powi(2)).sqrt();
            let differential_mag = (differential.0.powi(2) + differential.1.powi(2)).sqrt();
            // Pan requires both fingers to participate in roughly the
            // same translation. Two gates filter pinch/rotate signals
            // that masquerade as pan:
            //
            // A. Margin: |common| must beat |differential| by 20%, not
            //    just by epsilon. Near-perpendicular motion where the
            //    common-vs-differential test is right on the boundary
            //    isn't really "translation."
            // B. Per-finger participation, satisfied by *either*:
            //    - Balance: slower contact moves ≥ 30% of the faster.
            //      Catches symmetric pan where both fingers contribute.
            //    - Alignment: motion vectors point in nearly the same
            //      direction (cos > PAN_ALIGNMENT_COS_MIN ≈ 14°). Catches
            //      a slow scroll where one finger lags the other —
            //      common when fingers are crammed close on a small
            //      trackpad. Without this branch, the user's slow
            //      careful scrolls on the SoflePLUS2 misclassified as
            //      pinch+rotate (cf. /tmp/companion-logs ~2026-05-02:
            //      one finger moved 2.3 mm while the other moved 0.3 mm
            //      in the same direction; cos = 0.997, balance = 0.13).
            //
            // Both gates ride on top of the strict `common > differential`
            // test, which alone passes anchored-finger rotates where the
            // "anchored" finger drifts a few hundredths of a mm in the
            // same direction as the sweeper.
            let da_mag = (da.0.powi(2) + da.1.powi(2)).sqrt();
            let db_mag = (db.0.powi(2) + db.1.powi(2)).sqrt();
            let min_per_finger = da_mag.min(db_mag);
            let max_per_finger = da_mag.max(db_mag);
            let ang_delta_from_init = angle_delta(ang, base.initial_angle).abs();
            let pinch_ratio_from_init = (dist / base.initial_distance - 1.0).abs();
            // A geometric threshold alone is too eager on a short lever arm:
            // 4% of a 15 mm finger span is only 0.6 mm. Require a deliberate
            // amount of per-finger travel before bypassing the tap grace
            // window. The anchored-finger form remains responsive when one
            // finger is effectively still and the other clearly moves.
            let pinch_rot_motion_ready = max_per_finger >= TAP_MAX_MOVE_MM
                && (min_per_finger >= TAP_MAX_MOVE_MM
                    || min_per_finger <= ANCHORED_FINGER_FLOOR_MM);
            let is_active_pinch_or_rot = pinch_rot_motion_ready
                && ((base.rotate_admitted && ang_delta_from_init >= ROTATE_LOCK_RAD)
                    || (base.pinch_admitted && pinch_ratio_from_init >= PINCH_LOCK_RATIO));
            let could_still_tap = !is_active_pinch_or_rot
                && max_move < TAP_MAX_MOVE_MM
                && dur < TAP_MAX_DURATION;
            // The landing frame (and, with the default of 2, only the
            // landing frame) is observation-only: one contact is fresh
            // and the other is mid-glide, so any decomposition of their
            // motion describes the landing, not the user's intent.
            let within_grace = base.frames_observed < TWO_FINGER_MIN_FRAMES;
            if (could_still_tap || within_grace) && !is_active_pinch_or_rot {
                base.last_centroid = centroid;
                // Track scale and angle pre-lock so the first Changed
                // emit after lock is a one-frame delta, not a cumulative
                // pre-lock chunk.
                base.prev_scale = dist / base.initial_distance;
                base.prev_angle = ang;
                base.prev_transform_at = Some(now);
                self.two_baseline = Some(base);
                return;
            }
            // Cosine of the angle between the two motion vectors.
            // Undefined when either is zero — fall through to balance.
            let alignment = if da_mag > 0.0 && db_mag > 0.0 {
                (da.0 * db.0 + da.1 * db.1) / (da_mag * db_mag)
            } else {
                -1.0
            };
            let margin_ok = common_mag > differential_mag * 1.2;
            let balance = if max_per_finger > 0.0 {
                min_per_finger / max_per_finger
            } else {
                0.0
            };
            let balance_ok = balance >= 0.3;
            let aligned = alignment > PAN_ALIGNMENT_COS_MIN;
            let pan_qualified = margin_ok && (balance_ok || aligned);

            // Always-computed raw scores for the lock-decision log: a 0
            // there should mean "didn't accumulate," not "qualification
            // gate zeroed it." The selection scores below still gate on
            // qualification so suppressed signals can't win.
            let pan_raw = common_mag / PAN_LOCK_MM;
            let pinch_raw = (dist / base.initial_distance - 1.0).abs() / PINCH_LOCK_RATIO;
            let rot_raw = angle_delta(ang, base.initial_angle).abs() / ROTATE_LOCK_RAD;

            let pan = if pan_qualified { pan_raw } else { 0.0 };
            // Pinch/rotate scoring is hypersensitive to per-finger noise on
            // a long lever arm: with fingers ~20 mm apart, sub-mm jitter
            // accumulated over a few hundred ms can drift the inter-finger
            // angle past ROTATE_LOCK_RAD (4°) without the user actually
            // rotating. Two patterns produce trustworthy pinch/rot signal:
            //
            //   (a) Both fingers committed past tap-jitter
            //       (min_per_finger >= TAP_MAX_MOVE_MM). Real bimanual
            //       rotation/pinch.
            //   (b) One finger essentially anchored (sub-noise floor) and
            //       the other moving. Anchored-rotate / anchored-pinch.
            //
            // In between — one finger committed, the other drifting in the
            // ~0.3..1.0 mm noise band — the differential's direction is
            // dominated by the drifting finger's noise, which from contact
            // data alone is indistinguishable from a real anti-parallel
            // rotation. Defer the lock until either the trailer commits or
            // pan locks on coherent centroid motion. Reproduces user's
            // 2026-05-04 logs:
            //   * 485 ms quiet hold (max=0.89 mm, min=0.67 mm) → both
            //     fingers in noise band, defer.
            let u_x = (b.x - a.x) / dist;
            let u_y = (b.y - a.y) / dist;
            let v_x = -u_y;
            let v_y = u_x;

            let d_diff_x = db.0 - da.0;
            let d_diff_y = db.1 - da.1;

            // Pure relative motion along inter-finger axis (real pinch)
            let pinch_dist_mm = (d_diff_x * u_x + d_diff_y * u_y).abs();
            // Pure relative motion tangential to inter-finger axis (real rotate)
            let rot_arc_mm = (d_diff_x * v_x + d_diff_y * v_y).abs();
            let pinch_rot_admissible =
                !(ANCHORED_FINGER_FLOOR_MM..TAP_MAX_MOVE_MM).contains(&min_per_finger);
            // Penalize pinch/rot selection scores when the two finger-
            // motion vectors are roughly parallel (high positive
            // alignment cosine). Real pinch and real rotate have
            // anti-parallel or truly-anchored geometry — anti-parallel
            // gives cos <= 0 (penalty 1.0, no effect) and truly-anchored
            // gives cos = -1 by the code's fallback (penalty 1.0).
            let align_penalty = (1.0 - alignment).clamp(0.0, 1.0);

            let pinch = if base.pinch_admitted && pinch_rot_admissible {
                (pinch_dist_mm / (base.initial_distance * PINCH_LOCK_RATIO)).max(pinch_raw)
                    * align_penalty
            } else {
                0.0
            };
            let rot = if base.rotate_admitted && pinch_rot_admissible {
                (rot_arc_mm / (dist * ROTATE_LOCK_RAD)).max(rot_raw) * align_penalty
            } else {
                0.0
            };
            // Pan only ever scores when it qualified. A lenient override
            // here (accepting `common >= differential * 0.6`) inverts the
            // invariant this whole decomposition exists to enforce: an
            // anchored-finger pinch drifts the centroid as a *side
            // effect*, and letting that drift win produces exactly the
            // "lock scroll, then immediately switch to pinch" churn the
            // dynamic switch was then added to paper over.
            let pan_score = pan;

            if pan_score >= 1.0 || pinch >= 1.0 || rot >= 1.0 {
                // Pan is mutually exclusive with pinch/rotate (matches
                // macOS PTP behavior: a 2F gesture locks into either
                // pan/scroll or the pinch+rotate pair). It only wins
                // if it's the strongest signal — a gesture that's
                // mostly pinch but drifts the centroid a bit shouldn't
                // be misclassified as pan.
                let new_kind = if pan_score >= 1.0 && pan_score >= pinch && pan_score >= rot {
                    GestureKind::TwoFingerPan
                } else {
                    let pinch_or_rot = pinch.max(rot);
                    let pan_lenient = if common_mag > differential_mag * 1.1 {
                        common_mag / PAN_LOCK_MM
                    } else {
                        0.0
                    };
                    let aligned_motion = alignment > PAN_ALIGNMENT_COS_MIN;
                    if (pan_lenient > pinch_or_rot || aligned_motion)
                        && !base.pinch_rot_lock_pending
                    {
                        log::debug!(
                            "pinch+rotate lock deferred: pan_lenient={:.2} alignment={:.3}",
                            pan_lenient,
                            alignment,
                        );
                        base.pinch_rot_lock_pending = true;
                        self.two_baseline = Some(base);
                        return;
                    }
                    GestureKind::TwoFingerPinchAndRotate
                };
                self.kind = new_kind;
                self.pending_right_click = None;
                let pan_tag = if pan_qualified {
                    String::new()
                } else if !margin_ok {
                    " disq:margin".to_string()
                } else {
                    " disq:participation".to_string()
                };
                let pinch_tag = if !pinch_rot_admissible {
                    " gated:noise"
                } else if !base.pinch_admitted {
                    " gated:policy"
                } else {
                    ""
                };
                let rot_tag = if !pinch_rot_admissible {
                    " gated:noise"
                } else if !base.rotate_admitted {
                    " gated:policy"
                } else {
                    ""
                };
                match new_kind {
                    GestureKind::TwoFingerPan => {
                        log::info!(
                            "2F lock=scroll scores[pan={:.2}{} pinch={:.2}{} rot={:.2}{}] common={:.2}mm diff={:.2}mm align={:.2} balance={:.2}",
                            pan_raw,
                            pan_tag,
                            pinch_raw,
                            pinch_tag,
                            rot_raw,
                            rot_tag,
                            common_mag,
                            differential_mag,
                            alignment,
                            balance,
                        );
                        self.emit_scroll(0.0, 0.0, Phase::Began);
                        // Claim the Began here. The pan dispatch below
                        // also opens a stream when `last_scroll_time` is
                        // still unset (that path serves the partial-lift
                        // rejoin); without this the lock frame posts
                        // Began twice and downstream sees two
                        // overlapping scroll streams.
                        base.last_scroll_time = Some(now);
                    }
                    GestureKind::TwoFingerPinchAndRotate => {
                        log::info!(
                            "2F lock=pinch+rotate scores[pinch={:.2}{} rot={:.2}{} pan={:.2}{}] common={:.2}mm diff={:.2}mm align={:.2} balance={:.2}",
                            pinch_raw,
                            pinch_tag,
                            rot_raw,
                            rot_tag,
                            pan_raw,
                            pan_tag,
                            common_mag,
                            differential_mag,
                            alignment,
                            balance,
                        );
                        // Begin only streams admitted for the app under the
                        // cursor. Sending an unadmitted Began/Ended pair
                        // still creates an observable gesture in AppKit,
                        // even when all Changed events are filtered.
                        if base.pinch_admitted {
                            self.out.pinch(0.0, Phase::Began);
                        }
                        if base.rotate_admitted {
                            self.out.rotate(0.0, Phase::Began);
                        }
                        // Re-anchor baseline for streams that actually crossed lock threshold so initial
                        // travel is delivered without noise cross-pollution.
                        if rot >= 1.0 {
                            base.prev_angle = base.initial_angle;
                        }
                        if pinch >= 1.0 {
                            base.prev_scale = 1.0;
                        }
                        base.prev_transform_at = Some(now);
                    }
                    _ => {}
                }
            }
        }

        match self.kind {
            GestureKind::TwoFingerPan => {
                let scale = dist / base.initial_distance;
                let ddx = centroid.0 - base.last_centroid.0;
                let ddy = centroid.1 - base.last_centroid.1;

                if base.last_scroll_time.is_none() {
                    self.emit_scroll(0.0, 0.0, Phase::Began);
                }
                if ddx.abs() > MOTION_DEAD_ZONE_MM || ddy.abs() > MOTION_DEAD_ZONE_MM {
                    if let Some(prev) = base.last_scroll_time {
                        let dt = (now - prev).as_secs_f64().max(0.001);
                        let inst_vx = ddx / dt;
                        let inst_vy = ddy / dt;
                        base.scroll_velocity.0 = SCROLL_VELOCITY_ALPHA * inst_vx
                            + (1.0 - SCROLL_VELOCITY_ALPHA) * base.scroll_velocity.0;
                        base.scroll_velocity.1 = SCROLL_VELOCITY_ALPHA * inst_vy
                            + (1.0 - SCROLL_VELOCITY_ALPHA) * base.scroll_velocity.1;
                        // Track the peak of the smoothed velocity for the
                        // inertia seed. A peak older than the window is
                        // stale — the user has since slowed down on
                        // purpose — so it decays rather than persisting
                        // for the whole gesture.
                        let cur_speed = (base.scroll_velocity.0.powi(2)
                            + base.scroll_velocity.1.powi(2))
                        .sqrt();
                        let peak_speed =
                            (base.peak_velocity.0.powi(2) + base.peak_velocity.1.powi(2)).sqrt();
                        let peak_stale = base
                            .peak_velocity_at
                            .map(|t| now.saturating_duration_since(t) > INERTIA_PEAK_WINDOW)
                            .unwrap_or(true);
                        if cur_speed >= peak_speed || peak_stale {
                            base.peak_velocity = base.scroll_velocity;
                            base.peak_velocity_at = Some(now);
                        }
                    }
                    base.last_scroll_time = Some(now);
                    log::debug!(
                        "scroll: d=({:+.3},{:+.3})mm v=({:+.0},{:+.0})mm/s",
                        ddx,
                        ddy,
                        base.scroll_velocity.0,
                        base.scroll_velocity.1,
                    );
                    self.emit_scroll(ddx, ddy, Phase::Changed);
                    base.cumulative_dx += ddx;
                    if self.options.right_edge_swipe
                        && base.right_edge_candidate
                        && !base.right_edge_latched
                        && base.cumulative_dx <= -3.8
                    {
                        log::info!(
                            "2f right-edge swipe in: toggle notification center (cumulative_dx={:.2}mm)",
                            base.cumulative_dx
                        );
                        self.out.haptic(HapticKind::GestureCommitted);
                        self.out.toggle_notification_center();
                        base.right_edge_latched = true;
                    }
                    base.last_centroid = centroid;
                }
                // Dynamic transition to PinchAndRotate if user distinctly pinches or rotates mid-scroll
                let scale_rel = (dist / base.initial_distance - 1.0).abs();
                let frame_rot = angle_delta(ang, base.prev_angle).abs();
                let total_rot = angle_delta(ang, base.initial_angle).abs();
                // A lagging scroll finger can change the pair's angle or
                // distance without being an intentional pinch/rotate. Only
                // switch away from an established pan when the two contacts'
                // accumulated motion is not strongly co-directed. This keeps
                // same-direction trailing-finger motion in the scroll stream,
                // while anti-parallel pinch/rotate motion remains eligible.
                let (init_a, init_b) = if a.id == base.initial_a.0 {
                    (base.initial_a.1, base.initial_b.1)
                } else {
                    (base.initial_b.1, base.initial_a.1)
                };
                let da = (a.x - init_a.0, a.y - init_a.1);
                let db = (b.x - init_b.0, b.y - init_b.1);
                let da_mag = (da.0.powi(2) + da.1.powi(2)).sqrt();
                let db_mag = (db.0.powi(2) + db.1.powi(2)).sqrt();
                let relative_alignment = if da_mag > 0.0 && db_mag > 0.0 {
                    (da.0 * db.0 + da.1 * db.1) / (da_mag * db_mag)
                } else {
                    -1.0
                };
                let min_relative_motion = da_mag.min(db_mag);
                let max_relative_motion = da_mag.max(db_mag);
                let relative_motion_is_gesture = relative_alignment <= 0.3
                    || (min_relative_motion <= ANCHORED_FINGER_FLOOR_MM
                        && max_relative_motion >= TAP_MAX_MOVE_MM
                        && relative_alignment < 0.0);
                if (base.pinch_admitted && scale_rel >= 0.25 && relative_motion_is_gesture)
                    || (base.rotate_admitted
                        && (frame_rot >= 2.0_f64.to_radians()
                            || total_rot >= 15.0_f64.to_radians())
                        && relative_motion_is_gesture)
                {
                    log::info!(
                        "2F dynamic transition from Pan -> PinchAndRotate (scale_rel={:.2} frame_rot={:.2}rad align={:.2})",
                        scale_rel,
                        frame_rot,
                        relative_alignment,
                    );
                    self.emit_scroll(0.0, 0.0, Phase::Ended);
                    self.kind = GestureKind::TwoFingerPinchAndRotate;
                    if base.pinch_admitted {
                        self.out.pinch(0.0, Phase::Began);
                    }
                    if base.rotate_admitted {
                        self.out.rotate(0.0, Phase::Began);
                    }
                    base.prev_scale = scale;
                    base.prev_angle = ang;
                    base.prev_transform_at = Some(now);
                    self.two_baseline = Some(base);
                    return;
                }
                base.prev_scale = scale;
                base.prev_angle = ang;
            }
            GestureKind::TwoFingerPinchAndRotate => {
                let prev_dist = if base.prev_scale > 1e-4 {
                    base.prev_scale * base.initial_distance
                } else {
                    base.initial_distance
                };
                let scale = dist / base.initial_distance;
                // Mathematical differential magnification delta: (dist_t - dist_{t-1}) / dist_{t-1}
                let raw_scale_delta = if prev_dist > 1e-4 {
                    (dist - prev_dist) / prev_dist
                } else {
                    0.0
                };
                let transform_dt = base
                    .prev_transform_at
                    .map(|t| now.saturating_duration_since(t))
                    .unwrap_or_else(|| Duration::from_millis(16));
                let scale_delta = limit_transform_delta(
                    raw_scale_delta,
                    transform_dt,
                    PINCH_MAX_RATE,
                    PINCH_MAX_FRAME_DELTA,
                );
                let raw_angle_d = angle_delta(ang, base.prev_angle);
                // Suppress anomalous angle flips caused by contact swap or collinear crossing (>30 deg in a single frame).
                let angle_d = if raw_angle_d.abs() > 30.0_f64.to_radians() {
                    0.0
                } else {
                    limit_transform_delta(
                        raw_angle_d,
                        transform_dt,
                        ROTATE_MAX_RATE_RAD,
                        ROTATE_MAX_FRAME_RAD,
                    )
                };

                // Both admitted streams remain alive for the whole locked
                // transform session. This keeps each AppKit consumer's
                // Began/Changed/Ended lifecycle coherent, while the small
                // deadzones below suppress sensor noise.
                if scale_delta.abs() >= TRANSFORM_MIN_DELTA && base.pinch_admitted {
                    log::debug!("pinch: delta={:+.4} scale={:.4}", scale_delta, scale);
                    self.out.pinch(scale_delta, Phase::Changed);
                }

                if angle_d.abs() >= ROTATE_EMIT_DEADZONE_RAD && base.rotate_admitted {
                    // AppKit defines rotation as a signed relative angle;
                    // keep the geometric 1:1 value until a real-device A/B
                    // test proves an acceleration curve is desirable.
                    let appkit_angle_deg = -angle_d.to_degrees();
                    log::debug!("rotate: delta={:+.2}deg gain=1.00x", appkit_angle_deg);
                    self.out.rotate(appkit_angle_deg, Phase::Changed);
                }
                base.prev_scale = scale;
                base.prev_angle = ang;
                base.prev_transform_at = Some(now);
            }
            _ => {}
        }

        // Pan advances `last_centroid` on emit (above); other kinds don't
        // read it but stay in sync. The PinchAndRotate dispatch already
        // refreshes `prev_scale` / `prev_angle` every frame.
        if !matches!(self.kind, GestureKind::TwoFingerPan) {
            base.last_centroid = centroid;
        }
        self.two_baseline = Some(base);
    }

    fn dispatch_swipe(&mut self, active: &[Contact], now: Timestamp) {
        let Some(mut base) = self.multi_baseline else {
            return;
        };
        if active.is_empty() {
            return;
        }
        let cx: f64 = active.iter().map(|c| c.x).sum::<f64>() / active.len() as f64;
        let cy: f64 = active.iter().map(|c| c.y).sum::<f64>() / active.len() as f64;

        // When the finger count changes (e.g. 4 -> 3 or 3 -> 4) during a multi-finger swipe,
        // the raw centroid jumps by several millimetres because the cluster geometry changes,
        // NOT because the user's hand moved. Re-anchor last_centroid without adding the jump
        // to cumulative displacement.
        if base.finger_count != active.len() {
            log::debug!(
                "swipe: finger count changed ({} -> {}) — re-anchoring centroid to avoid jitter",
                base.finger_count,
                active.len()
            );
            base.finger_count = active.len();
            base.last_centroid = (cx, cy);
            base.last_centroid_time = Some(now);
            self.multi_baseline = Some(base);
            return;
        }

        let delta_x = cx - base.last_centroid.0;
        let delta_y = cy - base.last_centroid.1;
        base.cumulative_dx += delta_x;
        base.cumulative_dy += delta_y;

        let dx = base.cumulative_dx;
        let dy = base.cumulative_dy;

        // 4-Finger Radial Pinch/Spread Detection (Launchpad & Show Desktop):
        if base.finger_count >= 4
            && !base.radial_action_latched
            && base.axis.is_none()
            && base.initial_radial_spread > 1.0
        {
            let current_radial = compute_radial_spread(active, (cx, cy));
            let ratio = current_radial / base.initial_radial_spread;
            let travel = (dx * dx + dy * dy).sqrt();

            // Pinch in: fingers move toward centroid -> Launchpad (启动台)
            if ratio <= 0.72 && travel < 4.5 {
                log::info!("4f radial pinch-in: toggle Launchpad (ratio={ratio:.2})");
                self.out.haptic(HapticKind::GestureCommitted);
                self.out.toggle_launchpad();
                base.radial_action_latched = true;
                base.last_centroid = (cx, cy);
                base.last_centroid_time = Some(now);
                self.multi_baseline = Some(base);
                return;
            }

            // Spread out: fingers move away from centroid -> Show Desktop (显示桌面)
            if ratio >= 1.28 && travel < 4.5 {
                log::info!("4f radial spread-out: toggle Show Desktop (ratio={ratio:.2})");
                self.out.haptic(HapticKind::GestureCommitted);
                self.out.toggle_show_desktop();
                base.radial_action_latched = true;
                base.last_centroid = (cx, cy);
                base.last_centroid_time = Some(now);
                self.multi_baseline = Some(base);
                return;
            }
        }

        if base.radial_action_latched {
            base.last_centroid = (cx, cy);
            base.last_centroid_time = Some(now);
            self.multi_baseline = Some(base);
            return;
        }

        // Lock the swipe axis on first significant centroid motion.
        // Holding the axis for the rest of the gesture means a slight
        // wander near the diagonal can't flip the swipe sideways
        // mid-flight (which would bracket the in-flight stream with a
        // foreign-axis Began the Dock interprets as cancellation).
        if base.axis.is_none() {
            if dx.abs() < SWIPE_AXIS_LOCK_MM && dy.abs() < SWIPE_AXIS_LOCK_MM {
                base.last_centroid = (cx, cy);
                self.multi_baseline = Some(base);
                return;
            }
            let candidate = if dx.abs() >= dy.abs() {
                SwipeAxis::Horizontal
            } else {
                SwipeAxis::Vertical
            };
            let admitted = match candidate {
                SwipeAxis::Horizontal => base.swipe_horizontal_admitted,
                SwipeAxis::Vertical => base.swipe_vertical_admitted,
            };
            if !admitted {
                // Dominant axis isn't admitted under the cursor — refuse
                // to lock rather than firing a swipe the policy would
                // suppress. Fingers may pivot to the other axis later;
                // we keep re-evaluating each frame until a finger lifts.
                base.last_centroid = (cx, cy);
                self.multi_baseline = Some(base);
                return;
            }
            base.axis = Some(candidate);
        }
        let axis = base.axis.expect("axis just locked");

        // Update EMA velocity on the locked axis.
        if let Some(prev_t) = base.last_centroid_time {
            let dt = (now - prev_t).as_secs_f64().max(1e-3);
            let inst_vx = delta_x / dt;
            let inst_vy = delta_y / dt;
            base.velocity.0 =
                SCROLL_VELOCITY_ALPHA * inst_vx + (1.0 - SCROLL_VELOCITY_ALPHA) * base.velocity.0;
            base.velocity.1 =
                SCROLL_VELOCITY_ALPHA * inst_vy + (1.0 - SCROLL_VELOCITY_ALPHA) * base.velocity.1;
        }
        base.last_centroid = (cx, cy);
        base.last_centroid_time = Some(now);

        let signed_progress = match axis {
            SwipeAxis::Horizontal => dx / SWIPE_PROGRESS_REF_MM,
            SwipeAxis::Vertical => dy / SWIPE_PROGRESS_REF_MM,
        };
        let phase = if base.began_posted {
            Phase::Changed
        } else {
            base.began_posted = true;
            log::debug!(
                "swipe: began axis={:?} progress={:+.3} (n_fingers={})",
                axis,
                signed_progress,
                active.len(),
            );
            Phase::Began
        };
        if matches!(phase, Phase::Began) {
            self.out.haptic(HapticKind::GestureCommitted);
        }
        self.out
            .swipe(axis, signed_progress, /* velocity */ 0.0, phase);
        self.multi_baseline = Some(base);
    }
}

impl<O: Output> Drop for State<O> {
    fn drop(&mut self) {
        // A transport can disappear while the three-finger drag is still
        // engaged. Release the synthesized button before dropping the
        // output so macOS cannot retain a stuck left-button state.
        if self.drag_button_held {
            self.out.set_event_time(Timestamp::now());
            self.out.set_drag_button_held(false);
            self.drag_button_held = false;
        } else if self.prev_button {
            self.out.set_event_time(Timestamp::now());
            self.out.set_left_button_held(false);
            self.prev_button = false;
        }
    }
}

/// Smallest signed difference between two angles, in (-π, π].
fn angle_delta(a: f64, b: f64) -> f64 {
    let mut d = a - b;
    while d > std::f64::consts::PI {
        d -= 2.0 * std::f64::consts::PI;
    }
    while d <= -std::f64::consts::PI {
        d += 2.0 * std::f64::consts::PI;
    }
    d
}

/// Clamp a relative transform delta using both elapsed time and a hard
/// per-frame ceiling. A paused sender or dropped frame may otherwise turn a
/// single report into a visible zoom/rotation jump. The clamp is symmetric,
/// finite-input only, and preserves the direction of valid motion.
fn limit_transform_delta(raw: f64, elapsed: Duration, max_rate: f64, max_frame: f64) -> f64 {
    if !raw.is_finite() || !max_rate.is_finite() || !max_frame.is_finite() {
        return 0.0;
    }
    let dt = elapsed.as_secs_f64().clamp(0.008, 0.100);
    let limit = (max_rate * dt).abs().min(max_frame.abs());
    if limit <= 0.0 || !limit.is_finite() {
        return 0.0;
    }
    raw.clamp(-limit, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn transform_delta_filter_is_time_and_frame_bounded() {
        for millis in [8, 16, 100] {
            let dt = Duration::from_millis(millis);
            let expected = (PINCH_MAX_RATE * (millis as f64 / 1000.0))
                .min(PINCH_MAX_FRAME_DELTA);
            assert_eq!(
                limit_transform_delta(1.0, dt, PINCH_MAX_RATE, PINCH_MAX_FRAME_DELTA),
                expected,
                "pinch limit at {millis}ms"
            );
        }
        assert_eq!(
            limit_transform_delta(-1.0, Duration::from_millis(16), PINCH_MAX_RATE, PINCH_MAX_FRAME_DELTA),
            -0.048
        );
        assert_eq!(
            limit_transform_delta(1.0, Duration::from_millis(16), ROTATE_MAX_RATE_RAD, ROTATE_MAX_FRAME_RAD),
            0.192
        );
        assert!((limit_transform_delta(0.01, Duration::from_millis(16), PINCH_MAX_RATE, PINCH_MAX_FRAME_DELTA) - 0.01).abs() < 1e-9);
        assert_eq!(limit_transform_delta(f64::NAN, Duration::from_millis(16), PINCH_MAX_RATE, PINCH_MAX_FRAME_DELTA), 0.0);
    }

    struct Recorder {
        log: RefCell<Vec<String>>,
        /// What `cancel_inertia` should report. Toggle on with
        /// `set_inertia_active` to simulate "user touches the pad while
        /// a fling is coasting"; the next `cancel_inertia` returns true
        /// (just like the real Emitter would when its CFRunLoopTimer is
        /// live) and clears the flag.
        inertia_active: std::cell::Cell<bool>,
        /// Per-mode admission overrides. Default-true so existing tests
        /// (predating the under-cursor policy) keep passing without
        /// modification; the dedicated admit tests flip these off.
        pinch_admit: std::cell::Cell<bool>,
        rotate_admit: std::cell::Cell<bool>,
        swipe_horizontal_admit: std::cell::Cell<bool>,
        swipe_vertical_admit: std::cell::Cell<bool>,
    }

    impl Default for Recorder {
        fn default() -> Self {
            Self {
                log: RefCell::new(Vec::new()),
                inertia_active: std::cell::Cell::new(false),
                pinch_admit: std::cell::Cell::new(true),
                rotate_admit: std::cell::Cell::new(true),
                swipe_horizontal_admit: std::cell::Cell::new(true),
                swipe_vertical_admit: std::cell::Cell::new(true),
            }
        }
    }

    impl Recorder {
        fn pop(&self) -> Vec<String> {
            self.log.borrow_mut().drain(..).collect()
        }
        fn set_inertia_active(&self, active: bool) {
            self.inertia_active.set(active);
        }
        fn deny_pinch(&self) {
            self.pinch_admit.set(false);
        }
        fn deny_rotate(&self) {
            self.rotate_admit.set(false);
        }
    }

    impl Output for &Recorder {
        fn haptic(&self, kind: HapticKind) {
            self.log.borrow_mut().push(format!("haptic {kind:?}"));
        }
        fn move_cursor_by(&self, dx_px: i32, dy_px: i32) {
            self.log.borrow_mut().push(format!("move {dx_px} {dy_px}"));
        }
        fn click(&self, button: MouseButton) {
            self.log.borrow_mut().push(format!("click {button:?}"));
        }
        fn set_left_button_held(&self, held: bool) {
            self.log
                .borrow_mut()
                .push(format!("set_left_button_held {held}"));
        }
        fn scroll(&self, dx: f64, dy: f64, phase: Phase) {
            self.log
                .borrow_mut()
                .push(format!("scroll {dx:.4} {dy:.4} {phase:?}"));
        }
        fn scroll_inertia(&self, vx: f64, vy: f64) {
            self.log
                .borrow_mut()
                .push(format!("scroll_inertia {vx:.4} {vy:.4}"));
        }
        fn cancel_inertia(&self) -> bool {
            let was_active = self.inertia_active.replace(false);
            self.log.borrow_mut().push(format!(
                "cancel_inertia{}",
                if was_active { " (was_active)" } else { "" }
            ));
            was_active
        }
        fn pinch(&self, delta: f64, phase: Phase) {
            self.log
                .borrow_mut()
                .push(format!("pinch {delta:.4} {phase:?}"));
        }
        fn rotate(&self, delta: f64, phase: Phase) {
            self.log
                .borrow_mut()
                .push(format!("rotate {delta:.4} {phase:?}"));
        }
        fn swipe(&self, axis: SwipeAxis, signed_progress: f64, velocity: f64, phase: Phase) {
            self.log.borrow_mut().push(format!(
                "swipe {axis:?} {signed_progress:+.3} v={velocity:+.1} {phase:?}"
            ));
        }
        fn pinch_admissible_now(&self) -> bool {
            self.pinch_admit.get()
        }
        fn rotate_admissible_now(&self) -> bool {
            self.rotate_admit.get()
        }
        fn swipe_admissible_now(&self, axis: SwipeAxis) -> bool {
            match axis {
                SwipeAxis::Horizontal => self.swipe_horizontal_admit.get(),
                SwipeAxis::Vertical => self.swipe_vertical_admit.get(),
            }
        }
        fn look_up_dictionary(&self) {
            self.log.borrow_mut().push("look_up_dictionary".to_string());
        }
        fn smart_magnify(&self) {
            self.log.borrow_mut().push("smart_magnify".to_string());
        }
        fn toggle_notification_center(&self) {
            self.log
                .borrow_mut()
                .push("toggle_notification_center".to_string());
        }
        fn toggle_launchpad(&self) {
            self.log.borrow_mut().push("toggle_launchpad".to_string());
        }
        fn toggle_show_desktop(&self) {
            self.log.borrow_mut().push("toggle_show_desktop".to_string());
        }
        fn toggle_app_expose(&self) {
            self.log.borrow_mut().push("toggle_app_expose".to_string());
        }
        fn toggle_mission_control(&self) {
            self.log
                .borrow_mut()
                .push("toggle_mission_control".to_string());
        }
    }

    /// Tests pre-date the chip-px → mm migration: their coordinates are
    /// expressed as [0,1] fractions of a notional pad. The helper scales
    /// them onto a square 50 × 50 mm "test pad" so the engine sees the
    /// physical units it now expects. 50 mm is roughly the X dimension
    /// of the SoflePLUS2 (49 mm) and gives sensible mm budgets for the
    /// `0.001`-level deltas in tests like `pre_scroll_two_finger_settling`
    /// (~0.05 mm) and `lift_suppresses_prior_frame_centroid_shift_jump`
    /// (~0.25 mm normal motion vs. 2.5 mm lift jump).
    const TEST_PAD_MM: f64 = 50.0;

    /// Cursor-accel config used by every test that doesn't specifically
    /// exercise the curve. `exponent == 1.0` makes the gain plain
    /// linear so `mm * px_per_mm_at_ref` is the pixel delta — and at
    /// 1.0 px/mm the recorded `move dx dy` numbers match the input
    /// mm 1:1 (modulo the integer truncation + carry), keeping the
    /// existing assertions readable.
    fn test_accel() -> CursorAccel {
        CursorAccel {
            px_per_mm_at_ref: 1.0,
            exponent: 1.0,
            ref_mm_per_sec: 80.0,
        }
    }

    fn frame(contacts: &[(u8, f64, f64)]) -> Frame {
        Frame {
            contacts: contacts
                .iter()
                .map(|&(id, nx, ny)| Contact {
                    id,
                    x: nx * TEST_PAD_MM,
                    y: ny * TEST_PAD_MM,
                    tip: true,
                    confidence: true,
                })
                .collect(),
            scan_time_100us: 0,
            button: false,
        }
    }

    fn frame_with_button(contacts: &[(u8, f64, f64)], button: bool) -> Frame {
        let mut f = frame(contacts);
        f.button = button;
        f
    }

    #[test]
    fn button_press_then_release_forwards_held_edges() {
        // Hardware-button drag: the firmware sets `Frame::button` while
        // the user holds a key bound to MouseBtn1. The companion must
        // surface those transitions verbatim — once on press, once on
        // release — and nothing in between, regardless of how many
        // identical-button frames stream through.
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame_with_button(&[(1, 0.5, 0.5)], true));
        s.on_frame(frame_with_button(&[(1, 0.6, 0.5)], true));
        s.on_frame(frame_with_button(&[(1, 0.7, 0.5)], false));
        let log = r.pop();
        let edges: Vec<_> = log
            .iter()
            .filter(|l| l.starts_with("set_left_button_held"))
            .collect();
        assert_eq!(
            edges,
            vec![
                &"set_left_button_held true".to_string(),
                &"set_left_button_held false".to_string(),
            ],
            "{log:?}"
        );
    }

    #[test]
    fn button_held_without_finger_still_forwards_edges() {
        // Firmware emits a button-only PTP report (contact_count=0,
        // button=1) when the user presses MouseBtn1 without any finger
        // on the pad. Companion must forward the edge — apps need the
        // mouse-down before any drag motion arrives.
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame_with_button(&[], true));
        s.on_frame(frame_with_button(&[], false));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l == "set_left_button_held true"),
            "{log:?}"
        );
        assert!(
            log.iter().any(|l| l == "set_left_button_held false"),
            "{log:?}"
        );
    }

    #[test]
    fn one_finger_tap_emits_left_click() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame(&[(1, 0.5, 0.5)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(log.iter().any(|l| l.contains("click Left")), "{log:?}");
    }

    #[test]
    fn two_finger_tap_emits_right_click() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]), t0);
        s.on_frame_at(frame(&[]), at(t0, 50));
        s.tick(at(t0, 300));
        let log = r.pop();
        assert!(log.iter().any(|l| l.contains("click Right")), "{log:?}");
    }

    #[test]
    fn one_finger_drag_emits_cursor() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // Cursor motion is deferred by one frame, so a 3-frame sequence
        // is needed for the second frame's motion to surface (the third
        // frame, with a finger still down, drains the buffer). A 2-frame
        // sequence would leave the motion in `pending_motion` and the
        // implicit lift on the next call would drop it.
        s.on_frame(frame(&[(1, 0.5, 0.5)]));
        s.on_frame(frame(&[(1, 0.6, 0.5)]));
        s.on_frame(frame(&[(1, 0.7, 0.5)]));
        let log = r.pop();
        assert!(log.iter().any(|l| l.starts_with("move ")), "{log:?}");
    }

    #[test]
    fn two_finger_pan_emits_scroll() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]));
        s.on_frame(frame(&[(1, 0.4, 0.55), (2, 0.6, 0.55)]));
        s.on_frame(frame(&[(1, 0.4, 0.6), (2, 0.6, 0.6)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "{log:?}"
        );
        assert!(log.iter().any(|l| l.contains("Ended")), "{log:?}");
    }

    /// When pinch and rotate are both denied for the under-cursor app,
    /// a 2F spread (which would normally lock pinch) must NOT fire any
    /// pinch or rotate events. The user's intent in such an app is
    /// scroll; with no centroid translation here, the gesture stays
    /// unclassified — better than firing an unintended pinch.
    #[test]
    fn two_finger_spread_with_pinch_denied_emits_nothing() {
        let r = Recorder::default();
        r.deny_pinch();
        r.deny_rotate();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame(&[(1, 0.45, 0.5), (2, 0.55, 0.5)]));
        s.on_frame(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]));
        s.on_frame(frame(&[(1, 0.3, 0.5), (2, 0.7, 0.5)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") || l.starts_with("rotate")),
            "denied policy must suppress pinch/rotate: {log:?}"
        );
    }

    /// When pinch and rotate are denied but the user does a real 2F
    /// scroll (parallel motion, no spread/twist), pan still locks
    /// normally and scroll fires. This is the path that lets the user
    /// "scroll in iTerm2" without 2F gestures spuriously locking
    /// pinch+rotate.
    #[test]
    fn two_finger_pan_with_pinch_denied_still_scrolls() {
        let r = Recorder::default();
        r.deny_pinch();
        r.deny_rotate();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]));
        s.on_frame(frame(&[(1, 0.4, 0.55), (2, 0.6, 0.55)]));
        s.on_frame(frame(&[(1, 0.4, 0.6), (2, 0.6, 0.6)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "denied pinch/rotate must not block scroll: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") || l.starts_with("rotate")),
            "{log:?}"
        );
    }

    #[test]
    fn two_finger_spread_emits_pinch() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame(&[(1, 0.45, 0.5), (2, 0.55, 0.5)]));
        s.on_frame(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]));
        s.on_frame(frame(&[(1, 0.3, 0.5), (2, 0.7, 0.5)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "{log:?}"
        );
    }

    #[test]
    fn small_two_finger_spread_stays_unclassified_until_intent_is_clear() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // 15 mm starting span; a 0.6 mm relative spread is already 4% and
        // used to open a pinch stream on the second frame. This is within
        // finger settling noise and must remain tap/ambiguous instead.
        s.on_frame_at(frame(&[(1, 0.40, 0.60), (2, 0.70, 0.60)]), t0);
        s.on_frame_at(
            frame(&[(1, 0.394, 0.60), (2, 0.706, 0.60)]),
            at(t0, 16),
        );
        s.on_frame_at(frame(&[]), at(t0, 80));
        s.tick(at(t0, 400));
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("pinch") || l.starts_with("rotate")),
            "sub-mm spread must not lock a transform stream: {log:?}"
        );
        assert!(
            log.iter().any(|l| l.contains("click Right")),
            "ambiguous short touch should retain secondary-click behavior: {log:?}"
        );
    }

    /// Anchored-finger pinch: one finger stays put while the other
    /// moves toward it. Centroid drifts at half the moving finger's
    /// rate, so a naive `pan > pinch` comparison would lock pan first
    /// even when the user clearly intended a pinch (this was the
    /// SoflePLUS2 hardware test failure that motivated the
    /// common-vs-differential pan gate).
    #[test]
    fn asymmetric_pinch_locks_pinch_not_pan() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // 50mm test pad. Contact 1 at (10,30), Contact 2 at (40,20) —
        // distance ≈ 31.6 mm. Contact 1 stays anchored; Contact 2
        // moves diagonally toward it. Before the fix, the centroid
        // drift crossed PAN_LOCK_MM before the distance ratio crossed
        // PINCH_LOCK_RATIO and pan locked. After the fix, an anchored
        // finger gives `|common| = |differential|` exactly, so pan
        // is disqualified and pinch wins.
        s.on_frame(frame(&[(1, 0.2, 0.6), (2, 0.8, 0.4)]));
        s.on_frame(frame(&[(1, 0.2, 0.6), (2, 0.76, 0.42)]));
        s.on_frame(frame(&[(1, 0.2, 0.6), (2, 0.72, 0.44)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "expected pinch lock, got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "must not lock pan: {log:?}"
        );
    }

    /// Asymmetric pinch where *both* fingers move (so a per-finger
    /// `min_disp ≥ PAN_LOCK_MM` gate is not enough) but the
    /// differential motion still dominates the centroid translation.
    /// Reproduces the SoflePLUS2 hardware case from
    /// /tmp/companion-logs.txt run 2: contacts at (3.11,41.32) and
    /// (48.19,15.55) move to (3.78,40.12) and (47.83,15.89) by lock —
    /// per-finger displacements of 1.37mm and 0.50mm (both > 0.4mm),
    /// centroid drift 0.46mm vs. differential motion 0.92mm. The
    /// `common > differential` gate disqualifies pan; pinch wins on
    /// the next few frames as the distance ratio crosses threshold.
    #[test]
    fn asymmetric_pinch_with_minor_motion_on_anchor_finger_locks_pinch() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // Coordinates chosen to be in mm directly via the 50mm test
        // pad helper. Two contacts ~52mm apart along the diagonal.
        s.on_frame(frame(&[(1, 0.062, 0.826), (2, 0.964, 0.311)]));
        s.on_frame(frame(&[(1, 0.066, 0.812), (2, 0.961, 0.314)]));
        s.on_frame(frame(&[(1, 0.071, 0.798), (2, 0.957, 0.318)]));
        s.on_frame(frame(&[(1, 0.076, 0.802), (2, 0.939, 0.326)]));
        s.on_frame(frame(&[(1, 0.084, 0.798), (2, 0.924, 0.331)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "expected pinch lock, got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "must not lock pan: {log:?}"
        );
    }

    #[test]
    fn two_finger_rotate_emits_rotate() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]));
        // Rotate ~30° around centroid.
        s.on_frame(frame(&[(1, 0.413, 0.45), (2, 0.587, 0.55)]));
        s.on_frame(frame(&[(1, 0.45, 0.413), (2, 0.55, 0.587)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "{log:?}"
        );
    }

    #[test]
    fn denied_pinch_does_not_open_or_close_a_pinch_stream() {
        let r = Recorder::default();
        r.deny_pinch();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame(&[(1, 0.45, 0.5), (2, 0.55, 0.5)]));
        s.on_frame(frame(&[(1, 0.40, 0.5), (2, 0.60, 0.5)]));
        s.on_frame(frame(&[(1, 0.30, 0.5), (2, 0.70, 0.5)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("pinch")),
            "policy-denied pinch must not produce an orphaned stream: {log:?}"
        );
    }

    #[test]
    fn denied_rotate_does_not_suppress_admitted_pinch() {
        let r = Recorder::default();
        r.deny_rotate();
        let mut s = State::new(&r, test_accel());
        s.on_frame(frame(&[(1, 0.45, 0.5), (2, 0.55, 0.5)]));
        s.on_frame(frame(&[(1, 0.40, 0.5), (2, 0.60, 0.5)]));
        s.on_frame(frame(&[(1, 0.30, 0.5), (2, 0.70, 0.5)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Changed")),
            "policy-denied rotate must not steal pinch dominance: {log:?}"
        );
        assert!(
            !log.iter().any(|l| l.starts_with("rotate")),
            "policy-denied rotate must not produce an event stream: {log:?}"
        );
    }

    /// A clean pinch with sub-deadzone rotational noise should not create
    /// spurious rotate Changed events. The transform deadzone keeps the
    /// second stream quiet while the admitted pinch stream remains active.
    #[test]
    fn pinch_dominant_stream_stays_sticky_under_rotational_noise() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // c0 and c1 anti-parallel along x; c1 drifts +y by 0.04mm per
        // frame so a tiny angle change accumulates without dominating.
        s.on_frame(frame(&[(1, 0.20, 0.50), (2, 0.80, 0.50)]));
        s.on_frame(frame(&[(1, 0.205, 0.5004), (2, 0.795, 0.5008)]));
        s.on_frame(frame(&[(1, 0.215, 0.5008), (2, 0.785, 0.5016)]));
        s.on_frame(frame(&[(1, 0.230, 0.5012), (2, 0.770, 0.5024)]));
        s.on_frame(frame(&[(1, 0.245, 0.5016), (2, 0.755, 0.5032)]));
        s.on_frame(frame(&[(1, 0.260, 0.5020), (2, 0.740, 0.5040)]));
        s.on_frame(frame(&[(1, 0.275, 0.5024), (2, 0.725, 0.5048)]));
        s.on_frame(frame(&[(1, 0.290, 0.5028), (2, 0.710, 0.5056)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Changed")),
            "expected pinch Changed (the gesture is mostly pinch), got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Changed")),
            "rotational noise below the deadzone must not emit rotate Changed: {log:?}"
        );
    }

    /// Asymmetric two-finger motion where one contact moves much more
    /// than the other but both move in roughly the same direction —
    /// pan_qualified would slip past the strict `common > differential`
    /// test by a hair (~7%) and the small finger's motion (~0.2 mm)
    /// scraped past the old min-per-finger floor. Reproduces
    /// /tmp/rotate.txt:411-413 (one of the unintended scroll locks
    /// during the user's pinch/rotate alternation session). The
    /// balance-ratio gate (slower contact ≥ 30% of faster) plus the
    /// alignment gate close the pan-misclassification hole.
    ///
    /// Geometrically this case is indistinguishable from a slow scroll
    /// whose trailing finger lags (high alignment, very low balance,
    /// min_per_finger in the anchored noise band). The alignment-
    /// penalty gate (idea #2) treats both as ambiguous and defers
    /// rather than committing. Real pinches with crisper geometry
    /// (truly anchored finger, anti-parallel motion, or both fingers
    /// past the noise band) still lock — see
    /// `asymmetric_pinch_locks_pinch_not_pan` and
    /// `asymmetric_pinch_with_minor_motion_on_anchor_finger_locks_pinch`.
    #[test]
    fn asymmetric_directionally_correlated_motion_does_not_lock_pan() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // c0 barely moves while c1 sweeps 0.8 mm in similar direction.
        s.on_frame(frame(&[(1, 0.2172, 0.5416), (2, 0.6196, 0.5206)]));
        s.on_frame(frame(&[(1, 0.2196, 0.5412), (2, 0.6240, 0.5162)]));
        s.on_frame(frame(&[(1, 0.2210, 0.5404), (2, 0.6306, 0.5090)]));
        s.on_frame(frame(&[(1, 0.2216, 0.5400), (2, 0.6360, 0.5032)]));
        s.on_frame(frame(&[(1, 0.2220, 0.5396), (2, 0.6426, 0.4976)]));
        s.on_frame(frame(&[(1, 0.2222, 0.5394), (2, 0.6492, 0.4920)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            !log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "asymmetric motion must not classify as pan: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "ambiguous same-direction geometry must defer, not lock \
             pinch+rotate (idea #2 tradeoff): {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "ambiguous same-direction geometry must defer, not lock \
             pinch+rotate (idea #2 tradeoff): {log:?}"
        );
    }

    /// Slow scroll where the trailing finger barely moves at all
    /// (~3 mm/s vs. ~50 mm/s on the leader). Both fingers head in the
    /// same direction (cos ≈ 1.0) but their magnitudes are so different
    /// that |common| ≈ |differential|, which makes the basic
    /// common-vs-differential 1.2× margin test fail at the lock frame
    /// (1.13 < 1.2). With the basic margin failing, the lenient-pan
    /// branch of the deferral logic sees pan=0 and won't trigger;
    /// only the alignment branch (cos > PAN_ALIGNMENT_COS_MIN) catches
    /// this case. One frame later the trailing finger has caught up
    /// enough that the basic margin passes and pan_qualified flips
    /// true. Reproduces /tmp/companion-logs.txt at 2026-05-02
    /// 05:27:35.439 (rot_score=1.28, pinch_score=0.84 false lock; the
    /// next frame pan_score=3.94 dominated rot=1.79).
    #[test]
    fn slow_scroll_with_near_anchored_trailing_finger_locks_pan() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // mm coordinates / 50, taken from the user's hardware run.
        s.on_frame(frame(&[(1, 0.2704, 0.7574), (2, 0.6292, 0.6898)]));
        s.on_frame(frame(&[(1, 0.2722, 0.7430), (2, 0.6292, 0.6894)]));
        s.on_frame(frame(&[(1, 0.2784, 0.7220), (2, 0.6298, 0.6876)]));
        s.on_frame(frame(&[(1, 0.2852, 0.7038), (2, 0.6312, 0.6826)]));
        s.on_frame(frame(&[(1, 0.2900, 0.6880), (2, 0.6322, 0.6746)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "expected pan lock after one-frame deferral, got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "must not lock pinch+rotate: {log:?}"
        );
    }

    /// Slow scroll-down where the user's hand also drifts horizontally,
    /// shrinking the inter-finger distance. The lock frame catches the
    /// gesture mid-settle: balance fails by a hair (0.27 < 0.30),
    /// alignment is poor (cos = 0.45) because the y-components agree
    /// but the x-components diverge, so pan_qualified is false and
    /// pinch crosses first at score 1.12. But |common| (1.08 mm,
    /// dominantly south) already beats |differential| (0.85 mm) by
    /// >20% — the basic margin test is passing — and one frame later
    /// the trailing finger catches up enough that balance flips above
    /// 0.30. The deferral logic gives pan that one frame to qualify.
    /// Reproduces /tmp/companion-logs.txt at 2026-05-02 05:13:52.562
    /// (pinch_score=1.12, rot_score=0.80 false lock).
    #[test]
    fn slow_scroll_with_horizontal_drift_locks_pan_after_one_frame_defer() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // mm coordinates / 50: c0 sweeps south while c1 drifts east+south.
        // y-components are aligned (both south); x-components diverge.
        s.on_frame(frame(&[(1, 0.7168, 0.4774), (2, 0.2570, 0.6334)]));
        s.on_frame(frame(&[(1, 0.7154, 0.4842), (2, 0.2570, 0.6334)]));
        s.on_frame(frame(&[(1, 0.7140, 0.4968), (2, 0.2574, 0.6340)]));
        s.on_frame(frame(&[(1, 0.7126, 0.5146), (2, 0.2656, 0.6390)]));
        s.on_frame(frame(&[(1, 0.7102, 0.5336), (2, 0.2780, 0.6484)]));
        s.on_frame(frame(&[(1, 0.7086, 0.5646), (2, 0.2828, 0.6666)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "expected pan lock after one-frame deferral, got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "must not lock pinch+rotate: {log:?}"
        );
    }

    /// Slow careful scroll where one finger lags the other. Both
    /// fingers move in essentially the same direction (cos ≈ 1.0) but
    /// the trailing finger covers <15% as much ground. Without the
    /// alignment override on `pan_qualified`, the balance gate (min ≥
    /// 30% of max) disqualifies pan; pinch crosses first on the small
    /// distance change between closely-spaced fingers, and the gesture
    /// locks pinch+rotate. Reproduces /tmp/companion-logs.txt at
    /// 2026-05-02 04:55:09.044 (pinch_score=1.00, rot_score=1.62 with
    /// the user's actual finger coordinates from a SoflePLUS2 with
    /// fingers ~17 mm apart — narrow span makes pinch hypersensitive).
    #[test]
    fn slow_scroll_with_finger_lag_locks_pan() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // mm coordinates / 50: c0 sweeps ~2.3 mm south while c1 only
        // shifts ~0.3 mm in the same direction.
        s.on_frame(frame(&[(1, 0.7786, 0.3348), (2, 0.4360, 0.4008)]));
        s.on_frame(frame(&[(1, 0.7780, 0.3454), (2, 0.4354, 0.4008)]));
        s.on_frame(frame(&[(1, 0.7776, 0.3572), (2, 0.4350, 0.4024)]));
        s.on_frame(frame(&[(1, 0.7690, 0.3808), (2, 0.4350, 0.4066)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "expected pan lock, got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "must not lock pinch+rotate: {log:?}"
        );
    }

    /// Slow scroll where the leading finger sweeps ~2 mm while the
    /// trailing finger stays in the chip-noise band (~0.1 mm) for the
    /// first ~140 ms before catching up. The old single-frame deferral
    /// caught the first crossing but committed pinch+rotate on the
    /// second — high alignment (~0.96), low balance (~0.05), pan margin
    /// failing by a hair (common/diff ≈ 1.08 < 1.2), so the deferral
    /// branch couldn't fire twice. The alignment-penalty gate (idea #2)
    /// suppresses pinch/rot raw scores when the two finger-motion
    /// vectors are nearly parallel, so the gesture stays unclassified
    /// until the trailer commits enough that pan-margin passes — at
    /// which point pan locks. Reproduces user's hardware log on
    /// 2026-05-27T16:34:32 (pinch/rotate #138 in
    /// ~/Library/Logs/macos-trackpad-companion.log).
    #[test]
    fn slow_scroll_with_lazy_trailer_locks_pan_not_pinch() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // mm coordinates / 50, lifted from the user's log. y values run
        // above 1.0 because the SoflePLUS2 pad is 65 mm tall — fine for
        // the test, only relative motion matters.
        s.on_frame_at(frame(&[(1, 0.7016, 1.0884), (2, 0.3498, 1.1134)]), t0);
        s.on_frame_at(
            frame(&[(1, 0.7058, 1.0812), (2, 0.3498, 1.1134)]),
            at(t0, 18),
        );
        s.on_frame_at(
            frame(&[(1, 0.7134, 1.0680), (2, 0.3498, 1.1130)]),
            at(t0, 36),
        );
        s.on_frame_at(
            frame(&[(1, 0.7178, 1.0584), (2, 0.3502, 1.1122)]),
            at(t0, 54),
        );
        // Pre-fix lock frame: pinch crossed at align=0.98, balance=0.04.
        s.on_frame_at(
            frame(&[(1, 0.7206, 1.0532), (2, 0.3502, 1.1116)]),
            at(t0, 72),
        );
        s.on_frame_at(
            frame(&[(1, 0.7226, 1.0490), (2, 0.3498, 1.1108)]),
            at(t0, 90),
        );
        s.on_frame_at(
            frame(&[(1, 0.7240, 1.0456), (2, 0.3494, 1.1092)]),
            at(t0, 108),
        );
        s.on_frame_at(
            frame(&[(1, 0.7264, 1.0384), (2, 0.3494, 1.0982)]),
            at(t0, 126),
        );
        // c1 finally starts catching up here — pan margin passes once
        // both fingers are translating together.
        s.on_frame_at(
            frame(&[(1, 0.7416, 1.0092), (2, 0.3526, 1.0744)]),
            at(t0, 144),
        );
        s.on_frame_at(
            frame(&[(1, 0.7560, 0.9810), (2, 0.3656, 1.0466)]),
            at(t0, 162),
        );
        s.on_frame_at(frame(&[]), at(t0, 180));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "expected pan lock once trailer catches up, got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "must not lock pinch+rotate during the lazy-trailer phase: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "must not lock pinch+rotate during the lazy-trailer phase: {log:?}"
        );
    }

    /// Two fingers placed down and held mostly still for ~485 ms before
    /// the user starts scrolling. Both fingers stay within the tap-jitter
    /// floor (max per-finger displacement ≈ 0.89 mm < TAP_MAX_MOVE_MM)
    /// the whole time, but with the contacts ~18 mm apart, sub-mm jitter
    /// drifts the inter-finger angle past ROTATE_LOCK_RAD (4°) and would
    /// otherwise lock pinch+rotate at rot_score=1.00 — preempting the
    /// user's actual scroll. Reproduces user's hardware log on
    /// 2026-05-04. The admissibility gate must hold the lock off
    /// because both fingers are in the 0.3..1.0 mm noise band.
    #[test]
    fn long_settle_with_jitter_does_not_lock_pinch_rotate() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // mm coordinates / 50, sampled from user's hardware log. Real
        // device pad is 49×65 mm but only relative motion matters.
        // c0 stays near (1.15, 37.5)mm; c1 wanders within ~1mm of
        // (19.8, 33.4)mm. Total span is ~485 ms and per-finger
        // displacement at the would-be lock frame is ~0.67 mm and
        // ~0.89 mm — both under TAP_MAX_MOVE_MM (1.0).
        s.on_frame_at(frame(&[(1, 0.0224, 0.7542), (2, 0.3832, 0.6614)]), t0);
        s.on_frame_at(
            frame(&[(1, 0.0224, 0.7524), (2, 0.3942, 0.6632)]),
            at(t0, 35),
        );
        s.on_frame_at(
            frame(&[(1, 0.0230, 0.7486), (2, 0.4010, 0.6644)]),
            at(t0, 85),
        );
        s.on_frame_at(
            frame(&[(1, 0.0240, 0.7448), (2, 0.4020, 0.6652)]),
            at(t0, 135),
        );
        s.on_frame_at(
            frame(&[(1, 0.0244, 0.7444), (2, 0.4020, 0.6660)]),
            at(t0, 185),
        );
        s.on_frame_at(
            frame(&[(1, 0.0254, 0.7440), (2, 0.4010, 0.6694)]),
            at(t0, 235),
        );
        s.on_frame_at(
            frame(&[(1, 0.0264, 0.7444), (2, 0.3986, 0.6724)]),
            at(t0, 290),
        );
        s.on_frame_at(
            frame(&[(1, 0.0278, 0.7436), (2, 0.3966, 0.6750)]),
            at(t0, 345),
        );
        s.on_frame_at(
            frame(&[(1, 0.0292, 0.7426), (2, 0.3948, 0.6754)]),
            at(t0, 395),
        );
        s.on_frame_at(
            frame(&[(1, 0.0292, 0.7426), (2, 0.3938, 0.6758)]),
            at(t0, 485),
        );
        s.on_frame_at(frame(&[]), at(t0, 500));
        let log = r.pop();
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "must not lock pinch+rotate from sub-mm jitter on a wide-spread \
             baseline: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "must not lock pinch+rotate from sub-mm jitter on a wide-spread \
             baseline: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "must not lock pan either — common-mag stays well below \
             PAN_LOCK_MM here: {log:?}"
        );
    }

    /// Long-settle case where the leading finger eventually starts
    /// committing motion (scroll initiation) but the trailing finger
    /// is still drifting in the chip-noise band. Reproduces user's
    /// hardware log on 2026-05-04T23:59:38: ~570 ms of holding still,
    /// then c0 starts moving south (1.27 mm by lock) while c1 has only
    /// drifted (0.47 mm, opposite-y direction — pure noise). Without
    /// the dual-region gate, this looks structurally indistinguishable
    /// from a real anti-parallel rotation (alignment ≈ -0.94, |common| <
    /// |differential|), and rot_score crosses 1.08 — preempting what
    /// the user intends as a scroll. Holding the lock until min_per_finger
    /// crosses TAP_MAX_MOVE_MM lets the trailing finger reveal whether
    /// it's anchored, drifting opposite (real rotation), or catching up
    /// (real scroll lag).
    #[test]
    fn long_settle_then_leader_only_does_not_lock_pinch_rotate() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // mm coordinates / 50, sampled from user's hardware log. Real
        // device pad is 49×65 mm.
        s.on_frame_at(frame(&[(1, 0.7986, 0.7922), (2, 0.3900, 0.9112)]), t0);
        s.on_frame_at(
            frame(&[(1, 0.7986, 0.7922), (2, 0.3914, 0.9078)]),
            at(t0, 100),
        );
        s.on_frame_at(
            frame(&[(1, 0.7986, 0.7926), (2, 0.3924, 0.9040)]),
            at(t0, 200),
        );
        s.on_frame_at(
            frame(&[(1, 0.7976, 0.7930), (2, 0.3934, 0.9006)]),
            at(t0, 300),
        );
        s.on_frame_at(
            frame(&[(1, 0.7972, 0.7934), (2, 0.3938, 0.8992)]),
            at(t0, 400),
        );
        s.on_frame_at(
            frame(&[(1, 0.7972, 0.7944), (2, 0.3938, 0.8992)]),
            at(t0, 460),
        );
        // Leader (c0) starts heading south, trailer (c1) still drifting
        // in the noise band — would-be lock frame at ~570 ms.
        s.on_frame_at(
            frame(&[(1, 0.7972, 0.7982), (2, 0.3938, 0.8992)]),
            at(t0, 530),
        );
        s.on_frame_at(
            frame(&[(1, 0.7968, 0.8176), (2, 0.3938, 0.9026)]),
            at(t0, 569),
        );
        s.on_frame_at(frame(&[]), at(t0, 600));
        let log = r.pop();
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "leader-only motion with trailer in noise band must not lock \
             pinch+rotate: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "leader-only motion with trailer in noise band must not lock \
             pinch+rotate: {log:?}"
        );
    }

    /// Mixed pinch+rotate gesture: both admitted streams must surface
    /// Changed events throughout the locked session. Reproduces
    /// /tmp/rotate.txt:152-180 — pinch_score=1.52 at lock with a -6%
    /// scale delta, then rot deltas of -3.5°, -3.2°, -2.4° per frame
    /// while pinch deltas drop to -0.01 (rot crosses the dominance
    /// threshold by the next frame).
    #[test]
    fn mixed_pinch_rotate_emits_both_streams_in_their_dominant_frames() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // Coordinates lifted from /tmp/rotate.txt:152-174.
        s.on_frame(frame(&[(1, 0.9378, 0.6374), (2, 0.5116, 0.2222)]));
        s.on_frame(frame(&[(1, 0.9378, 0.6368), (2, 0.5116, 0.2222)]));
        s.on_frame(frame(&[(1, 0.9378, 0.6364), (2, 0.5130, 0.2218)]));
        s.on_frame(frame(&[(1, 0.9384, 0.6360), (2, 0.5148, 0.2214)]));
        s.on_frame(frame(&[(1, 0.9384, 0.6360), (2, 0.5168, 0.2214)]));
        s.on_frame(frame(&[(1, 0.9384, 0.6356), (2, 0.5182, 0.2214)]));
        s.on_frame(frame(&[(1, 0.9388, 0.6352), (2, 0.5182, 0.2226)]));
        s.on_frame(frame(&[(1, 0.9388, 0.6348), (2, 0.5172, 0.2256)]));
        s.on_frame(frame(&[(1, 0.9398, 0.6246), (2, 0.5110, 0.2344)]));
        s.on_frame(frame(&[(1, 0.9378, 0.5958), (2, 0.5024, 0.2454)]));
        s.on_frame(frame(&[(1, 0.9340, 0.5684), (2, 0.4944, 0.2526)]));
        s.on_frame(frame(&[(1, 0.9354, 0.5510), (2, 0.4880, 0.2578)]));
        s.on_frame(frame(&[(1, 0.9388, 0.5388), (2, 0.4848, 0.2602)]));
        s.on_frame(frame(&[(1, 0.9408, 0.5316), (2, 0.4824, 0.2628)]));
        s.on_frame(frame(&[(1, 0.9418, 0.5266), (2, 0.4800, 0.2674)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "expected rotate Began, got: {log:?}"
        );
        assert!(
            log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "expected pinch Began, got: {log:?}"
        );
        assert!(
            log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Changed")),
            "expected rotate Changed once rotation dominates a frame, got: {log:?}"
        );
        assert!(
            log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Changed")),
            "expected pinch Changed (lock-frame pinch delta), got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "must not lock pan: {log:?}"
        );
    }

    #[test]
    fn two_finger_pan_transitions_to_pinch_when_pinching_mid_scroll() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // Start as regular 2-finger scroll
        s.on_frame(frame(&[(1, 0.45, 0.40), (2, 0.55, 0.40)]));
        s.on_frame(frame(&[(1, 0.45, 0.45), (2, 0.55, 0.45)]));
        s.on_frame(frame(&[(1, 0.45, 0.50), (2, 0.55, 0.50)]));
        // User starts spreading fingers apart mid-scroll (scale relative >= 0.18)
        s.on_frame(frame(&[(1, 0.35, 0.50), (2, 0.65, 0.50)]));
        s.on_frame(frame(&[(1, 0.25, 0.50), (2, 0.75, 0.50)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l.starts_with("scroll") && l.contains("Began")),
            "expected initial scroll Began, got: {log:?}"
        );
        assert!(
            log.iter().any(|l| l.starts_with("scroll") && l.contains("Ended")),
            "expected scroll Ended on dynamic transition, got: {log:?}"
        );
        assert!(
            log.iter().any(|l| l.starts_with("pinch") && l.contains("Began")),
            "expected pinch Began on dynamic transition, got: {log:?}"
        );
        assert!(
            log.iter().any(|l| l.starts_with("pinch") && l.contains("Changed")),
            "expected pinch Changed, got: {log:?}"
        );
    }

    #[test]
    fn two_finger_right_edge_swipe_toggles_notification_center() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // Two fingers land near the right edge (x >= 0.85 on test pad of 50mm -> x >= 42.5mm)
        s.on_frame(frame(&[(1, 0.90, 0.50), (2, 0.90, 0.60)]));
        s.on_frame(frame(&[(1, 0.85, 0.50), (2, 0.85, 0.60)]));
        s.on_frame(frame(&[(1, 0.70, 0.50), (2, 0.70, 0.60)]));
        s.on_frame(frame(&[(1, 0.50, 0.50), (2, 0.50, 0.60)]));
        s.on_frame(frame(&[(1, 0.30, 0.50), (2, 0.30, 0.60)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l == "toggle_notification_center"),
            "expected toggle_notification_center after right edge swipe, got: {log:?}"
        );
    }

    /// Anti-parallel diagonal motion that's mostly rotation around the
    /// centroid but carries ~4% radial spread as a side effect.
    /// Reproduces the SoflePLUS2 hardware case from /tmp/rotate.txt:99-109.
    /// The original log locked at the fifth frame (c0 had moved 0.84 mm,
    /// c1 1.29 mm); the new pinch/rot admissibility gate requires
    /// `min_per_finger >= TAP_MAX_MOVE_MM` (or one finger essentially
    /// anchored), so we extend the data with one more frame continuing
    /// the same trend until c0 reaches 1.09 mm. The point of the test
    /// is the post-lock behavior — both pinch and rotate streams must
    /// fire Began concurrently — not the exact frame the lock crosses.
    #[test]
    fn antiparallel_diagonal_motion_emits_both_pinch_and_rotate() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // Coordinates pulled from /tmp/rotate.txt:99-109, normalized
        // onto the 50 mm test pad. Real device is 49×65 mm but the
        // test geometry only needs the relative motion to match.
        s.on_frame(frame(&[(1, 0.4450, 0.4722), (2, 0.8632, 0.6368)]));
        s.on_frame(frame(&[(1, 0.4408, 0.4706), (2, 0.8732, 0.6250)]));
        s.on_frame(frame(&[(1, 0.4374, 0.4706), (2, 0.8752, 0.6186)]));
        s.on_frame(frame(&[(1, 0.4350, 0.4710), (2, 0.8756, 0.6170)]));
        s.on_frame(frame(&[(1, 0.4292, 0.4778), (2, 0.8766, 0.6148)]));
        // Synthetic continuation of the same trend so the trailing
        // finger crosses TAP_MAX_MOVE_MM and the admissibility gate
        // opens.
        s.on_frame(frame(&[(1, 0.4250, 0.4810), (2, 0.8780, 0.6120)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "expected rotate Began, got: {log:?}"
        );
        assert!(
            log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "expected pinch Began, got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "must not lock pan: {log:?}"
        );
    }

    /// Anchored-finger rotate where the "anchored" finger has tiny
    /// drift (sensor noise, finger settling). Reproduces the SoflePLUS2
    /// hardware case from /tmp/rotate.txt:184 — c0 drifted by a few
    /// hundredths of a mm by lock while c1 swept tangentially. With the
    /// previous strictly-greater `common > differential` gate, those few
    /// hundredths of a mm flipped pan_qualified true, pan_score raced to
    /// ~3.5 (≈ |db|/0.8), and locked pan well before rot crossed 1.0.
    /// The min-per-finger floor disqualifies pan when one contact has
    /// barely moved, so rotate wins on the frame the angle threshold
    /// actually triggers.
    #[test]
    fn anchored_finger_rotate_with_drift_locks_rotate_not_pan() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        // c0 (id=1) at (15, 20) mm, c1 (id=2) at (15, 40) mm — 20 mm
        // apart vertically. Frames advance c1 along an arc around c0
        // (clockwise on screen, +x with slight -y as it sweeps off the
        // vertical) while c0 jitters by ~0.06 mm by lock. Drift
        // direction is roughly orthogonal to the sweep so the alignment-
        // penalty gate (idea #2) doesn't damp rot — chip noise is
        // directionally random, so picking an orthogonal direction is
        // representative.
        s.on_frame(frame(&[(1, 0.300, 0.400), (2, 0.300, 0.800)]));
        s.on_frame(frame(&[(1, 0.2999, 0.4001), (2, 0.301, 0.7998)]));
        s.on_frame(frame(&[(1, 0.2998, 0.4002), (2, 0.310, 0.7994)]));
        s.on_frame(frame(&[(1, 0.2996, 0.4003), (2, 0.328, 0.7984)]));
        s.on_frame(frame(&[(1, 0.2990, 0.4008), (2, 0.3556, 0.7962)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "expected rotate lock, got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "must not lock pan: {log:?}"
        );
    }

    #[test]
    fn three_finger_swipe_left_emits_horizontal_negative_progress() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), swipe_options());
        // Three fingers move 10mm left across 3 frames (50mm pad,
        // 0.1 normalized = 5mm; 0.5 → 0.3 = 10mm). That's well past
        // SWIPE_AXIS_LOCK_MM (3mm) so the gesture locks Horizontal
        // and emits Began with negative progress (finger moved left).
        s.on_frame(frame(&[(1, 0.5, 0.5), (2, 0.55, 0.5), (3, 0.6, 0.5)]));
        s.on_frame(frame(&[(1, 0.4, 0.5), (2, 0.45, 0.5), (3, 0.5, 0.5)]));
        s.on_frame(frame(&[(1, 0.3, 0.5), (2, 0.35, 0.5), (3, 0.4, 0.5)]));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.contains("Horizontal") && l.contains("Began") && l.contains('-')),
            "expected Horizontal Began with negative progress, got: {log:?}",
        );
    }

    /// Reproduces the spurious right-click seen at
    /// /tmp/companion-logs.txt:67 — after a 3F swipe up fired, the
    /// fingers lifted asynchronously (3 → 2 → 0 across two chip
    /// frames). Without the SwipeLatched stay-latched guard, the
    /// brief 2F window reclassified as TwoFingerUnclassified and the
    /// 2F → Idle close-out fired a Right click 10 ms later.
    #[test]
    fn async_lift_after_swipe_does_not_fire_click() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), swipe_options());
        s.on_frame(frame(&[(1, 0.4, 0.5), (2, 0.5, 0.5), (3, 0.6, 0.5)]));
        s.on_frame(frame(&[(1, 0.4, 0.3), (2, 0.5, 0.3), (3, 0.6, 0.3)]));
        // Swipe Began should have fired by here — drain the log.
        let mid = r.pop();
        assert!(
            mid.iter()
                .any(|l| l.contains("Vertical") && l.contains("Began")),
            "{mid:?}",
        );
        // Async lift: contact 2 lifts first (only 1 and 3 remain),
        // then all lift on the next frame. This is the exact pattern
        // that produced the spurious right-click on the SoflePLUS2.
        s.on_frame(frame(&[(1, 0.4, 0.3), (3, 0.6, 0.3)]));
        s.on_frame(frame(&[]));
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("click")),
            "post-swipe async lift must not fire any click, got: {log:?}",
        );
        // We do expect an Ended on the swipe stream itself.
        assert!(
            log.iter()
                .any(|l| l.contains("Vertical") && l.contains("Ended")),
            "expected swipe Ended on lift, got: {log:?}",
        );
    }

    // ── Scenarios ported from rmk's TrackpadProcessor tests ──
    //
    // These mirror the chip-side trackpad processor's behavioural
    // expectations, expressed via the same `frame()` helper (so the [0,1]
    // values get scaled onto the 50 mm test pad). Some are aspirational
    // — they describe behaviour the chip-side processor has but this
    // engine still lacks. Those are marked `#[ignore]` with a comment
    // naming the gap.
    //
    // Threshold parity: rmk's `TAP_DIST = 40` chip units on a 3936-wide,
    // 65 mm pad ≈ 0.66 mm — close to this engine's
    // `TAP_MAX_MOVE_MM = 1.0`. Slight conservatism here, since macOS
    // users expect taps to be forgiving of minor finger drift.

    fn at(t0: Timestamp, ms: u64) -> Timestamp {
        t0 + Duration::from_millis(ms)
    }

    /// Single-finger touchdown then lift, well under TAP_MAX_DURATION and
    /// without moving — emits a left click.
    #[test]
    fn short_stationary_single_finger_tap_fires_left_click() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(frame(&[(1, 0.5, 0.5)]), t0);
        s.on_frame_at(frame(&[(1, 0.5, 0.5)]), at(t0, 50));
        s.on_frame_at(frame(&[]), at(t0, 100));
        let log = r.pop();
        assert!(log.iter().any(|l| l.contains("click Left")), "{log:?}");
    }

    /// Two-finger touchdown then lift, short and stationary — right click.
    #[test]
    fn short_stationary_two_finger_tap_fires_right_click() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]), t0);
        s.on_frame_at(frame(&[]), at(t0, 80));
        s.tick(at(t0, 350));
        let log = r.pop();
        assert!(log.iter().any(|l| l.contains("click Right")), "{log:?}");
    }

    /// Touch held past TAP_MAX_DURATION with no motion — does not tap.
    /// (The chip-side processor would also latch a press-and-hold here;
    /// see `software_press_and_hold_*` tests below for that side.)
    #[test]
    fn long_touch_does_not_fire_tap() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(frame(&[(1, 0.5, 0.5)]), t0);
        // Lift well past 220 ms.
        s.on_frame_at(frame(&[]), at(t0, 400));
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("click")),
            "long touch must not tap ({log:?})",
        );
    }

    /// Single-finger touch with motion exceeding TAP_MAX_MOVE_MM — does not
    /// tap on lift, only emits cursor motion.
    #[test]
    fn motion_laden_touch_does_not_fire_tap() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Move ~2.5 mm along x (0.05 fraction of the 50 mm test pad)
        // — well past TAP_MAX_MOVE_MM = 1.0.
        s.on_frame_at(frame(&[(1, 0.50, 0.50)]), t0);
        s.on_frame_at(frame(&[(1, 0.52, 0.50)]), at(t0, 20));
        s.on_frame_at(frame(&[(1, 0.55, 0.50)]), at(t0, 40));
        s.on_frame_at(frame(&[]), at(t0, 60));
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("click")),
            "motion-laden touch must not tap ({log:?})",
        );
        assert!(
            log.iter().any(|l| l.starts_with("move")),
            "cursor motion should still emit ({log:?})",
        );
    }

    /// Diagonal short touch where every contact stays within TAP_MAX_MOVE_MM
    /// of its landing point still fires a tap. Mirrors rmk's
    /// `diagonal_short_touch_within_radius_fires_tap` — captures real-device
    /// pattern where a finger wobbles diagonally during a brisk tap.
    #[test]
    fn diagonal_short_touch_within_radius_fires_tap() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Series of small diagonal hops; final deviation from start
        // ≈ √(0.007² + 0.006²) × 50 mm ≈ 0.46 mm, well under
        // TAP_MAX_MOVE_MM = 1.0.
        s.on_frame_at(frame(&[(1, 0.500, 0.500)]), t0);
        s.on_frame_at(frame(&[(1, 0.502, 0.499)]), at(t0, 13));
        s.on_frame_at(frame(&[(1, 0.504, 0.497)]), at(t0, 26));
        s.on_frame_at(frame(&[(1, 0.506, 0.495)]), at(t0, 39));
        s.on_frame_at(frame(&[(1, 0.507, 0.494)]), at(t0, 52));
        s.on_frame_at(frame(&[]), at(t0, 75));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l.contains("click Left")),
            "diagonal short touch should still tap ({log:?})",
        );
    }

    /// Two-finger touch that pans into a scroll then lifts — the lift must
    /// not also fire a right-click tap. Centroid moved well past
    /// TAP_MAX_MOVE_MM so the tap branch on TwoFingerUnclassified→Idle
    /// shouldn't fire either.
    #[test]
    fn scroll_during_touch_does_not_fire_tap() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(frame(&[(1, 0.40, 0.50), (2, 0.60, 0.50)]), t0);
        s.on_frame_at(frame(&[(1, 0.40, 0.55), (2, 0.60, 0.55)]), at(t0, 16));
        s.on_frame_at(frame(&[(1, 0.40, 0.60), (2, 0.60, 0.60)]), at(t0, 32));
        s.on_frame_at(frame(&[]), at(t0, 48));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "expected scroll Began ({log:?})",
        );
        assert!(
            !log.iter().any(|l| l.contains("click")),
            "scroll-then-lift must not fire a tap ({log:?})",
        );
    }

    // ── Aspirational specs (mark behaviours rmk has, this engine lacks) ──

    /// Press-and-hold should latch the left button after HOLD_TIME, then
    /// pass cursor motion through with the button held, releasing on lift.
    /// Port of rmk's `software_press_and_hold_latches_button_then_drags_and_releases`.
    #[test]
    fn software_press_and_hold_latches_button_then_drags_and_releases() {
        let r = Recorder::default();
        let mut s = State::with_options(
            &r,
            test_accel(),
            GestureOptions {
                press_and_hold_drag: true,
                ..GestureOptions::default()
            },
        );
        let t0 = Timestamp::now();
        // Touch persists past the hold threshold (HOLD_TIME = 450 ms).
        s.on_frame_at(frame(&[(1, 0.50, 0.50)]), t0);
        s.on_frame_at(frame(&[(1, 0.50, 0.50)]), at(t0, 200));
        s.on_frame_at(frame(&[(1, 0.50, 0.50)]), at(t0, 460));
        // Drag motion under the held button.
        s.on_frame_at(frame(&[(1, 0.54, 0.50)]), at(t0, 475));
        s.on_frame_at(frame(&[(1, 0.58, 0.50)]), at(t0, 488));
        s.on_frame_at(frame(&[(1, 0.62, 0.50)]), at(t0, 495));
        // Lift releases the button.
        s.on_frame_at(frame(&[]), at(t0, 501));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l == "set_left_button_held true"),
            "expected explicit button press from hold latch ({log:?})",
        );
        assert!(
            log.iter().any(|l| l.starts_with("move")),
            "expected drag motion under held button ({log:?})",
        );
        assert!(
            log.iter().any(|l| l == "set_left_button_held false"),
            "expected button release on lift ({log:?})",
        );
    }

    /// Press-and-hold must not latch when the touch moves enough to
    /// disqualify, nor for two-finger sessions (those are reserved for
    /// scroll/right-click). Port of rmk's
    /// `software_press_and_hold_does_not_latch_with_motion_or_two_fingers`.
    #[test]
    fn software_press_and_hold_does_not_latch_with_motion_or_two_fingers() {
        // Motion past TAP_MAX_MOVE_MM before the hold window — no latch.
        {
            let r = Recorder::default();
            let mut s = State::with_options(
                &r,
                test_accel(),
                GestureOptions {
                    press_and_hold_drag: true,
                    ..GestureOptions::default()
                },
            );
            let t0 = Timestamp::now();
            s.on_frame_at(frame(&[(1, 0.50, 0.50)]), t0);
            s.on_frame_at(frame(&[(1, 0.55, 0.55)]), at(t0, 30));
            s.on_frame_at(frame(&[(1, 0.55, 0.55)]), at(t0, 460));
            let log = r.pop();
            assert!(
                !log.iter().any(|l| l.starts_with("set_left_button_held")),
                "motion past TAP_MAX_MOVE_MM must not latch a hold ({log:?})",
            );
        }

        // Two-finger sessions never latch a hold.
        {
            let r = Recorder::default();
            let mut s = State::with_options(
                &r,
                test_accel(),
                GestureOptions {
                    press_and_hold_drag: true,
                    ..GestureOptions::default()
                },
            );
            let t0 = Timestamp::now();
            s.on_frame_at(frame(&[(1, 0.40, 0.50), (2, 0.60, 0.50)]), t0);
            s.on_frame_at(frame(&[(1, 0.40, 0.50), (2, 0.60, 0.50)]), at(t0, 460));
            let log = r.pop();
            assert!(
                !log.iter().any(|l| l.starts_with("set_left_button_held")),
                "two-finger touch must not latch a hold ({log:?})",
            );
        }
    }

    #[test]
    fn four_finger_radial_pinch_in_triggers_launchpad() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // 4 contacts spaced around center (0.5, 0.5):
        s.on_frame_at(
            frame(&[
                (1, 0.30, 0.30),
                (2, 0.70, 0.30),
                (3, 0.30, 0.70),
                (4, 0.70, 0.70),
            ]),
            t0,
        );
        // Pinch in by ~50% toward (0.5, 0.5):
        s.on_frame_at(
            frame(&[
                (1, 0.40, 0.40),
                (2, 0.60, 0.40),
                (3, 0.40, 0.60),
                (4, 0.60, 0.60),
            ]),
            at(t0, 40),
        );
        s.on_frame_at(frame(&[]), at(t0, 80));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l == "toggle_launchpad"),
            "expected toggle_launchpad after 4F radial pinch-in, got: {log:?}"
        );
    }

    #[test]
    fn four_finger_radial_spread_out_triggers_show_desktop() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // 4 contacts close around center (0.5, 0.5):
        s.on_frame_at(
            frame(&[
                (1, 0.45, 0.45),
                (2, 0.55, 0.45),
                (3, 0.45, 0.55),
                (4, 0.55, 0.55),
            ]),
            t0,
        );
        // Spread out outward by ~60%:
        s.on_frame_at(
            frame(&[
                (1, 0.30, 0.30),
                (2, 0.70, 0.30),
                (3, 0.30, 0.70),
                (4, 0.70, 0.70),
            ]),
            at(t0, 40),
        );
        s.on_frame_at(frame(&[]), at(t0, 80));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l == "toggle_show_desktop"),
            "expected toggle_show_desktop after 4F radial spread-out, got: {log:?}"
        );
    }

    /// On finger lift, the last frame's motion is commonly a centroid-shift
    /// artifact (the contact patch shrinks asymmetrically) and should not
    /// be emitted as cursor motion. The engine buffers `dispatch_one`
    /// motion by one frame and drops the buffered value on the lift
    /// transition.
    ///
    /// Port of rmk's `lift_suppresses_prior_frame_centroid_shift_jump`.
    #[test]
    fn lift_suppresses_prior_frame_centroid_shift_jump() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Open with motion well past TAP_MAX_MOVE_MM so the could-still-tap
        // gate releases on frame 2 — otherwise no `move` lines emit and
        // the assertion has nothing to check. Then steady 2.5 mm/frame of
        // tracking motion before a 7.5 mm final-with-finger jump (the
        // artifact this test exists to suppress).
        s.on_frame_at(frame(&[(1, 0.500, 0.500)]), t0);
        s.on_frame_at(frame(&[(1, 0.550, 0.500)]), at(t0, 13));
        s.on_frame_at(frame(&[(1, 0.600, 0.500)]), at(t0, 26));
        s.on_frame_at(frame(&[(1, 0.650, 0.500)]), at(t0, 39));
        // Final with-finger frame: 7.5 mm jump.
        s.on_frame_at(frame(&[(1, 0.800, 0.500)]), at(t0, 52));
        // Lift.
        s.on_frame_at(frame(&[]), at(t0, 65));

        let log = r.pop();
        let moves: Vec<&String> = log.iter().filter(|l| l.starts_with("move ")).collect();
        assert!(
            !moves.is_empty(),
            "test must emit some move lines to be meaningful: {log:?}"
        );
        // Tracking deltas are 2.5 mm; the lift-frame jump is 7.5 mm. A 5 mm
        // ceiling separates the two — anything above is the artifact leaking.
        for line in &moves {
            if let Some(rest) = line.strip_prefix("move ") {
                let dx: f64 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                assert!(
                    dx.abs() <= 5.0,
                    "lift-frame centroid jump leaked into cursor ({line})",
                );
            }
        }
    }

    /// When a second finger lands during a one-finger touch, finger 0
    /// commonly drifts as the hand settles into the scroll posture.
    /// Cursor must not jump on those settling frames — gesture mode
    /// transitions to TwoFingerUnclassified before the user actually
    /// commits to panning.
    ///
    /// Port of rmk's `pre_scroll_two_finger_settling_does_not_emit_cursor`.
    #[test]
    fn pre_scroll_two_finger_settling_does_not_emit_cursor() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // Touchdown: 1 finger. Engine enters OneFinger mode; no motion yet.
        s.on_frame_at(frame(&[(1, 0.323, 0.535)]), t0);
        // Drain anything spurious before the second finger lands.
        let _ = r.pop();

        // Second finger lands. On this frame the engine transitions to
        // TwoFingerUnclassified — dispatch_one should NOT run for finger
        // 0's drift.
        s.on_frame_at(frame(&[(1, 0.323, 0.535), (2, 0.505, 0.453)]), at(t0, 25));
        // Subsequent settling frames: finger 0 drifts, both fingers track
        // together but slowly; centroid hasn't moved enough to lock pan.
        s.on_frame_at(frame(&[(1, 0.322, 0.535), (2, 0.505, 0.452)]), at(t0, 41));
        s.on_frame_at(frame(&[(1, 0.321, 0.534), (2, 0.504, 0.450)]), at(t0, 56));
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("move ")),
            "pre-scroll two-finger settling must not emit cursor motion ({log:?})",
        );
    }

    /// Captures the user-reported regression: small finger drift during a
    /// brisk tap (well inside both TAP_MAX_MOVE_MM = 1.0 and
    /// TAP_MAX_DURATION = 220 ms) must not push the cursor before the
    /// click lands. Pre-fix, per-frame deltas above MOTION_DEAD_ZONE_MM
    /// (0.04 mm) leaked through `dispatch_one` even when the touch was
    /// destined to resolve as a tap, so the click registered at a
    /// shifted location. The could-still-tap gate in `dispatch_one`
    /// holds cursor motion until the touch is committed to "not a tap".
    #[test]
    fn small_drift_during_tap_does_not_move_cursor() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Recreates the captured trace: ~0.13 mm total drift over 4
        // frames, lift in 70 ms — clearly a tap, but per-frame Δy hovers
        // at the dead-zone boundary. Note the helper is fed mm
        // directly here (not [0,1] fractions) so the drift figures
        // match the bug report 1:1.
        let frame_at_mm = |x: f64, y: f64| Frame {
            contacts: vec![Contact {
                id: 0,
                x,
                y,
                tip: true,
                confidence: true,
            }],
            scan_time_100us: 0,
            button: false,
        };
        s.on_frame_at(frame_at_mm(35.70, 39.04), t0);
        s.on_frame_at(frame_at_mm(35.67, 39.02), at(t0, 17));
        s.on_frame_at(frame_at_mm(35.65, 38.97), at(t0, 31));
        s.on_frame_at(frame_at_mm(35.63, 38.93), at(t0, 47));
        s.on_frame_at(
            Frame {
                contacts: vec![],
                scan_time_100us: 0,
                button: false,
            },
            at(t0, 70),
        );

        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("move ")),
            "tap-eligible drift must not move cursor ({log:?})",
        );
        assert!(
            log.iter().any(|l| l.contains("click Left")),
            "tap should still fire ({log:?})",
        );
    }

    /// Captures the user-reported regression: while panning, slow steady
    /// drift below `MOTION_DEAD_ZONE_MM` (0.04 mm) per frame must still
    /// produce scroll events as cumulative motion crosses the threshold.
    /// Pre-fix, `base.last_centroid` advanced every frame regardless of
    /// whether scroll fired, so per-frame deltas at the chip's quantum
    /// (~0.02 mm) were thrown away — a finger drifting at ~1 mm/s
    /// emitted zero `Changed` events for seconds at a time.
    #[test]
    fn slow_pan_drift_below_dead_zone_still_emits() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        let frame_two_mm = |ay: f64, by: f64| Frame {
            contacts: vec![
                Contact {
                    id: 1,
                    x: 20.0,
                    y: ay,
                    tip: true,
                    confidence: true,
                },
                Contact {
                    id: 2,
                    x: 30.0,
                    y: by,
                    tip: true,
                    confidence: true,
                },
            ],
            scan_time_100us: 0,
            button: false,
        };
        // Two fingers down; hold past TAP_MAX_DURATION (150 ms) so the
        // could-still-tap gate releases.
        s.on_frame_at(frame_two_mm(25.0, 25.0), t0);
        s.on_frame_at(frame_two_mm(25.0, 25.0), at(t0, 200));
        // One decisive frame to lock TwoFingerPan (centroid moves
        // 0.5 mm > PAN_LOCK_MM = 0.4 mm). Drain the resulting Began
        // and large initial Changed.
        s.on_frame_at(frame_two_mm(25.5, 25.5), at(t0, 216));
        let _ = r.pop();
        // Slow steady drift: 0.02 mm/frame at ~60 Hz ≈ 1.2 mm/s. Each
        // per-frame Δy is half the dead zone, so a per-frame check
        // never fires; cumulative motion crosses the dead zone every
        // 3rd frame.
        for i in 1..=10u64 {
            let y = 25.5 + 0.02 * i as f64;
            s.on_frame_at(frame_two_mm(y, y), at(t0, 216 + 16 * i));
        }
        let log = r.pop();
        let changed_emits: Vec<&String> = log
            .iter()
            .filter(|l| l.starts_with("scroll ") && l.contains("Changed"))
            .filter(|l| {
                let parts: Vec<&str> = l.split_whitespace().collect();
                let dy: f64 = parts[2].parse().unwrap();
                dy.abs() > 0.0
            })
            .collect();
        assert!(
            !changed_emits.is_empty(),
            "slow drift below per-frame dead zone must still emit scroll \
             events as cumulative motion (~0.2 mm here) crosses it ({log:?})",
        );
    }

    /// Scroll-end always seeds inertia with the EMA-smoothed velocity at
    /// lift; the `Output` decides whether the seed is fast enough to coast.
    /// Gesture-side responsibility: emit the call exactly once per
    /// scroll session, after the matching `scroll(.., Ended)`.
    #[test]
    fn scroll_lift_seeds_inertia() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Two-finger pan moving 5 mm/16ms ≈ 312 mm/s — well above any
        // sane seed threshold the Output side might apply.
        s.on_frame_at(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]), t0);
        s.on_frame_at(frame(&[(1, 0.4, 0.55), (2, 0.6, 0.55)]), at(t0, 16));
        s.on_frame_at(frame(&[(1, 0.4, 0.6), (2, 0.6, 0.6)]), at(t0, 32));
        s.on_frame_at(frame(&[(1, 0.4, 0.65), (2, 0.6, 0.65)]), at(t0, 48));
        s.on_frame_at(frame(&[]), at(t0, 64));
        let log = r.pop();
        let inertia: Vec<&String> = log
            .iter()
            .filter(|l| l.starts_with("scroll_inertia"))
            .collect();
        assert_eq!(inertia.len(), 1, "expected one inertia seed ({log:?})");
        // After 3 motion frames at +2.5 mm/16ms each, the EMA should be
        // tracking somewhere near +156 mm/s on Y. Don't pin the exact
        // value — EMA dynamics depend on how many samples land before
        // lift — but we should at least see a non-trivial Y velocity
        // and a near-zero X.
        let line = inertia[0];
        let parts: Vec<&str> = line.split_whitespace().collect();
        let vx: f64 = parts[1].parse().unwrap();
        let vy: f64 = parts[2].parse().unwrap();
        assert!(
            vy.abs() > 50.0,
            "expected Y velocity > 50 mm/s, got {vy} ({line})"
        );
        assert!(
            vx.abs() < 50.0,
            "expected near-zero X velocity, got {vx} ({line})"
        );
    }

    /// First contact after a fully-released gesture must cancel any
    /// in-flight inertia coast — otherwise a tap on the pad would
    /// "blend into" a fling instead of stopping it.
    #[test]
    fn new_touch_cancels_inertia() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Idle → 1F triggers cancel_inertia.
        s.on_frame_at(frame(&[(1, 0.5, 0.5)]), t0);
        let log = r.pop();
        assert!(
            log.iter().any(|l| l.starts_with("cancel_inertia")),
            "expected cancel_inertia on first touch ({log:?})",
        );
    }

    /// rmk's `born_during_coast`: a 1F touch that lands while a fling
    /// is coasting must cancel the inertia *and* be excluded from tap
    /// evaluation on lift. The user reached in to stop the scroll, not
    /// to click. Captures the user-reported regression where a stop-
    /// the-fling tap fired a Left click.
    #[test]
    fn one_finger_tap_during_coast_does_not_click() {
        let r = Recorder::default();
        r.set_inertia_active(true);
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(frame(&[(1, 0.5, 0.5)]), t0);
        s.on_frame_at(frame(&[(1, 0.5, 0.5)]), at(t0, 50));
        s.on_frame_at(frame(&[]), at(t0, 80));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l.contains("cancel_inertia")),
            "expected inertia cancellation on first touch ({log:?})",
        );
        assert!(
            !log.iter().any(|l| l.starts_with("click")),
            "born-during-coast tap must not fire a click ({log:?})",
        );
    }

    /// 2F-version: two fingers land during coast (e.g. user grabs the
    /// pad to stop a fling), short and stationary. Must not fire Right.
    #[test]
    fn two_finger_tap_during_coast_does_not_click() {
        let r = Recorder::default();
        r.set_inertia_active(true);
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]), t0);
        s.on_frame_at(frame(&[]), at(t0, 60));
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("click")),
            "born-during-coast 2f tap must not fire a click ({log:?})",
        );
    }

    /// After a fling stops normally (no touch), the next 1F tap should
    /// resume firing clicks — `born_during_coast` is a per-session flag
    /// and lift must clear it.
    #[test]
    fn one_finger_tap_after_coast_ends_naturally_fires_click() {
        let r = Recorder::default();
        // Inertia is NOT active for this touch (already decayed).
        r.set_inertia_active(false);
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(frame(&[(1, 0.5, 0.5)]), t0);
        s.on_frame_at(frame(&[]), at(t0, 80));
        let log = r.pop();
        assert!(
            log.iter().any(|l| l.contains("click Left")),
            "fresh 1f tap (no live coast) must still fire ({log:?})",
        );
    }

    /// Async-lift after a 2F pan: contact 0 goes tip=false a frame
    /// before contact 1, leaving a brief 1F residual. Pre-fix the
    /// residual was treated as a fresh single-finger tap and fired
    /// Left on lift. Captures the user-reported regression where
    /// scrolling sometimes ended in an accidental Left click.
    #[test]
    fn async_lift_after_two_finger_pan_does_not_fire_click() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        let one = |id, x, y, tip| Contact {
            id,
            x,
            y,
            tip,
            confidence: true,
        };
        let two = |a: Contact, b: Contact| Frame {
            contacts: vec![a, b],
            scan_time_100us: 0,
            button: false,
        };
        let single = |c: Contact| Frame {
            contacts: vec![c],
            scan_time_100us: 0,
            button: false,
        };
        // Touchdown 2F.
        s.on_frame_at(two(one(0, 20.0, 30.0, true), one(1, 35.0, 30.0, true)), t0);
        // Scroll a clearly-not-a-tap distance to lock TwoFingerPan.
        s.on_frame_at(
            two(one(0, 20.0, 33.0, true), one(1, 35.0, 33.0, true)),
            at(t0, 16),
        );
        s.on_frame_at(
            two(one(0, 20.0, 36.0, true), one(1, 35.0, 36.0, true)),
            at(t0, 32),
        );
        // Contact 0 lifts; contact 1 hangs around tip=true for one frame.
        s.on_frame_at(
            two(one(0, 20.0, 36.0, false), one(1, 35.0, 36.0, true)),
            at(t0, 48),
        );
        // Contact 1 lifts.
        s.on_frame_at(single(one(1, 35.0, 36.0, false)), at(t0, 60));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "expected scroll to begin ({log:?})",
        );
        assert!(
            !log.iter().any(|l| l.contains("click")),
            "async-lift after 2f pan must not fire a click ({log:?})",
        );
    }

    /// 2F analogue of `small_drift_during_tap_does_not_move_cursor`. A
    /// brief two-finger tap with synchronized sub-mm centroid drift sits
    /// above PAN_LOCK_MM (0.4 mm) but below TAP_MAX_MOVE_MM (1.0 mm), so
    /// pre-fix the lock branch would commit to TwoFingerPan and start
    /// emitting scroll events — and the lift would no longer fire the
    /// right-click (transition arm only checks for it from
    /// TwoFingerUnclassified). The could-still-tap gate in
    /// `dispatch_two` keeps the kind unclassified until the tap window
    /// closes.
    #[test]
    fn small_drift_during_two_finger_tap_does_not_lock_or_scroll() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Both fingers drift ~0.5 mm in the same direction over four
        // frames — centroid pan ~0.5 mm (above PAN_LOCK_MM = 0.4) but
        // each finger's max_move ~0.5 mm (under TAP_MAX_MOVE_MM = 1.0).
        let frame_at_mm = |a: (f64, f64), b: (f64, f64)| Frame {
            contacts: vec![
                Contact {
                    id: 0,
                    x: a.0,
                    y: a.1,
                    tip: true,
                    confidence: true,
                },
                Contact {
                    id: 1,
                    x: b.0,
                    y: b.1,
                    tip: true,
                    confidence: true,
                },
            ],
            scan_time_100us: 0,
            button: false,
        };
        s.on_frame_at(frame_at_mm((20.0, 30.0), (35.0, 30.0)), t0);
        s.on_frame_at(frame_at_mm((20.15, 30.15), (35.15, 30.15)), at(t0, 17));
        s.on_frame_at(frame_at_mm((20.30, 30.30), (35.30, 30.30)), at(t0, 34));
        s.on_frame_at(frame_at_mm((20.45, 30.45), (35.45, 30.45)), at(t0, 51));
        s.on_frame_at(
            Frame {
                contacts: vec![],
                scan_time_100us: 0,
                button: false,
            },
            at(t0, 75),
        );
        s.tick(at(t0, 350));

        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("scroll")),
            "tap-eligible 2F drift must not lock pan ({log:?})",
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") || l.starts_with("rotate")),
            "tap-eligible 2F drift must not lock pinch/rotate ({log:?})",
        );
        assert!(
            log.iter().any(|l| l.contains("click Right")),
            "right-click should still fire on lift ({log:?})",
        );
    }

    /// 2F tap where the two fingers don't lift in the same frame —
    /// captured from a real device trace where one finger went tip=false
    /// at t=65 ms and the other at t=77 ms (12 ms gap, well within human
    /// release tolerance). Pre-fix the engine treated the residual 12 ms
    /// of 1F as a fresh single-finger tap and fired Left; the fix
    /// recognizes the residual as the tail of the 2F lift sequence and
    /// fires Right (or, if the residual sits past the tap window,
    /// nothing).
    #[test]
    fn two_finger_tap_with_split_lift_fires_right_not_left() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        let one = |id, x, y, tip| Contact {
            id,
            x,
            y,
            tip,
            confidence: true,
        };
        let two = |a: Contact, b: Contact| Frame {
            contacts: vec![a, b],
            scan_time_100us: 0,
            button: false,
        };
        let single = |c: Contact| Frame {
            contacts: vec![c],
            scan_time_100us: 0,
            button: false,
        };
        // t=0: id=0 lands.
        s.on_frame_at(single(one(0, 15.53, 35.84, true)), t0);
        // t=19: id=1 lands → 2F.
        s.on_frame_at(
            two(one(0, 15.53, 35.84, true), one(1, 31.80, 29.50, true)),
            at(t0, 19),
        );
        // t=50: still 2F.
        s.on_frame_at(
            two(one(0, 15.50, 35.84, true), one(1, 31.80, 29.50, true)),
            at(t0, 50),
        );
        // t=65: id=0 goes tip=false (still appears in report). The
        // engine sees 1 active contact → transitions to OneFinger and
        // stashes the pending right-click.
        s.on_frame_at(
            two(one(0, 15.50, 35.84, false), one(1, 31.80, 29.50, true)),
            at(t0, 65),
        );
        // t=77: id=1 also lifts. OneFinger → Idle consumes the pending
        // right-click.
        s.on_frame_at(single(one(1, 31.80, 29.50, false)), at(t0, 77));
        s.tick(at(t0, 350));

        let log = r.pop();
        assert!(
            log.iter().any(|l| l.contains("click Right")),
            "split-lift 2F tap should fire Right ({log:?})",
        );
        assert!(
            !log.iter().any(|l| l.contains("click Left")),
            "split-lift 2F tap must not also fire Left ({log:?})",
        );
    }

    /// If the residual 1F finger sits past the original 2F tap window,
    /// the right-click is no longer eligible — and crucially, the
    /// residual must not fall through to fire its own left-click, since
    /// it's still part of the 2F lift sequence (the user didn't intend
    /// a 1F tap).
    #[test]
    fn two_finger_tap_with_long_residual_fires_nothing() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        let one = |id, x, y, tip| Contact {
            id,
            x,
            y,
            tip,
            confidence: true,
        };
        let two = |a: Contact, b: Contact| Frame {
            contacts: vec![a, b],
            scan_time_100us: 0,
            button: false,
        };
        let single = |c: Contact| Frame {
            contacts: vec![c],
            scan_time_100us: 0,
            button: false,
        };
        s.on_frame_at(single(one(0, 20.0, 30.0, true)), t0);
        s.on_frame_at(
            two(one(0, 20.0, 30.0, true), one(1, 35.0, 30.0, true)),
            at(t0, 20),
        );
        // First finger lifts at t=80 (still 2F-tap-eligible).
        s.on_frame_at(
            two(one(0, 20.0, 30.0, false), one(1, 35.0, 30.0, true)),
            at(t0, 80),
        );
        // Residual 1F holds stationary until t=400 — past the 220 ms
        // total window measured from the 2F start.
        s.on_frame_at(single(one(1, 35.0, 30.0, false)), at(t0, 400));

        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("click")),
            "long residual must fire neither Right nor Left ({log:?})",
        );
    }

    /// Curve passes through the linear `px_per_mm_at_ref · v` value at
    /// `v == ref_mm_per_sec` regardless of exponent — that's what
    /// "anchor velocity" means. If this drifts, the user's tuning
    /// intuition (`--sensitivity` = pixels/mm at ref) breaks.
    #[test]
    fn cursor_curve_anchored_at_reference_velocity() {
        let cfg = CursorAccel {
            px_per_mm_at_ref: 25.0,
            exponent: 1.5,
            ref_mm_per_sec: 80.0,
        };
        let v = 80.0; // == ref
        let pixels_per_sec = accelerate_cursor(v, cfg);
        // Linear feel at ref: 25 px/mm × 80 mm/s = 2000 px/s.
        assert!(
            (pixels_per_sec - 2000.0).abs() < 1e-6,
            "got {pixels_per_sec}"
        );
    }

    /// Exponent > 1: slow movements get sub-linear gain (more
    /// precision), fast movements get super-linear gain (faster
    /// flicks). Verify monotonicity of the *gain* (px/mm) so a
    /// regression in the formula direction is caught.
    #[test]
    fn cursor_curve_exponent_gt_one_amplifies_at_speed() {
        let cfg = CursorAccel {
            px_per_mm_at_ref: 10.0,
            exponent: 1.4,
            ref_mm_per_sec: 80.0,
        };
        let gain_at = |v: f64| accelerate_cursor(v, cfg) / v;
        let slow = gain_at(20.0);
        let anchor = gain_at(80.0);
        let fast = gain_at(320.0);
        assert!(
            slow < anchor,
            "slow gain {slow} should be < anchor {anchor}"
        );
        assert!(
            fast > anchor,
            "fast gain {fast} should be > anchor {anchor}"
        );
        // Anchor invariant.
        assert!((anchor - 10.0).abs() < 1e-6, "got {anchor}");
    }

    /// Sign is preserved on negative velocities (a common silent-bug
    /// site for power curves applied to signed values).
    #[test]
    fn cursor_curve_preserves_sign() {
        let cfg = CursorAccel {
            px_per_mm_at_ref: 25.0,
            exponent: 1.3,
            ref_mm_per_sec: 80.0,
        };
        let pos = accelerate_cursor(50.0, cfg);
        let neg = accelerate_cursor(-50.0, cfg);
        assert!(pos > 0.0 && neg < 0.0, "pos={pos} neg={neg}");
        assert!(
            (pos + neg).abs() < 1e-9,
            "magnitudes should match: {pos} vs {neg}"
        );
    }

    #[test]
    fn cursor_vector_curve_uses_speed_magnitude_once() {
        let cfg = CursorAccel {
            px_per_mm_at_ref: 25.0,
            exponent: 1.35,
            ref_mm_per_sec: 70.0,
        };
        let (x, y) = accelerate_cursor_vector(30.0, 40.0, cfg);
        let speed = 50.0;
        let expected_gain = accelerate_cursor(speed, cfg) / speed;
        assert!((x / 30.0 - expected_gain).abs() < 1e-9);
        assert!((y / 40.0 - expected_gain).abs() < 1e-9);
        assert!((x.hypot(y) - accelerate_cursor(speed, cfg)).abs() < 1e-9);
    }

    /// Fractional residual must roll over across frames — without it,
    /// a slow steady drag below 1 px/frame produces no cursor motion
    /// at all (integer truncation eats the whole delta every frame).
    /// At 1 px/mm linear, a sustained 0.5-mm/frame stream emits
    /// roughly half its frames at 1 px each (and the rest at 0). The
    /// no-carry counterfactual is "every small frame emits 0" — only
    /// the two large opening frames would produce moves at all.
    #[test]
    fn cursor_carry_accumulates_subpixel_motion_across_frames() {
        let r = Recorder::default();
        // 1 px/mm linear — keeps the arithmetic readable.
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Two opener frames clear the could-still-tap gate; their
        // motion is 2.5 mm each → emits 2 and 3 px (the 3 includes
        // the carry from the first opener).
        s.on_frame_at(frame(&[(1, 0.50, 0.50)]), t0);
        s.on_frame_at(frame(&[(1, 0.55, 0.50)]), at(t0, 13));
        s.on_frame_at(frame(&[(1, 0.60, 0.50)]), at(t0, 26));
        // Then 10 frames of 0.5-mm-per-frame motion (50 mm pad:
        // 0.01 unit/frame). Each frame is 0.5 mm = 0.5 px.
        let mut x = 0.60;
        for i in 0..10 {
            x += 0.01;
            s.on_frame_at(frame(&[(1, x, 0.50)]), at(t0, 39 + i * 13));
        }
        // One more frame so the last small-motion sample's
        // pending_motion gets emitted before lift drops it.
        x += 0.01;
        s.on_frame_at(frame(&[(1, x, 0.50)]), at(t0, 39 + 10 * 13));

        let log = r.pop();
        let move_count = log.iter().filter(|l| l.starts_with("move ")).count();
        // Without carry, only the two opener frames would produce
        // moves (each 0.5 mm small frame would truncate to 0). With
        // carry, ~half of the small frames also emit. Be lenient on
        // the exact count — the precise emit pattern depends on
        // where the carry resets and which frames the carry happens
        // to push over the integer boundary — but ≥ 5 emits proves
        // sub-pixel motion is reaching the output.
        assert!(
            move_count >= 5,
            "expected carry to convert sub-pixel frames into emits; \
             got {move_count} move lines ({log:?})",
        );
    }

    /// Partial-lift rejoin: a 2F pan that briefly drops a finger
    /// (single-frame chip drop-out, common on TPS65) and recovers
    /// within the window must resume the scroll lock instead of
    /// closing it out and re-classifying. Downstream sees the close
    /// (Ended) but then a fresh Began as the gesture continues — and
    /// crucially, no `pinch`/`rotate` Began (which is what
    /// re-classification was producing when one finger landed mid-glide).
    #[test]
    fn partial_lift_rejoin_resumes_scroll_lock() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Lock pan with a few clean scroll frames.
        s.on_frame_at(frame(&[(1, 0.4, 0.4), (2, 0.6, 0.4)]), t0);
        s.on_frame_at(frame(&[(1, 0.4, 0.45), (2, 0.6, 0.45)]), at(t0, 16));
        s.on_frame_at(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]), at(t0, 32));
        // Brief 1F drop-out (contact 2 disappears for one frame).
        s.on_frame_at(frame(&[(1, 0.4, 0.55)]), at(t0, 48));
        // 2F returns within window (~16 ms later); both fingers
        // continue the scroll trajectory.
        s.on_frame_at(frame(&[(1, 0.4, 0.6), (2, 0.6, 0.6)]), at(t0, 64));
        s.on_frame_at(frame(&[(1, 0.4, 0.65), (2, 0.6, 0.65)]), at(t0, 80));
        s.on_frame_at(frame(&[]), at(t0, 96));

        let log = r.pop();
        let scroll_began = log
            .iter()
            .filter(|l| l.starts_with("scroll") && l.contains("Began"))
            .count();
        let scroll_ended = log
            .iter()
            .filter(|l| l.starts_with("scroll") && l.contains("Ended"))
            .count();
        // One pair before the lift, one after the rejoin: two complete
        // Began/Ended brackets. The point is that the rejoin resumed
        // scroll rather than locking pinch/rotate.
        assert_eq!(
            scroll_began, 2,
            "expected scroll Began x2 (initial + rejoin), got: {log:?}"
        );
        assert_eq!(
            scroll_ended, 2,
            "expected scroll Ended x2 (lift gap + final lift), got: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") || l.starts_with("rotate")),
            "rejoin must not produce pinch/rotate events: {log:?}"
        );
    }

    /// Reproduces the structure of pinch #678 (2026-05-27 in
    /// `~/Library/Logs/macos-trackpad-companion.log`): a 2F scroll
    /// briefly drops to 1F before the lock crosses, then the chip
    /// re-acquires both contacts with one finger near its prior
    /// position and the other landing fresh. Without partial-lift
    /// continuation the rejoin frame catches one finger mid-glide
    /// and the other stationary, balance/alignment fail, and pinch
    /// locks spuriously. With continuation we preserve started_at /
    /// max_move_sq and reset per-finger anchors to the rejoin frame
    /// so the lock decision evaluates motion from a clean baseline.
    #[test]
    fn partial_lift_rejoin_during_unclassified_avoids_pinch() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Begin a 2F gesture and accumulate motion past TAP_MAX_MOVE_MM
        // so the post-rejoin Unclassified state isn't tap-eligible.
        s.on_frame_at(frame(&[(1, 0.4, 0.4), (2, 0.6, 0.4)]), t0);
        s.on_frame_at(frame(&[(1, 0.4, 0.42), (2, 0.6, 0.44)]), at(t0, 16));
        s.on_frame_at(frame(&[(1, 0.4, 0.45), (2, 0.6, 0.48)]), at(t0, 32));
        // Partial lift before lock decision crosses.
        s.on_frame_at(frame(&[(1, 0.4, 0.46)]), at(t0, 48));
        // 2F returns: contact 2 lands at a slightly different x
        // (matches the #678 pattern of an asymmetric re-acquisition).
        s.on_frame_at(frame(&[(1, 0.4, 0.5), (2, 0.58, 0.5)]), at(t0, 64));
        // Continue the scroll trajectory together for several frames.
        s.on_frame_at(frame(&[(1, 0.4, 0.54), (2, 0.58, 0.54)]), at(t0, 80));
        s.on_frame_at(frame(&[(1, 0.4, 0.58), (2, 0.58, 0.58)]), at(t0, 96));
        s.on_frame_at(frame(&[(1, 0.4, 0.62), (2, 0.58, 0.62)]), at(t0, 112));
        s.on_frame_at(frame(&[]), at(t0, 128));

        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll") && l.contains("Began")),
            "expected scroll to lock after rejoin: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("pinch") && l.contains("Began")),
            "rejoin must not lock pinch: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("rotate") && l.contains("Began")),
            "rejoin must not lock rotate: {log:?}"
        );
    }

    /// Rejoin past PARTIAL_LIFT_REJOIN_WINDOW (80 ms) is treated as
    /// a new 2F gesture: fresh baseline, no continuation. Verified by
    /// the absence of the rejoin debug log and the presence of normal
    /// fresh-start behavior.
    #[test]
    fn partial_lift_rejoin_beyond_window_starts_fresh() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Lock pan.
        s.on_frame_at(frame(&[(1, 0.4, 0.4), (2, 0.6, 0.4)]), t0);
        s.on_frame_at(frame(&[(1, 0.4, 0.45), (2, 0.6, 0.45)]), at(t0, 16));
        s.on_frame_at(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]), at(t0, 32));
        // Drop to 1F, then wait 200 ms (well past window) before 2F
        // returns. The new 2F should be a fresh gesture (its own
        // could-still-tap window, then independent classification).
        s.on_frame_at(frame(&[(1, 0.4, 0.55)]), at(t0, 48));
        s.on_frame_at(frame(&[(1, 0.4, 0.55)]), at(t0, 150));
        // Fresh 2F: contact 2 returns. New scroll motion starts here.
        s.on_frame_at(frame(&[(1, 0.4, 0.55), (2, 0.6, 0.55)]), at(t0, 250));
        s.on_frame_at(frame(&[(1, 0.4, 0.6), (2, 0.6, 0.6)]), at(t0, 266));
        s.on_frame_at(frame(&[(1, 0.4, 0.65), (2, 0.6, 0.65)]), at(t0, 282));
        s.on_frame_at(frame(&[]), at(t0, 298));

        let log = r.pop();
        // No rejoin log line (the rejection-by-age log is at debug
        // level and not captured by the recorder, but the rejoin
        // resume log isn't expected either — what we check is that
        // there are TWO separate scroll Began emits, one per
        // independent gesture).
        let scroll_began = log
            .iter()
            .filter(|l| l.starts_with("scroll") && l.contains("Began"))
            .count();
        assert_eq!(
            scroll_began, 2,
            "expected two independent scroll sessions: {log:?}"
        );
    }

    /// Surviving finger that drifts further than
    /// PARTIAL_LIFT_REJOIN_DRIFT_MM during the 1F gap suggests the
    /// chip re-issued IDs (the apparent "surviving" contact is
    /// actually a different finger). Refuse the rejoin and start a
    /// fresh 2F session.
    #[test]
    fn partial_lift_rejoin_with_surviving_drift_starts_fresh() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Lock pan.
        s.on_frame_at(frame(&[(1, 0.4, 0.4), (2, 0.6, 0.4)]), t0);
        s.on_frame_at(frame(&[(1, 0.4, 0.45), (2, 0.6, 0.45)]), at(t0, 16));
        s.on_frame_at(frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]), at(t0, 32));
        // Brief 1F. Surviving contact 1 then teleports across the pad
        // (15 mm jump in 16 ms — > PARTIAL_LIFT_REJOIN_DRIFT_MM).
        s.on_frame_at(frame(&[(1, 0.4, 0.5)]), at(t0, 48));
        // 2F returns but contact 1 is at a very different position.
        s.on_frame_at(frame(&[(1, 0.1, 0.1), (2, 0.6, 0.5)]), at(t0, 64));
        s.on_frame_at(frame(&[(1, 0.1, 0.15), (2, 0.6, 0.55)]), at(t0, 80));
        s.on_frame_at(frame(&[(1, 0.1, 0.2), (2, 0.6, 0.6)]), at(t0, 96));
        s.on_frame_at(frame(&[]), at(t0, 112));

        let log = r.pop();
        // Two separate scroll sessions: one before the drop, one
        // after the (rejected) rejoin treated as fresh.
        let scroll_began = log
            .iter()
            .filter(|l| l.starts_with("scroll") && l.contains("Began"))
            .count();
        assert_eq!(
            scroll_began, 2,
            "drift should force a fresh 2F session: {log:?}"
        );
    }

    /// Carry must reset on lift so a fresh OneFinger session can't
    /// inherit a residual pixel from the previous one.
    #[test]
    fn cursor_carry_resets_on_lift() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        // Drive enough motion to populate the carry, then lift.
        s.on_frame_at(frame(&[(1, 0.50, 0.50)]), t0);
        s.on_frame_at(frame(&[(1, 0.55, 0.50)]), at(t0, 13));
        s.on_frame_at(frame(&[(1, 0.60, 0.50)]), at(t0, 26));
        s.on_frame_at(frame(&[(1, 0.605, 0.50)]), at(t0, 39));
        s.on_frame_at(frame(&[]), at(t0, 52));
        // After lift, both carries should be exactly zero so the next
        // session starts clean.
        assert_eq!(s.cursor_carry_x_px, 0.0);
        assert_eq!(s.cursor_carry_y_px, 0.0);
    }

    // ── 三指拖移 (three-finger drag) ──
    //
    // Native 拖移样式 = 三指拖移 semantics: three fingers resting then
    // moving = left button held + accelerated cursor; the drag survives
    // asynchronous lifts (3 → 2 → 1) and releases exactly once when the
    // pad empties; drags never fire click events.

    fn drag_options() -> GestureOptions {
        GestureOptions {
            three_finger_drag: true,
            release_delay_ms: 0,
            ..GestureOptions::default()
        }
    }

    /// Three-finger drag turned off, which is what the
    /// `[gestures.three_finger_drag] enable = "off"` config produces.
    /// Three fingers then drive Dock/Spaces swipes, matching a stock
    /// macOS install where Three-Finger Drag is an Accessibility opt-in.
    fn swipe_options() -> GestureOptions {
        GestureOptions {
            three_finger_drag: false,
            ..GestureOptions::default()
        }
    }

    fn drag_lock_options() -> GestureOptions {
        GestureOptions {
            three_finger_drag: true,
            release_delay_ms: 260,
            ..GestureOptions::default()
        }
    }

    #[test]
    fn drag_engages_moves_and_releases_on_async_lift() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), drag_options());
        let t0 = Timestamp::now();
        // Land three fingers, then slide right ~1 mm/finger/frame.
        s.on_frame_at(
            frame(&[(1, 0.40, 0.50), (2, 0.50, 0.50), (3, 0.60, 0.50)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 0.42, 0.50), (2, 0.52, 0.50), (3, 0.62, 0.50)]),
            at(t0, 10),
        );
        s.on_frame_at(
            frame(&[(1, 0.44, 0.50), (2, 0.54, 0.50), (3, 0.64, 0.50)]),
            at(t0, 20),
        );
        s.on_frame_at(
            frame(&[(1, 0.46, 0.50), (2, 0.56, 0.50), (3, 0.66, 0.50)]),
            at(t0, 30),
        );
        let mid = r.pop();
        assert!(
            mid.iter().any(|l| l == "set_left_button_held true"),
            "expected drag engage, got: {mid:?}",
        );
        assert!(
            !mid.iter().any(|l| l.starts_with("click")),
            "drag onset must not click, got: {mid:?}",
        );

        // Async lift to one finger — cursor keeps streaming, button stays held.
        s.on_frame_at(frame(&[(1, 0.48, 0.50)]), at(t0, 40));
        s.on_frame_at(frame(&[(1, 0.50, 0.50)]), at(t0, 50));
        // Final empty pad releases the press exactly once.
        s.on_frame_at(frame(&[]), at(t0, 70));

        let mut log = mid;
        log.extend(r.pop());
        assert!(
            log.iter().any(|l| l.starts_with("move")),
            "cursor motion expected after finger-count drop: {log:?}",
        );
        assert!(
            !log.iter().any(|l| l.starts_with("click")),
            "drag lift must never fire clicks: {log:?}",
        );
        let held: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(_, l)| *l == "set_left_button_held true")
            .map(|(i, _)| i)
            .collect();
        let released: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(_, l)| *l == "set_left_button_held false")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(held.len(), 1, "exactly one engage: {log:?}");
        assert_eq!(released.len(), 1, "exactly one release: {log:?}");
        assert!(held[0] < released[0], "release must follow engage: {log:?}");
    }

    #[test]
    fn pre_engage_wiggle_and_lift_emits_nothing() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), drag_options());
        let t0 = Timestamp::now();
        s.on_frame_at(
            frame(&[(1, 0.40, 0.50), (2, 0.50, 0.50), (3, 0.60, 0.50)]),
            t0,
        );
        // Cumulative jitter ≈ 0.2 mm per finger — far below DRAG_ENGAGE_MM.
        s.on_frame_at(
            frame(&[(1, 0.404, 0.502), (2, 0.503, 0.501), (3, 0.603, 0.500)]),
            at(t0, 12),
        );
        s.on_frame_at(frame(&[]), at(t0, 26));
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.contains("set_left_button_held")),
            "sub-threshold touch must not engage: {log:?}",
        );
        assert!(
            !log.iter()
                .any(|l| l.starts_with("move") || l.starts_with("click")),
            "no motion/click before engage: {log:?}",
        );
    }

    #[test]
    fn four_finger_swipes_still_work_when_drag_enabled() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), drag_options());
        let t0 = Timestamp::now();
        s.on_frame_at(
            frame(&[(1, 0.5, 0.5), (2, 0.5, 0.55), (3, 0.6, 0.6), (4, 0.7, 0.65)]),
            t0,
        );
        s.on_frame_at(
            frame(&[
                (1, 0.5, 0.44),
                (2, 0.56, 0.49),
                (3, 0.62, 0.54),
                (4, 0.68, 0.59),
            ]),
            at(t0, 12),
        );
        let mid = r.pop();
        assert!(
            mid.iter()
                .any(|l| l.contains("Vertical") && l.contains("Began")),
            "vertical swipe should still fire: {mid:?}",
        );
    }

    #[test]
    fn adding_third_finger_to_locked_two_finger_pan_does_not_start_drag() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), drag_options());
        let t0 = Timestamp::now();
        let two = |y: f64| Frame {
            contacts: vec![
                Contact {
                    id: 1,
                    x: 20.0,
                    y,
                    tip: true,
                    confidence: true,
                },
                Contact {
                    id: 2,
                    x: 30.0,
                    y,
                    tip: true,
                    confidence: true,
                },
            ],
            scan_time_100us: 0,
            button: false,
        };

        // Let the two-finger gesture leave the tap window, then lock it as
        // a real pan before a third contact lands.
        s.on_frame_at(two(25.0), t0);
        s.on_frame_at(two(25.0), at(t0, 200));
        s.on_frame_at(two(27.0), at(t0, 216));
        let _ = r.pop();

        s.on_frame_at(
            Frame {
                contacts: vec![
                    Contact {
                        id: 1,
                        x: 20.0,
                        y: 27.0,
                        tip: true,
                        confidence: true,
                    },
                    Contact {
                        id: 2,
                        x: 30.0,
                        y: 27.0,
                        tip: true,
                        confidence: true,
                    },
                    Contact {
                        id: 3,
                        x: 25.0,
                        y: 27.0,
                        tip: true,
                        confidence: true,
                    },
                ],
                scan_time_100us: 0,
                button: false,
            },
            at(t0, 232),
        );
        let log = r.pop();
        assert!(
            !log.iter().any(|l| l == "set_left_button_held true"),
            "a third finger must not hijack a locked 2F pan into drag: {log:?}",
        );
        assert!(
            log.iter()
                .any(|l| l.starts_with("scroll ") && l.contains("Ended")),
            "the existing pan must close out cleanly: {log:?}",
        );
    }

    #[test]
    fn drag_lock_regrip_resets_centroid_without_pointer_jump() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), drag_lock_options());
        let t0 = Timestamp::now();
        let three = |x: f64| Frame {
            contacts: vec![
                Contact {
                    id: 1,
                    x,
                    y: 20.0,
                    tip: true,
                    confidence: true,
                },
                Contact {
                    id: 2,
                    x: x + 10.0,
                    y: 20.0,
                    tip: true,
                    confidence: true,
                },
                Contact {
                    id: 3,
                    x: x + 20.0,
                    y: 20.0,
                    tip: true,
                    confidence: true,
                },
            ],
            scan_time_100us: 0,
            button: false,
        };

        s.on_frame_at(three(20.0), t0);
        // Centroid travel crosses DRAG_ENGAGE_MM: press, but do not move
        // the pointer by the pre-engage distance.
        s.on_frame_at(three(20.8), at(t0, 10));
        s.on_frame_at(three(21.8), at(t0, 20));
        s.on_frame_at(frame(&[]), at(t0, 30));

        // Re-grip at a completely different position while drag-lock is
        // pending. The first frame is only an anchor; it must not jump.
        s.on_frame_at(three(100.0), at(t0, 100));
        s.on_frame_at(three(101.0), at(t0, 110));
        s.on_frame_at(frame(&[]), at(t0, 120));
        s.tick(at(t0, 400));

        let log = r.pop();
        let moves: Vec<&String> = log.iter().filter(|l| l.starts_with("move ")).collect();
        assert_eq!(
            moves,
            vec![&"move 1 0".to_string(), &"move 1 0".to_string()],
            "{log:?}"
        );
        assert_eq!(
            log.iter()
                .filter(|l| *l == "set_left_button_held true")
                .count(),
            1,
            "re-grip must continue one drag session: {log:?}",
        );
        assert_eq!(
            log.iter()
                .filter(|l| *l == "set_left_button_held false")
                .count(),
            1,
            "drag-lock must release exactly once: {log:?}",
        );
    }

    /// Three-finger behavior is configurable and defaults to drag (the
    /// user's chosen default for this build). These two tests pin both
    /// halves of that contract so neither side can drift silently.
    #[test]
    fn default_options_three_fingers_drag() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(
            frame(&[(1, 0.42, 0.44), (2, 0.52, 0.46), (3, 0.62, 0.48)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 0.42, 0.38), (2, 0.52, 0.40), (3, 0.62, 0.42)]),
            at(t0, 12),
        );
        let log = r.pop();
        assert!(
            log.iter().any(|l| l == "set_left_button_held true"),
            "default mode must engage three-finger drag: {log:?}",
        );
        assert!(
            !log.iter().any(|l| l.starts_with("swipe ")),
            "default mode must not also fire a swipe: {log:?}",
        );
    }

    #[test]
    fn three_fingers_swipe_when_drag_disabled() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), swipe_options());
        let t0 = Timestamp::now();
        s.on_frame_at(
            frame(&[(1, 0.42, 0.44), (2, 0.52, 0.46), (3, 0.62, 0.48)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 0.42, 0.38), (2, 0.52, 0.40), (3, 0.62, 0.42)]),
            at(t0, 12),
        );
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.contains("Vertical") && l.contains("Began")),
            "with drag disabled, 3f must swipe: {log:?}",
        );
        assert!(
            !log.iter().any(|l| l.contains("set_left_button_held")),
            "with drag disabled, 3f must never hold the button: {log:?}",
        );
    }

    #[test]
    fn one_finger_tap_drag_lifecycle() {
        let r = Recorder::default();
        let opts = GestureOptions {
            one_finger_tap_drag: true,
            ..GestureOptions::default()
        };
        let mut s = State::with_options(&r, test_accel(), opts);
        let t0 = Timestamp::now();

        // 1. First tap (down and up in 50ms)
        s.on_frame_at(frame(&[(1, 10.0, 10.0)]), t0);
        s.on_frame_at(frame(&[]), at(t0, 50));
        let log = r.pop();
        assert!(
            log.contains(&"click Left".to_string()),
            "first tap must click: {log:?}"
        );

        // 2. Second tap lands 100ms later. The press is deliberately
        //    deferred: at this instant the gesture is still either a
        //    drag or the second half of a double-click.
        s.on_frame_at(frame(&[(1, 10.0, 10.0)]), at(t0, 150));
        let log = r.pop();
        assert!(
            !log.contains(&"set_left_button_held true".to_string()),
            "landing frame must not commit to a drag yet: {log:?}"
        );

        // 3. Finger moves — that resolves it as a drag, and the press
        //    lands before any cursor motion is emitted so the grab
        //    happens on the target the user aimed at.
        s.on_frame_at(frame(&[(1, 15.0, 10.0)]), at(t0, 180));
        s.on_frame_at(frame(&[(1, 20.0, 10.0)]), at(t0, 200));
        let log = r.pop();
        let press = log.iter().position(|l| l == "set_left_button_held true");
        let first_move = log.iter().position(|l| l.starts_with("move "));
        assert!(press.is_some(), "motion must engage the drag: {log:?}");
        if let (Some(p), Some(m)) = (press, first_move) {
            assert!(p < m, "press must precede cursor motion: {log:?}");
        }

        // 4. Lift finger -> releases left button, no spurious extra click
        s.on_frame_at(frame(&[]), at(t0, 250));
        let log = r.pop();
        assert!(
            log.contains(&"set_left_button_held false".to_string()),
            "lift after tap-drag must release left button: {log:?}"
        );
        assert!(
            !log.contains(&"click Left".to_string()),
            "lift after tap-drag must not emit click: {log:?}"
        );
    }

    /// The complaint that produced this test: a real double-click was
    /// being eaten. The second tap of the pair was pressing the button
    /// on its landing frame, so macOS saw `click` followed by an
    /// unrelated press/release instead of two clicks in a row.
    #[test]
    fn quick_second_tap_completes_a_double_click_instead_of_dragging() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), GestureOptions::default());
        let t0 = Timestamp::now();

        s.on_frame_at(frame(&[(1, 10.0, 10.0)]), t0);
        s.on_frame_at(frame(&[]), at(t0, 60));
        // Second tap: down and back up in 48 ms without moving — the
        // shape of a double-click, not of a drag.
        s.on_frame_at(frame(&[(1, 10.0, 10.0)]), at(t0, 130));
        s.on_frame_at(frame(&[]), at(t0, 178));

        let log = r.pop();
        assert_eq!(
            log.iter().filter(|l| *l == "click Left").count(),
            2,
            "a tap pair must produce two clicks: {log:?}",
        );
        assert!(
            !log.iter().any(|l| l.starts_with("set_left_button_held")),
            "a double-click must never latch the button: {log:?}",
        );
    }

    /// Holding the second tap still without moving is the other half of
    /// the same decision: after `TAP_DRAG_CONFIRM` the user has clearly
    /// not double-clicked, so the drag commits even with zero motion.
    #[test]
    fn held_second_tap_engages_drag_without_motion() {
        let r = Recorder::default();
        let opts = GestureOptions {
            one_finger_tap_drag: true,
            ..GestureOptions::default()
        };
        let mut s = State::with_options(&r, test_accel(), opts);
        let t0 = Timestamp::now();

        s.on_frame_at(frame(&[(1, 10.0, 10.0)]), t0);
        s.on_frame_at(frame(&[]), at(t0, 60));
        s.on_frame_at(frame(&[(1, 10.0, 10.0)]), at(t0, 130));
        let _ = r.pop();
        // Same position, well past the confirm window.
        s.on_frame_at(frame(&[(1, 10.0, 10.0)]), at(t0, 400));

        let log = r.pop();
        assert!(
            log.contains(&"set_left_button_held true".to_string()),
            "a held second tap must commit to a drag: {log:?}",
        );
    }

    /// A link stall is not a lift. Synthesizing one produced `dur=0ms`
    /// phantom taps that macOS coalesced into double-clicks.
    #[test]
    fn link_timeout_cancels_touch_without_clicking() {
        let r = Recorder::default();
        let mut s = State::with_options(&r, test_accel(), GestureOptions::default());
        let t0 = Timestamp::now();

        s.on_frame_at(frame(&[(1, 10.0, 10.0)]), t0);
        let _ = r.pop();
        s.cancel_touch(at(t0, 250));

        let log = r.pop();
        assert!(
            !log.iter().any(|l| l.starts_with("click")),
            "a canceled touch must never click: {log:?}",
        );

        // And the cancel must not leave the engine believing a finger is
        // still down: a genuine tap right afterwards still works.
        s.on_frame_at(frame(&[(1, 30.0, 30.0)]), at(t0, 400));
        s.on_frame_at(frame(&[]), at(t0, 450));
        let log = r.pop();
        assert!(
            log.contains(&"click Left".to_string()),
            "a real tap after a cancel must still click: {log:?}",
        );
    }

    #[test]
    fn link_timeout_cancels_stream_without_seeding_inertia() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(
            Frame {
                contacts: vec![
                    Contact {
                        id: 1,
                        x: 20.0,
                        y: 20.0,
                        tip: true,
                        confidence: true,
                    },
                    Contact {
                        id: 2,
                        x: 35.0,
                        y: 20.0,
                        tip: true,
                        confidence: true,
                    },
                ],
                scan_time_100us: 0,
                button: false,
            },
            t0,
        );
        s.on_frame_at(
            Frame {
                contacts: vec![
                    Contact {
                        id: 1,
                        x: 20.0,
                        y: 24.0,
                        tip: true,
                        confidence: true,
                    },
                    Contact {
                        id: 2,
                        x: 35.0,
                        y: 24.0,
                        tip: true,
                        confidence: true,
                    },
                ],
                scan_time_100us: 0,
                button: false,
            },
            at(t0, 16),
        );
        s.on_frame_at(
            Frame {
                contacts: vec![
                    Contact {
                        id: 1,
                        x: 20.0,
                        y: 28.0,
                        tip: true,
                        confidence: true,
                    },
                    Contact {
                        id: 2,
                        x: 35.0,
                        y: 28.0,
                        tip: true,
                        confidence: true,
                    },
                ],
                scan_time_100us: 0,
                button: false,
            },
            at(t0, 32),
        );
        let _ = r.pop();
        s.cancel_touch(at(t0, 48));
        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.contains("scroll") && l.contains("Cancelled")),
            "link timeout must cancel the phased scroll stream: {log:?}"
        );
        assert!(
            !log.iter().any(|l| l.starts_with("scroll_inertia")),
            "link timeout must not seed inertia: {log:?}"
        );
    }

    #[test]
    fn two_finger_rotation_direct_lock() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // Touch 2 fingers horizontally: id 1 at (20, 20), id 2 at (40, 20) (distance 20mm, angle 0 rad)
        s.on_frame_at(frame(&[(1, 20.0, 20.0), (2, 40.0, 20.0)]), t0);

        // Rotate clockwise: id 1 moves to (20.0, 22.0), id 2 moves to (40.0, 18.0)
        // Centroid stays at (30.0, 20.0), angle rotates by ~11.3 deg (0.20 rad > ROTATE_LOCK_RAD)
        s.on_frame_at(frame(&[(1, 20.0, 22.0), (2, 40.0, 18.0)]), at(t0, 20));
        s.on_frame_at(frame(&[(1, 20.0, 24.0), (2, 40.0, 16.0)]), at(t0, 40));

        let log = r.pop();
        assert!(
            log.iter().any(|l| l.starts_with("rotate ") && (l.contains("Began") || l.contains("Changed"))),
            "pure rotation must lock and emit rotate events: {log:?}"
        );
    }

    #[test]
    fn three_finger_split_lift_lookup() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // Fractions of the 50 mm test pad: fingers 7.5 mm apart, and the
        // inter-frame drift is 0.1 mm — well inside DRAG_ENGAGE_MM, so
        // the drag never engages and the gesture stays a tap. With
        // three-finger drag on by default that distinction is the whole
        // point: a stationary three-finger tap still looks a word up.
        s.on_frame_at(
            frame(&[(1, 0.30, 0.40), (2, 0.50, 0.40), (3, 0.70, 0.40)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 0.302, 0.40), (2, 0.50, 0.402), (3, 0.70, 0.40)]),
            at(t0, 30),
        );

        // Finger 1 lifts first (3 -> 2)
        s.on_frame_at(frame(&[(2, 0.50, 0.402), (3, 0.70, 0.40)]), at(t0, 80));

        // Remaining 2 fingers lift (2 -> 0)
        s.on_frame_at(frame(&[]), at(t0, 120));

        let log = r.pop();
        assert!(
            log.contains(&"look_up_dictionary".to_string()),
            "3F split-lift within window must trigger look_up_dictionary: {log:?}"
        );
    }

    #[test]
    fn fat_finger_jitter_does_not_fire_right_click() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // Coordinates are [0,1] fractions of the 50 mm test pad, so
        // 0.08 apart is 4 mm — half of FAT_FINGER_SPLIT_MM. No hand can
        // place two fingers that close; a capacitive panel splitting one
        // contact into two blobs is the only way it happens.
        s.on_frame_at(frame(&[(1, 0.40, 0.40), (2, 0.48, 0.40)]), t0);
        // Split resolves back to 1 contact after 30ms
        s.on_frame_at(frame(&[(1, 0.40, 0.40)]), at(t0, 30));
        // Lifts after 80ms total
        s.on_frame_at(frame(&[]), at(t0, 80));

        let log = r.pop();
        assert!(
            log.contains(&"click Left".to_string()),
            "fat-finger split (<8mm) must resolve to a clean Left click, NOT Right click: {log:?}"
        );
        assert!(
            !log.contains(&"click Right".to_string()),
            "fat-finger split must never fire Right click: {log:?}"
        );
    }

    #[test]
    fn single_finger_to_two_finger_scroll_transition() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // 1 finger moves cursor down
        s.on_frame_at(frame(&[(1, 20.0, 20.0)]), t0);
        s.on_frame_at(frame(&[(1, 20.0, 25.0)]), at(t0, 30));

        // 2nd finger joins (15mm away)
        s.on_frame_at(frame(&[(1, 20.0, 26.0), (2, 35.0, 26.0)]), at(t0, 50));

        // Both fingers scroll down together
        s.on_frame_at(frame(&[(1, 20.0, 28.0), (2, 35.0, 28.0)]), at(t0, 70));
        s.on_frame_at(frame(&[(1, 20.0, 30.0), (2, 35.0, 30.0)]), at(t0, 90));

        let log = r.pop();
        assert!(
            log.iter().any(|l| l.starts_with("scroll ") && (l.contains("Began") || l.contains("Changed"))),
            "1-to-2 finger transition must engage smooth scrolling: {log:?}"
        );
    }

    #[test]
    fn three_finger_drag_to_four_finger_swipe_transition() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // 3 fingers down and drag window:
        s.on_frame_at(
            frame(&[(1, 20.0, 20.0), (2, 35.0, 20.0), (3, 50.0, 20.0)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 20.0, 23.0), (2, 35.0, 23.0), (3, 50.0, 23.0)]),
            at(t0, 40),
        );

        let log = r.pop();
        assert!(
            log.contains(&"set_left_button_held true".to_string()),
            "3F drag must hold drag button: {log:?}"
        );

        // 4th finger joins to swipe desktop (Spaces)
        s.on_frame_at(
            frame(&[
                (1, 20.0, 23.0),
                (2, 35.0, 23.0),
                (3, 50.0, 23.0),
                (4, 65.0, 23.0),
            ]),
            at(t0, 60),
        );
        // All 4 fingers slide horizontally to the left
        s.on_frame_at(
            frame(&[
                (1, 10.0, 23.0),
                (2, 25.0, 23.0),
                (3, 40.0, 23.0),
                (4, 55.0, 23.0),
            ]),
            at(t0, 90),
        );

        let log2 = r.pop();
        // The window being dragged has to travel with the cursor across
        // the Space change, so the button stays held through the
        // transition and is released when the pad finally empties.
        assert!(
            !log2.contains(&"set_left_button_held false".to_string()),
            "4F transition must carry the held button, not drop it: {log2:?}"
        );
        assert!(
            log2.iter()
                .any(|l| l.starts_with("swipe ") && l.contains("Horizontal")),
            "4F swipe must engage horizontal spaces navigation: {log2:?}"
        );

        s.on_frame_at(frame(&[]), at(t0, 130));
        let log3 = r.pop();
        assert!(
            log3.contains(&"set_left_button_held false".to_string()),
            "lifting after the carried swipe must release the button: {log3:?}"
        );
    }

    #[test]
    fn link_timeout_releases_drag_button_carried_into_four_finger_swipe() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();
        s.on_frame_at(
            frame(&[(1, 20.0, 20.0), (2, 35.0, 20.0), (3, 50.0, 20.0)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 20.0, 23.0), (2, 35.0, 23.0), (3, 50.0, 23.0)]),
            at(t0, 40),
        );
        s.on_frame_at(
            frame(&[
                (1, 20.0, 23.0),
                (2, 35.0, 23.0),
                (3, 50.0, 23.0),
                (4, 65.0, 23.0),
            ]),
            at(t0, 60),
        );
        let _ = r.pop();
        s.cancel_touch(at(t0, 80));
        let log = r.pop();
        assert!(
            log.contains(&"set_left_button_held false".to_string()),
            "link timeout must release a drag button carried into 4F: {log:?}"
        );
    }

    #[test]
    fn four_finger_swipe_finger_drop_and_recover_no_jitter() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // 4 fingers down at x = 20, 35, 50, 65 (mean cx = 42.5)
        s.on_frame_at(
            frame(&[
                (1, 20.0, 20.0),
                (2, 35.0, 20.0),
                (3, 50.0, 20.0),
                (4, 65.0, 20.0),
            ]),
            t0,
        );

        // Slide horizontally to the right: x = 25, 40, 55, 70 (+5mm, mean cx = 47.5)
        s.on_frame_at(
            frame(&[
                (1, 25.0, 20.0),
                (2, 40.0, 20.0),
                (3, 55.0, 20.0),
                (4, 70.0, 20.0),
            ]),
            at(t0, 40),
        );

        // Slide further: x = 30, 45, 60, 75 (+10mm total, mean cx = 52.5)
        s.on_frame_at(
            frame(&[
                (1, 30.0, 20.0),
                (2, 45.0, 20.0),
                (3, 60.0, 20.0),
                (4, 75.0, 20.0),
            ]),
            at(t0, 80),
        );

        let log = r.pop();
        assert!(
            log.iter()
                .any(|l| l.starts_with("swipe ") && l.contains("Horizontal")),
            "4F swipe must engage: {log:?}"
        );

        // Finger 4 momentarily lifts: remaining fingers 1, 2, 3 move +2mm (x = 32, 47, 62)
        // Raw centroid of 3 fingers is (32+47+62)/3 = 47.0 (which would have plunged backwards if unanchored!)
        s.on_frame_at(
            frame(&[(1, 32.0, 20.0), (2, 47.0, 20.0), (3, 62.0, 20.0)]),
            at(t0, 100),
        );

        // 3 fingers continue moving right +3mm (x = 35, 50, 65)
        s.on_frame_at(
            frame(&[(1, 35.0, 20.0), (2, 50.0, 20.0), (3, 65.0, 20.0)]),
            at(t0, 120),
        );

        let log2 = r.pop();
        // Progress should remain positive and monotonically increasing, never jerking negative
        let swipe_entries: Vec<&String> = log2.iter().filter(|l| l.starts_with("swipe ")).collect();
        for entry in &swipe_entries {
            // Verify progress is positive and continuous
            assert!(
                !entry.contains("progress -"),
                "Progress must not jump negative during finger count drop: {entry}"
            );
        }
    }

    #[test]
    fn two_finger_to_one_finger_with_motion_clears_pending_right_click() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // 2 fingers land stationary
        s.on_frame_at(
            frame(&[(1, 0.3, 0.3), (2, 0.6, 0.3)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 0.3, 0.3), (2, 0.6, 0.3)]),
            at(t0, 20),
        );

        // Finger 2 lifts, leaving Finger 1 down
        s.on_frame_at(
            frame(&[(1, 0.3, 0.3)]),
            at(t0, 40),
        );

        // Finger 1 moves across screen (aiming at a target: 0.3 -> 0.35 is 2.5mm on 50mm pad)
        s.on_frame_at(
            frame(&[(1, 0.33, 0.3)]),
            at(t0, 100),
        );
        s.on_frame_at(
            frame(&[(1, 0.36, 0.3)]),
            at(t0, 200),
        );

        // Finger 1 lifts to click
        s.on_frame_at(
            frame(&[]),
            at(t0, 220),
        );

        let log = r.pop();
        // Must NOT emit a right click!
        assert!(
            !log.contains(&"click Right".to_string()),
            "1F motion after 2F partial lift must clear pending right-click: {log:?}"
        );
    }

    #[test]
    fn three_finger_split_lift_full_chain_lookup() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // 3 fingers land stationary
        s.on_frame_at(
            frame(&[(1, 0.2, 0.3), (2, 0.4, 0.3), (3, 0.6, 0.3)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 0.2, 0.3), (2, 0.4, 0.3), (3, 0.6, 0.3)]),
            at(t0, 30),
        );

        // Finger 3 lifts (3F -> 2F)
        s.on_frame_at(
            frame(&[(1, 0.2, 0.3), (2, 0.4, 0.3)]),
            at(t0, 60),
        );

        // Finger 2 lifts (2F -> 1F)
        s.on_frame_at(
            frame(&[(1, 0.2, 0.3)]),
            at(t0, 90),
        );

        // Finger 1 lifts (1F -> 0F)
        s.on_frame_at(
            frame(&[]),
            at(t0, 120),
        );

        let log = r.pop();
        assert!(
            log.contains(&"look_up_dictionary".to_string()),
            "Full chain 3F->2F->1F->0 split-lift must trigger dictionary lookup: {log:?}"
        );
    }

    #[test]
    fn two_finger_rotate_clockwise_produces_negative_appkit_degrees() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        s.on_frame_at(
            frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 0.4, 0.5), (2, 0.6, 0.5)]),
            at(t0, 20),
        );

        // Rotate ~30° clockwise around centroid
        s.on_frame_at(
            frame(&[(1, 0.413, 0.45), (2, 0.587, 0.55)]),
            at(t0, 60),
        );
        s.on_frame_at(
            frame(&[(1, 0.45, 0.413), (2, 0.55, 0.587)]),
            at(t0, 100),
        );

        let log = r.pop();
        let rot_changed: Vec<&String> = log.iter().filter(|l| l.starts_with("rotate ") && l.contains("Changed")).collect();
        assert!(!rot_changed.is_empty(), "Must emit rotate Changed: {log:?}");
        for entry in rot_changed {
            assert!(
                entry.contains("rotate -"),
                "Clockwise rotation must yield negative degrees for AppKit NSEvent.rotation: {entry}"
            );
        }
    }

    #[test]
    fn smart_zoom_double_tap_dispatches_smart_magnify() {
        let r = Recorder::default();
        let mut s = State::new(&r, test_accel());
        let t0 = Timestamp::now();

        // Tap 1: 2 fingers down and up quickly (50ms)
        s.on_frame_at(
            frame(&[(1, 0.3, 0.3), (2, 0.6, 0.3)]),
            t0,
        );
        s.on_frame_at(
            frame(&[(1, 0.3, 0.3), (2, 0.6, 0.3)]),
            at(t0, 20),
        );
        s.on_frame_at(
            frame(&[]),
            at(t0, 50),
        );

        // Tap 2: 2 fingers down 100ms later and up quickly
        s.on_frame_at(
            frame(&[(1, 0.3, 0.3), (2, 0.6, 0.3)]),
            at(t0, 150),
        );
        s.on_frame_at(
            frame(&[(1, 0.3, 0.3), (2, 0.6, 0.3)]),
            at(t0, 170),
        );
        s.on_frame_at(
            frame(&[]),
            at(t0, 200),
        );

        s.tick(at(t0, 500));

        let log = r.pop();
        assert!(
            log.contains(&"smart_magnify".to_string()),
            "Two 2F taps within 220ms must emit smart_magnify: {log:?}"
        );
        assert!(
            !log.iter().any(|l| l.contains("click Right")),
            "Two 2F double tap must NEVER emit right click: {log:?}"
        );
    }
}
