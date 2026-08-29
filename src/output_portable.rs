//! Platform-neutral gesture output contract used by the engine tests.
//!
//! The real CGEvent/IOKit emitter lives in `output.rs` and is compiled only
//! on macOS. Keeping this contract available on other hosts lets the gesture
//! classifier and wire protocol run deterministic tests in CI without
//! pretending that Linux can post macOS events.

use crate::time::Timestamp;

#[derive(Clone, Debug)]
pub struct Config {
    pub scroll_accel: f64,
    pub natural_scroll: bool,
    pub horizontal_scroll: bool,
    pub momentum_scroll: bool,
    pub modifier_zoom_mask: u64,
    pub shift_scroll_horizontal: bool,
    pub haptic_feedback: bool,
    pub pinch: GesturePolicy,
    pub rotate: GesturePolicy,
    pub horizontal_swipe: SwipeConfig,
    pub vertical_swipe: SwipeConfig,
}

#[derive(Clone, Debug)]
pub struct SwipeConfig {
    pub policy: GesturePolicy,
    pub backend: SwipeBackend,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GesturePolicy {
    On,
    Off,
    Only(Vec<String>),
    Except(Vec<String>),
}

impl GesturePolicy {
    pub fn evaluate(&self, lookup: impl FnOnce() -> Option<String>) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Only(list) => lookup().is_some_and(|id| list.iter().any(|x| x == &id)),
            Self::Except(list) => lookup().is_none_or(|id| !list.iter().any(|x| x == &id)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeBackend {
    Synthetic,
    Notification,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Began,
    Changed,
    Ended,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HapticKind {
    Click,
    DragEngaged,
    GestureCommitted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

pub trait Output {
    fn set_event_time(&self, _ts: Timestamp) {}
    fn haptic(&self, _kind: HapticKind) {}
    fn pinch_admissible_now(&self) -> bool {
        true
    }
    fn rotate_admissible_now(&self) -> bool {
        true
    }
    fn swipe_admissible_now(&self, _axis: SwipeAxis) -> bool {
        true
    }
    fn move_cursor_by(&self, dx_px: i32, dy_px: i32);
    fn click(&self, button: MouseButton);
    /// Return currently held Quartz modifier bits when a platform can
    /// observe them. Linux/test emitters intentionally return zero.
    fn current_modifiers(&self) -> u64 {
        0
    }
    /// Emit a click using a modifier snapshot captured at gesture time.
    fn click_with_modifiers(&self, button: MouseButton, _modifiers: u64) {
        self.click(button);
    }
    fn set_left_button_held(&self, held: bool);
    fn set_drag_button_held(&self, held: bool) {
        self.set_left_button_held(held);
    }
    fn scroll(&self, dx_mm: f64, dy_mm: f64, phase: Phase);
    fn scroll_inertia(&self, vx_mm_per_sec: f64, vy_mm_per_sec: f64);
    fn cancel_inertia(&self) -> bool {
        false
    }
    fn pinch(&self, delta: f64, phase: Phase);
    fn rotate(&self, delta_degrees: f64, phase: Phase);
    fn swipe(&self, axis: SwipeAxis, signed_progress: f64, velocity_mm_per_sec: f64, phase: Phase);
    fn look_up_dictionary(&self) {}
    fn smart_magnify(&self) {}
    fn toggle_notification_center(&self) {}
    fn toggle_launchpad(&self) {}
    fn toggle_show_desktop(&self) {}
    fn toggle_app_expose(&self) {}
    fn toggle_mission_control(&self) {}
}

/// No-op emitter used only when a downstream crate wants to exercise the
/// public startup API on Linux. It intentionally never pretends to deliver
/// macOS events; production binaries are gated to `target_os = "macos"`.
#[derive(Default)]
pub struct Emitter;

impl Emitter {
    pub fn new(_cfg: Config) -> Self {
        Self
    }
}

impl Output for Emitter {
    fn move_cursor_by(&self, _dx_px: i32, _dy_px: i32) {}
    fn click(&self, _button: MouseButton) {}
    fn set_left_button_held(&self, _held: bool) {}
    fn scroll(&self, _dx_mm: f64, _dy_mm: f64, _phase: Phase) {}
    fn scroll_inertia(&self, _vx_mm_per_sec: f64, _vy_mm_per_sec: f64) {}
    fn pinch(&self, _delta: f64, _phase: Phase) {}
    fn rotate(&self, _delta_degrees: f64, _phase: Phase) {}
    fn swipe(
        &self,
        _axis: SwipeAxis,
        _signed_progress: f64,
        _velocity_mm_per_sec: f64,
        _phase: Phase,
    ) {
    }
}
