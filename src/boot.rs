//! Shared startup glue for the binaries: turns TOML-shaped
//! [`crate::config`] values into the policy types [`crate::output`]
//! consumes. Kept here so each bin stays thin and the mapping rules
//! have exactly one home going forward.
//!
//! Both `companion` and `companion-net` go through this module so their
//! gesture behavior stays identical.

use crate::config::{self, GestureEnable, HapticSetting, SwipeAxisCfg};
use crate::gesture;
use crate::output;

pub fn emitter_config(cfg: &config::Config) -> output::Config {
    let modifier_zoom_mask = cfg
        .scroll
        .modifier_zoom_mask
        .map(|mask| mask & crate::scroll_policy::SUPPORTED_ZOOM_MODIFIER_MASK)
        .unwrap_or(crate::scroll_policy::DEFAULT_MODIFIER_ZOOM_MASK);
    output::Config {
        scroll_accel: cfg.scroll.sensitivity,
        natural_scroll: cfg.scroll.natural,
        horizontal_scroll: cfg.scroll.horizontal,
        momentum_scroll: cfg.scroll.momentum,
        modifier_zoom_mask,
        shift_scroll_horizontal: cfg.scroll.shift_scroll_horizontal,
        haptic_feedback: !matches!(cfg.macos.haptic_feedback, HapticSetting::Off),
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
        tap_to_click: !matches!(cfg.gestures.tap_to_click, GestureEnable::Off),
        secondary_click: !matches!(cfg.gestures.secondary_click, GestureEnable::Off),
        smart_zoom: !matches!(cfg.gestures.smart_zoom, GestureEnable::Off),
        pinch_gain: gesture::normalize_transform_gain(cfg.gestures.pinch.gain),
        rotate_gain: gesture::normalize_transform_gain(cfg.gestures.rotate.gain),
        dynamic_transform_compat: cfg.gestures.dynamic_transform_compat,
        dictionary_lookup: !matches!(cfg.gestures.dictionary_lookup, GestureEnable::Off),
        scroll_enabled: cfg.scroll.enable,
        right_edge_swipe: !matches!(cfg.gestures.right_edge_swipe, GestureEnable::Off),
        three_finger_drag,
        one_finger_tap_drag,
        release_delay_ms: cfg.gestures.three_finger_drag.release_delay_ms,
        press_and_hold_drag: !matches!(cfg.gestures.press_and_hold_drag.enable, GestureEnable::Off),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_and_hold_setting_reaches_gesture_options() {
        let cfg: config::Config = toml::from_str(
            r#"
            [gestures.press_and_hold_drag]
            enable = "on"
        "#,
        )
        .unwrap();
        assert!(gesture_options(&cfg).press_and_hold_drag);
        assert!(!gesture_options(&config::Config::default()).press_and_hold_drag);
    }

    #[test]
    fn modifier_scroll_settings_reach_emitter_config() {
        let cfg: config::Config = toml::from_str(
            r#"
            [scroll]
            modifier_zoom_mask = 524288
            shift_scroll_horizontal = true
        "#,
        )
        .unwrap();
        let output = emitter_config(&cfg);
        assert_eq!(output.modifier_zoom_mask, 524_288);
        assert!(output.shift_scroll_horizontal);

        let defaults = emitter_config(&config::Config::default());
        assert_eq!(
            defaults.modifier_zoom_mask,
            crate::scroll_policy::DEFAULT_MODIFIER_ZOOM_MASK
        );
        assert!(!defaults.shift_scroll_horizontal);
    }

    #[test]
    fn shift_is_not_an_accessibility_zoom_modifier() {
        let cfg: config::Config = toml::from_str(
            r#"
            [scroll]
            modifier_zoom_mask = 655360
        "#,
        )
        .unwrap();
        // Shift (0x20000) | Option (0x80000): only Option reaches Zoom.
        assert_eq!(emitter_config(&cfg).modifier_zoom_mask, 0x0008_0000);
    }

    #[test]
    fn transform_gain_and_compat_settings_reach_gesture_options() {
        let cfg: config::Config = toml::from_str(
            r#"
            [gestures]
            dynamic_transform_compat = true
            [gestures.pinch]
            gain = 2.5
            [gestures.rotate]
            gain = 0.1
        "#,
        )
        .unwrap();
        let options = gesture_options(&cfg);
        assert_eq!(options.pinch_gain, 2.5);
        assert_eq!(options.rotate_gain, 0.25);
        assert!(options.dynamic_transform_compat);
    }
}
