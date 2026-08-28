//! Shared startup glue for the binaries: turns TOML-shaped
//! [`crate::config`] values into the policy types [`crate::output`]
//! consumes. Kept here so each bin stays thin and the mapping rules
//! have exactly one home going forward.
//!
//! Both `companion` and `companion-net` go through this module so their
//! gesture behavior stays identical.

use crate::config::{self, GestureEnable, SwipeAxisCfg};
use crate::gesture;
use crate::output;

pub fn emitter_config(cfg: &config::Config) -> output::Config {
    output::Config {
        scroll_accel: cfg.scroll.sensitivity,
        natural_scroll: cfg.scroll.natural,
        pinch: enable_to_policy(&cfg.gestures.pinch.enable),
        rotate: enable_to_policy(&cfg.gestures.rotate.enable),
        horizontal_swipe: resolve_swipe(&cfg.gestures.swipe.horizontal),
        vertical_swipe: resolve_swipe(&cfg.gestures.swipe.vertical),
    }
}

pub fn cursor_accel(cfg: &config::Config) -> gesture::CursorAccel {
    gesture::CursorAccel {
        px_per_mm_at_ref: cfg.cursor.sensitivity,
        exponent: cfg.cursor.accel_exponent,
        ref_mm_per_sec: cfg.cursor.accel_ref,
    }
}

pub fn gesture_options(cfg: &config::Config) -> gesture::GestureOptions {
    use config::GestureEnable;
    // Only/Except app-gating for drags isn't meaningful (drag targets
    // are decided by where the cursor lands); treat the table forms as
    // an implicit on.
    let three_finger_drag = !matches!(cfg.gestures.three_finger_drag.enable, GestureEnable::Off);
    let one_finger_tap_drag =
        !matches!(cfg.gestures.one_finger_tap_drag.enable, GestureEnable::Off);
    gesture::GestureOptions {
        three_finger_drag,
        one_finger_tap_drag,
        release_delay_ms: cfg.gestures.three_finger_drag.release_delay_ms,
        press_and_hold_drag: false,
    }
}

fn enable_to_policy(en: &GestureEnable) -> output::GesturePolicy {
    match en {
        GestureEnable::On => output::GesturePolicy::On,
        GestureEnable::Off => output::GesturePolicy::Off,
        GestureEnable::Only(apps) => output::GesturePolicy::Only(apps.clone()),
        GestureEnable::Except(apps) => output::GesturePolicy::Except(apps.clone()),
    }
}

fn resolve_swipe(c: &SwipeAxisCfg) -> output::SwipeConfig {
    use config::SwipeBackend;
    let backend = match c.backend {
        SwipeBackend::Synthetic => output::SwipeBackend::Synthetic,
        SwipeBackend::Notification => output::SwipeBackend::Notification,
        SwipeBackend::Off => output::SwipeBackend::Off,
    };
    output::SwipeConfig {
        policy: enable_to_policy(&c.enable),
        backend,
    }
}
