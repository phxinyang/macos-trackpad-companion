//! macOS trackpad preference discovery and policy normalization.
//!
//! The preference keys are historical implementation details rather than a
//! stable public gesture API. This module therefore keeps three layers
//! separate: raw values (including their source domain), normalized policy,
//! and the conservative merge into companion configuration.

use crate::config::{Config, GestureEnable, HapticSetting};
use std::collections::BTreeMap;

#[cfg(target_os = "macos")]
const PRIMARY_DOMAIN: &str = "com.apple.AppleMultitouchTrackpad";
#[cfg(target_os = "macos")]
const FALLBACK_DOMAIN: &str = "com.apple.driver.AppleBluetoothMultitouch.trackpad";
#[cfg(target_os = "macos")]
const GLOBAL_DOMAIN: &str = ".GlobalPreferences";
const GLOBAL_NATURAL_SCROLL_KEY: &str = "com.apple.swipescrolldirection";

/// Keys collected for diagnostics. A key can be reported even when the
/// current companion build cannot faithfully reproduce its hardware/system
/// behavior.
pub const KNOWN_KEYS: &[&str] = &[
    "ActuateDetents",
    "Clicking",
    "DragLock",
    "Dragging",
    "FirstClickThreshold",
    "ForceSuppressed",
    "HIDScrollZoomModifierMask",
    "SecondClickThreshold",
    "TrackpadCornerSecondaryClick",
    "TrackpadFiveFingerPinchGesture",
    "TrackpadFourFingerHorizSwipeGesture",
    "TrackpadFourFingerPinchGesture",
    "TrackpadFourFingerVertSwipeGesture",
    "TrackpadHandResting",
    "TrackpadHorizScroll",
    "TrackpadMomentumScroll",
    "TrackpadPinch",
    "TrackpadRightClick",
    "TrackpadRotate",
    "TrackpadScroll",
    "TrackpadThreeFingerDrag",
    "TrackpadThreeFingerHorizSwipeGesture",
    "TrackpadThreeFingerTapGesture",
    "TrackpadThreeFingerVertSwipeGesture",
    "TrackpadTwoFingerDoubleTapGesture",
    "TrackpadTwoFingerFromRightEdgeSwipeGesture",
    "USBMouseStopsTrackpad",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreferenceValue {
    Integer(i64),
    Boolean(bool),
}

impl PreferenceValue {
    fn as_i64(self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(v),
            Self::Boolean(v) => Some(i64::from(v)),
        }
    }
}

/// Raw values retained for diagnostics and future key mappings.
#[derive(Clone, Debug, Default)]
pub struct RawTrackpadPreferences {
    values: BTreeMap<String, PreferenceValue>,
    sources: BTreeMap<String, String>,
    conflicts: Vec<String>,
}

impl RawTrackpadPreferences {
    pub fn value(&self, key: &str) -> Option<PreferenceValue> {
        self.values.get(key).copied()
    }

    pub fn source(&self, key: &str) -> Option<&str> {
        self.sources.get(key).map(String::as_str)
    }

    pub fn conflicts(&self) -> &[String] {
        &self.conflicts
    }

    #[cfg(test)]
    fn from_pairs(pairs: &[(&str, PreferenceValue)]) -> Self {
        let mut raw = Self::default();
        for (key, value) in pairs {
            raw.values.insert((*key).to_string(), *value);
            raw.sources.insert((*key).to_string(), "test".to_string());
        }
        raw
    }

    #[cfg(target_os = "macos")]
    fn insert(&mut self, key: &str, value: PreferenceValue, source: &str) {
        self.values.insert(key.to_string(), value);
        self.sources.insert(key.to_string(), source.to_string());
    }
}

/// Settings that have a direct, defensible application-layer equivalent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NormalizedTrackpadPolicy {
    pub haptic_feedback: Option<bool>,
    pub tap_to_click: Option<bool>,
    pub secondary_click: Option<bool>,
    pub scroll_enabled: Option<bool>,
    pub horizontal_scroll: Option<bool>,
    pub momentum_scroll: Option<bool>,
    pub natural_scroll: Option<bool>,
    pub pinch: Option<bool>,
    pub rotate: Option<bool>,
    pub smart_zoom: Option<bool>,
    pub dictionary_lookup: Option<bool>,
    pub three_finger_drag: Option<bool>,
    pub one_finger_tap_drag: Option<bool>,
    pub drag_lock: Option<bool>,
    pub right_edge_swipe: Option<bool>,
    pub horizontal_swipe: Option<bool>,
    pub vertical_swipe: Option<bool>,
    pub modifier_zoom_mask: Option<u64>,
    pub unsupported: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub enabled: bool,
    pub raw_values: usize,
    pub applied: Vec<String>,
    pub explicit_overrides: Vec<String>,
    pub unsupported: Vec<String>,
    pub conflicts: Vec<String>,
}

fn bool01(raw: &RawTrackpadPreferences, key: &str, unsupported: &mut Vec<String>) -> Option<bool> {
    let value = raw.value(key)?.as_i64()?;
    match value {
        0 => Some(false),
        1 => Some(true),
        other => {
            unsupported.push(format!("{key}={other} (expected 0/1)"));
            None
        }
    }
}

fn enum_02(raw: &RawTrackpadPreferences, key: &str, unsupported: &mut Vec<String>) -> Option<bool> {
    let value = raw.value(key)?.as_i64()?;
    match value {
        0 => Some(false),
        2 => Some(true),
        other => {
            unsupported.push(format!("{key}={other} (expected 0/2)"));
            None
        }
    }
}

fn nonzero(raw: &RawTrackpadPreferences, key: &str) -> Option<bool> {
    raw.value(key)
        .and_then(PreferenceValue::as_i64)
        .map(|v| v != 0)
}

pub fn normalize(raw: &RawTrackpadPreferences) -> NormalizedTrackpadPolicy {
    let mut policy = NormalizedTrackpadPolicy::default();
    policy.haptic_feedback = bool01(raw, "ActuateDetents", &mut policy.unsupported);
    policy.tap_to_click = bool01(raw, "Clicking", &mut policy.unsupported);
    policy.secondary_click = bool01(raw, "TrackpadRightClick", &mut policy.unsupported);
    policy.scroll_enabled = bool01(raw, "TrackpadScroll", &mut policy.unsupported);
    policy.horizontal_scroll = bool01(raw, "TrackpadHorizScroll", &mut policy.unsupported);
    policy.momentum_scroll = bool01(raw, "TrackpadMomentumScroll", &mut policy.unsupported);
    policy.pinch = bool01(raw, "TrackpadPinch", &mut policy.unsupported);
    policy.rotate = bool01(raw, "TrackpadRotate", &mut policy.unsupported);
    policy.smart_zoom = bool01(
        raw,
        "TrackpadTwoFingerDoubleTapGesture",
        &mut policy.unsupported,
    );
    policy.dictionary_lookup = nonzero(raw, "TrackpadThreeFingerTapGesture");
    policy.three_finger_drag = bool01(raw, "TrackpadThreeFingerDrag", &mut policy.unsupported);
    policy.one_finger_tap_drag = bool01(raw, "Dragging", &mut policy.unsupported);
    policy.drag_lock = bool01(raw, "DragLock", &mut policy.unsupported);
    policy.right_edge_swipe = nonzero(raw, "TrackpadTwoFingerFromRightEdgeSwipeGesture");
    policy.horizontal_swipe = enum_02(
        raw,
        "TrackpadFourFingerHorizSwipeGesture",
        &mut policy.unsupported,
    );
    policy.vertical_swipe = enum_02(
        raw,
        "TrackpadFourFingerVertSwipeGesture",
        &mut policy.unsupported,
    );
    policy.natural_scroll = raw
        .value(GLOBAL_NATURAL_SCROLL_KEY)
        .and_then(PreferenceValue::as_i64)
        .map(|v| v != 0)
        .or_else(|| bool01(raw, "TrackpadScrollNatural", &mut policy.unsupported));
    policy.modifier_zoom_mask = raw
        .value("HIDScrollZoomModifierMask")
        .and_then(PreferenceValue::as_i64)
        .and_then(|v| u64::try_from(v).ok());

    for key in [
        "FirstClickThreshold",
        "ForceSuppressed",
        "SecondClickThreshold",
        "TrackpadCornerSecondaryClick",
        "TrackpadFiveFingerPinchGesture",
        "TrackpadFourFingerPinchGesture",
        "TrackpadHandResting",
        "TrackpadThreeFingerHorizSwipeGesture",
        "TrackpadThreeFingerVertSwipeGesture",
        "USBMouseStopsTrackpad",
    ] {
        if let Some(value) = raw.value(key) {
            policy.unsupported.push(format!("{key}={value:?}"));
        }
    }
    policy
}

fn set_enable(
    cfg: &mut Config,
    path: &str,
    value: Option<bool>,
    slot: impl FnOnce(&mut Config, GestureEnable),
    report: &mut SyncReport,
) {
    let Some(value) = value else { return };
    if cfg.has_explicit(path) {
        report.explicit_overrides.push(path.to_string());
        return;
    }
    slot(
        cfg,
        if value {
            GestureEnable::On
        } else {
            GestureEnable::Off
        },
    );
    report.applied.push(path.to_string());
}

fn set_bool(
    cfg: &mut Config,
    path: &str,
    value: Option<bool>,
    slot: impl FnOnce(&mut Config, bool),
    report: &mut SyncReport,
) {
    let Some(value) = value else { return };
    if cfg.has_explicit(path) {
        report.explicit_overrides.push(path.to_string());
        return;
    }
    slot(cfg, value);
    report.applied.push(path.to_string());
}

fn merge_policy(cfg: &mut Config, policy: &NormalizedTrackpadPolicy, report: &mut SyncReport) {
    if let Some(enabled) = policy.haptic_feedback {
        if cfg.has_explicit("macos.haptic_feedback") {
            report
                .explicit_overrides
                .push("macos.haptic_feedback".to_string());
        } else {
            cfg.macos.haptic_feedback = if enabled {
                HapticSetting::On
            } else {
                HapticSetting::Off
            };
            report.applied.push("macos.haptic_feedback".to_string());
        }
    }
    set_enable(
        cfg,
        "gestures.tap_to_click",
        policy.tap_to_click,
        |cfg, value| cfg.gestures.tap_to_click = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.secondary_click",
        policy.secondary_click,
        |cfg, value| cfg.gestures.secondary_click = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.smart_zoom",
        policy.smart_zoom,
        |cfg, value| cfg.gestures.smart_zoom = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.dictionary_lookup",
        policy.dictionary_lookup,
        |cfg, value| cfg.gestures.dictionary_lookup = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.three_finger_drag",
        policy.three_finger_drag,
        |cfg, value| cfg.gestures.three_finger_drag.enable = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.one_finger_tap_drag",
        policy.one_finger_tap_drag,
        |cfg, value| cfg.gestures.one_finger_tap_drag.enable = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.right_edge_swipe",
        policy.right_edge_swipe,
        |cfg, value| cfg.gestures.right_edge_swipe = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.pinch.enable",
        policy.pinch,
        |cfg, value| cfg.gestures.pinch.enable = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.rotate.enable",
        policy.rotate,
        |cfg, value| cfg.gestures.rotate.enable = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.swipe.horizontal.enable",
        policy.horizontal_swipe,
        |cfg, value| cfg.gestures.swipe.horizontal.enable = value,
        report,
    );
    set_enable(
        cfg,
        "gestures.swipe.vertical.enable",
        policy.vertical_swipe,
        |cfg, value| cfg.gestures.swipe.vertical.enable = value,
        report,
    );

    set_bool(
        cfg,
        "scroll.enable",
        policy.scroll_enabled,
        |cfg, value| cfg.scroll.enable = value,
        report,
    );
    set_bool(
        cfg,
        "scroll.horizontal",
        policy.horizontal_scroll,
        |cfg, value| cfg.scroll.horizontal = value,
        report,
    );
    set_bool(
        cfg,
        "scroll.momentum",
        policy.momentum_scroll,
        |cfg, value| cfg.scroll.momentum = value,
        report,
    );
    set_bool(
        cfg,
        "scroll.natural",
        policy.natural_scroll,
        |cfg, value| cfg.scroll.natural = value,
        report,
    );

    if let Some(mask) = policy.modifier_zoom_mask {
        if cfg.has_explicit("scroll.modifier_zoom_mask") {
            report
                .explicit_overrides
                .push("scroll.modifier_zoom_mask".to_string());
        } else {
            cfg.scroll.modifier_zoom_mask = Some(mask);
            report.applied.push("scroll.modifier_zoom_mask".to_string());
        }
    }
    if let Some(lock) = policy.drag_lock {
        if cfg.has_explicit("gestures.three_finger_drag.release_delay_ms") {
            report
                .explicit_overrides
                .push("gestures.three_finger_drag.release_delay_ms".to_string());
        } else {
            cfg.gestures.three_finger_drag.release_delay_ms = if lock { 500 } else { 0 };
            report
                .applied
                .push("gestures.three_finger_drag.release_delay_ms".to_string());
        }
    }
}

#[cfg(target_os = "macos")]
fn read_value(domain: &str, key: &str) -> Option<PreferenceValue> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::CFTypeRef;
    use core_foundation_sys::preferences;

    let domain = CFString::new(domain);
    let key = CFString::new(key);
    let raw = unsafe {
        preferences::CFPreferencesCopyValue(
            key.as_concrete_TypeRef(),
            domain.as_concrete_TypeRef(),
            preferences::kCFPreferencesCurrentUser,
            preferences::kCFPreferencesAnyHost,
        )
    };
    if raw.is_null() {
        return None;
    }
    let value: CFType = unsafe { TCFType::wrap_under_create_rule(raw as CFTypeRef) };
    if let Some(boolean) = value.downcast::<CFBoolean>() {
        return Some(PreferenceValue::Boolean(bool::from(boolean)));
    }
    value
        .downcast::<CFNumber>()
        .and_then(|number| number.to_i64())
        .map(PreferenceValue::Integer)
}

#[cfg(target_os = "macos")]
pub fn read_raw() -> RawTrackpadPreferences {
    let mut raw = RawTrackpadPreferences::default();
    for key in KNOWN_KEYS {
        let primary = read_value(PRIMARY_DOMAIN, key);
        let fallback = read_value(FALLBACK_DOMAIN, key);
        if let (Some(a), Some(b)) = (primary, fallback)
            && a != b
        {
            raw.conflicts
                .push(format!("{key}: primary={a:?} fallback={b:?}"));
        }
        if let Some(value) = primary {
            raw.insert(key, value, PRIMARY_DOMAIN);
        } else if let Some(value) = fallback {
            raw.insert(key, value, FALLBACK_DOMAIN);
        }
    }
    if let Some(value) = read_value(GLOBAL_DOMAIN, GLOBAL_NATURAL_SCROLL_KEY) {
        raw.insert(GLOBAL_NATURAL_SCROLL_KEY, value, GLOBAL_DOMAIN);
    }
    raw
}

/// Read and merge preferences. This function is intentionally infallible:
/// preferences are advisory defaults, never a reason to fail startup.
pub fn apply(cfg: &mut Config) -> SyncReport {
    let mut report = SyncReport {
        enabled: cfg.macos.sync_system_settings,
        ..SyncReport::default()
    };
    if !cfg.macos.sync_system_settings {
        return report;
    }

    #[cfg(target_os = "macos")]
    let raw = read_raw();
    #[cfg(not(target_os = "macos"))]
    let raw = RawTrackpadPreferences::default();
    let policy = normalize(&raw);
    report.raw_values = raw.values.len();
    report.conflicts = raw.conflicts.clone();
    report.unsupported = policy.unsupported.clone();
    merge_policy(cfg, &policy, &mut report);

    #[cfg(target_os = "macos")]
    {
        for (key, value) in &raw.values {
            log::debug!(
                "macOS trackpad preference: {key}={value:?} (source={})",
                raw.source(key).unwrap_or("unknown")
            );
        }
        for conflict in &report.conflicts {
            log::warn!("macOS trackpad preference conflict: {conflict}");
        }
        for unsupported in &report.unsupported {
            log::debug!("macOS trackpad preference not mapped: {unsupported}");
        }
        log::info!(
            "macOS trackpad settings sync: {} raw values, {} applied, {} TOML overrides, {} unsupported",
            report.raw_values,
            report.applied.len(),
            report.explicit_overrides.len(),
            report.unsupported.len(),
        );
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_known_values_and_keep_unknown_enums() {
        let raw = RawTrackpadPreferences::from_pairs(&[
            ("ActuateDetents", PreferenceValue::Integer(1)),
            ("Clicking", PreferenceValue::Integer(0)),
            ("TrackpadRightClick", PreferenceValue::Integer(1)),
            ("TrackpadMomentumScroll", PreferenceValue::Integer(0)),
            (
                "TrackpadFourFingerHorizSwipeGesture",
                PreferenceValue::Integer(2),
            ),
            (
                "TrackpadFourFingerVertSwipeGesture",
                PreferenceValue::Integer(7),
            ),
            (
                "HIDScrollZoomModifierMask",
                PreferenceValue::Integer(262_144),
            ),
        ]);
        let policy = normalize(&raw);
        assert_eq!(policy.tap_to_click, Some(false));
        assert_eq!(policy.haptic_feedback, Some(true));
        assert_eq!(policy.secondary_click, Some(true));
        assert_eq!(policy.momentum_scroll, Some(false));
        assert_eq!(policy.horizontal_swipe, Some(true));
        assert_eq!(policy.vertical_swipe, None);
        assert_eq!(policy.modifier_zoom_mask, Some(262_144));
        assert!(
            policy
                .unsupported
                .iter()
                .any(|entry| entry.contains("TrackpadFourFingerVertSwipeGesture=7"))
        );
    }

    #[test]
    fn merge_respects_explicit_toml_and_maps_drag_lock() {
        let mut cfg = Config::parse_str(
            r#"
            [scroll]
            natural = true
            [gestures.pinch]
            enable = "on"
            "#,
        )
        .unwrap();
        let raw = RawTrackpadPreferences::from_pairs(&[
            ("ActuateDetents", PreferenceValue::Integer(1)),
            ("Clicking", PreferenceValue::Integer(0)),
            ("TrackpadPinch", PreferenceValue::Integer(0)),
            ("DragLock", PreferenceValue::Integer(0)),
            (GLOBAL_NATURAL_SCROLL_KEY, PreferenceValue::Integer(1)),
        ]);
        let policy = normalize(&raw);
        let mut report = SyncReport {
            enabled: true,
            ..SyncReport::default()
        };
        merge_policy(&mut cfg, &policy, &mut report);
        assert_eq!(cfg.scroll.natural, true);
        assert_eq!(cfg.macos.haptic_feedback, HapticSetting::On);
        assert_eq!(cfg.gestures.pinch.enable, GestureEnable::On);
        assert_eq!(cfg.gestures.tap_to_click, GestureEnable::Off);
        assert_eq!(cfg.gestures.three_finger_drag.release_delay_ms, 0);
        assert!(
            report
                .explicit_overrides
                .contains(&"scroll.natural".to_string())
        );
        assert!(
            report
                .explicit_overrides
                .contains(&"gestures.pinch.enable".to_string())
        );
    }
}
