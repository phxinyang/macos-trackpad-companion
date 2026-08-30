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
const GLOBAL_TRACKPAD_SCALING_KEY: &str = "com.apple.trackpad.scaling";
const GLOBAL_SCROLLWHEEL_SCALING_KEY: &str = "com.apple.scrollwheel.scaling";
const KNOWN_QUARTZ_MODIFIER_MASK: u64 = crate::scroll_policy::SUPPORTED_ZOOM_MODIFIER_MASK;

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

/// Legacy global scalars used by macOS' Tracking speed / scroll-wheel
/// preference panes. Their numeric domains are not a stable public API, so
/// normalization below deliberately applies bounded compatibility mappings.
#[cfg(target_os = "macos")]
const GLOBAL_SCALAR_KEYS: &[&str] = &[GLOBAL_TRACKPAD_SCALING_KEY, GLOBAL_SCROLLWHEEL_SCALING_KEY];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PreferenceValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    /// A known preference key had a non-numeric/non-boolean CF type.
    Unsupported,
}

impl PreferenceValue {
    fn as_i64(self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(v),
            Self::Boolean(v) => Some(i64::from(v)),
            Self::Float(_) | Self::Unsupported => None,
        }
    }

    fn as_f64(self) -> Option<f64> {
        match self {
            Self::Integer(v) => Some(v as f64),
            Self::Float(v) if v.is_finite() => Some(v),
            Self::Boolean(_) | Self::Float(_) | Self::Unsupported => None,
        }
    }
}

/// Raw values retained for diagnostics and future key mappings.
#[derive(Clone, Debug, Default)]
pub struct RawTrackpadPreferences {
    values: BTreeMap<String, PreferenceValue>,
    sources: BTreeMap<String, String>,
    conflicts: Vec<String>,
    trackpad_domain_available: bool,
    global_domain_available: bool,
}

impl RawTrackpadPreferences {
    /// Number of preference values collected across the primary, fallback,
    /// and global domains. This is intentionally exposed for diagnostics;
    /// callers should use `value` for individual settings.
    #[allow(dead_code)]
    pub fn value_count(&self) -> usize {
        self.values.len()
    }

    pub fn value(&self, key: &str) -> Option<PreferenceValue> {
        self.values.get(key).copied()
    }

    pub fn source(&self, key: &str) -> Option<&str> {
        self.sources.get(key).map(String::as_str)
    }

    pub fn conflicts(&self) -> &[String] {
        &self.conflicts
    }

    pub fn trackpad_domain_available(&self) -> bool {
        self.trackpad_domain_available
    }

    pub fn global_domain_available(&self) -> bool {
        self.global_domain_available
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NormalizedTrackpadPolicy {
    pub haptic_feedback: Option<bool>,
    /// Compatibility-normalized value from `com.apple.trackpad.scaling`.
    pub cursor_sensitivity: Option<f64>,
    /// `-1` in the legacy tracking-speed key means acceleration disabled;
    /// this is the one acceleration setting we can represent directly.
    pub cursor_accel_exponent: Option<f64>,
    /// Compatibility-normalized value from `com.apple.scrollwheel.scaling`.
    pub scroll_sensitivity: Option<f64>,
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
    pub trackpad_domain_available: bool,
    pub global_domain_available: bool,
    pub applied: Vec<String>,
    pub explicit_overrides: Vec<String>,
    pub unsupported: Vec<String>,
    pub conflicts: Vec<String>,
}

fn bool01(raw: &RawTrackpadPreferences, key: &str, unsupported: &mut Vec<String>) -> Option<bool> {
    let raw_value = raw.value(key)?;
    let Some(value) = raw_value.as_i64() else {
        unsupported.push(format!("{key}={raw_value:?} (expected integer 0/1)"));
        return None;
    };
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
    let raw_value = raw.value(key)?;
    let Some(value) = raw_value.as_i64() else {
        unsupported.push(format!("{key}={raw_value:?} (expected integer 0/2)"));
        return None;
    };
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

const DEFAULT_CURSOR_SENSITIVITY: f64 = 28.0;
const DEFAULT_SCROLL_SENSITIVITY: f64 = 20.0;
const DEFAULT_SCROLLWHEEL_SCALING: f64 = 0.6875;

fn normalize_scalars(raw: &RawTrackpadPreferences, policy: &mut NormalizedTrackpadPolicy) {
    if let Some(value) = raw.value(GLOBAL_TRACKPAD_SCALING_KEY) {
        match value.as_f64() {
            Some(-1.0) => policy.cursor_accel_exponent = Some(1.0),
            Some(value) if (0.0..=3.0).contains(&value) => {
                // macOS' slider is observed as 0..3, but Apple does not
                // publish a px/mm transfer function. Keep the mapping
                // bounded and anchored at the companion default.
                // A value of 1.0 is the common neutral setting. Keep the
                // slider's observed 0..3 range bounded to 0.5..2.0x.
                let factor = (0.5 + 0.5 * value).clamp(0.5, 2.0);
                policy.cursor_sensitivity = Some(DEFAULT_CURSOR_SENSITIVITY * factor);
            }
            Some(value) => policy.unsupported.push(format!(
                "{GLOBAL_TRACKPAD_SCALING_KEY}={value} (expected -1 or 0..3)"
            )),
            None => policy.unsupported.push(format!(
                "{GLOBAL_TRACKPAD_SCALING_KEY}={value:?} (expected finite number)"
            )),
        }
    }

    if let Some(value) = raw.value(GLOBAL_SCROLLWHEEL_SCALING_KEY) {
        match value.as_f64() {
            Some(value) if value > 0.0 && value <= 4.0 => {
                // 0.6875 is the commonly observed macOS baseline. This is
                // a compatibility scalar, not an Apple-documented formula.
                policy.scroll_sensitivity = Some(
                    (DEFAULT_SCROLL_SENSITIVITY * (value / DEFAULT_SCROLLWHEEL_SCALING))
                        .clamp(5.0, 80.0),
                );
            }
            Some(value) => policy.unsupported.push(format!(
                "{GLOBAL_SCROLLWHEEL_SCALING_KEY}={value} (expected 0<value<=4; -1 acceleration mode is unsupported)"
            )),
            None => policy.unsupported.push(format!(
                "{GLOBAL_SCROLLWHEEL_SCALING_KEY}={value:?} (expected finite number)"
            )),
        }
    }
}

pub fn normalize(raw: &RawTrackpadPreferences) -> NormalizedTrackpadPolicy {
    let mut policy = NormalizedTrackpadPolicy::default();
    normalize_scalars(raw, &mut policy);
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
    if let Some(value) = raw
        .value("HIDScrollZoomModifierMask")
        .and_then(PreferenceValue::as_i64)
    {
        if value < 0 {
            policy
                .unsupported
                .push(format!("HIDScrollZoomModifierMask={value} (negative)"));
        } else {
            let raw_mask = value as u64;
            let unknown = raw_mask & !KNOWN_QUARTZ_MODIFIER_MASK;
            if unknown != 0 {
                policy.unsupported.push(format!(
                    "HIDScrollZoomModifierMask unknown bits=0x{unknown:x}"
                ));
            }
            let known = raw_mask & KNOWN_QUARTZ_MODIFIER_MASK;
            if known != 0 {
                policy.modifier_zoom_mask = Some(known);
            } else {
                policy.unsupported.push(format!(
                    "HIDScrollZoomModifierMask={value} (no known Quartz modifiers)"
                ));
            }
        }
    }

    for key in [
        "FirstClickThreshold",
        "DragLock",
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

fn set_f64(
    cfg: &mut Config,
    path: &str,
    value: Option<f64>,
    slot: impl FnOnce(&mut Config, f64),
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
    set_f64(
        cfg,
        "cursor.sensitivity",
        policy.cursor_sensitivity,
        |cfg, value| cfg.cursor.sensitivity = value,
        report,
    );
    set_f64(
        cfg,
        "cursor.accel_exponent",
        policy.cursor_accel_exponent,
        |cfg, value| cfg.cursor.accel_exponent = value,
        report,
    );
    set_f64(
        cfg,
        "scroll.sensitivity",
        policy.scroll_sensitivity,
        |cfg, value| cfg.scroll.sensitivity = value,
        report,
    );
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
}

fn policy_for_mode(raw: &RawTrackpadPreferences, virtual_input: bool) -> NormalizedTrackpadPolicy {
    let mut policy = normalize(raw);
    if virtual_input {
        // The native `Clicking` preference is not meaningful for a phone or
        // browser surface. `merge_policy` still sees explicit TOML, so users
        // can opt out with `gestures.tap_to_click = "off"`.
        policy.tap_to_click = None;
    }
    policy
}

#[cfg(target_os = "macos")]
fn read_value(domain: &str, key: &str) -> Option<PreferenceValue> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::CFTypeRef;
    use core_foundation_sys::number::CFNumberIsFloatType;
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
    if let Some(number) = value.downcast::<CFNumber>() {
        if unsafe { CFNumberIsFloatType(number.as_concrete_TypeRef()) != 0 } {
            return number.to_f64().map(PreferenceValue::Float);
        }
        return number.to_i64().map(PreferenceValue::Integer);
    }
    Some(PreferenceValue::Unsupported)
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
            raw.trackpad_domain_available = true;
            raw.insert(key, value, PRIMARY_DOMAIN);
        } else if let Some(value) = fallback {
            raw.trackpad_domain_available = true;
            raw.insert(key, value, FALLBACK_DOMAIN);
        }
    }
    for key in std::iter::once(GLOBAL_NATURAL_SCROLL_KEY).chain(GLOBAL_SCALAR_KEYS.iter().copied())
    {
        if let Some(value) = read_value(GLOBAL_DOMAIN, key) {
            raw.global_domain_available = true;
            raw.insert(key, value, GLOBAL_DOMAIN);
        }
    }
    raw
}

/// Read and merge preferences for the local HID input path. This function is
/// intentionally infallible: preferences are advisory defaults, never a
/// reason to fail startup.
pub fn apply(cfg: &mut Config) -> SyncReport {
    apply_with_mode(cfg, false)
}

/// Read and merge preferences for a network/virtual input surface.
///
/// `Clicking` belongs to Apple's physical trackpad driver. A Mac mini can
/// retain that preference domain even though no trackpad is attached, and a
/// virtual phone surface has no physical click switch to mirror. Keep the
/// companion default (`tap_to_click = on`) in that mode while still honoring
/// an explicit TOML setting.
#[allow(dead_code)]
pub fn apply_for_virtual_input(cfg: &mut Config) -> SyncReport {
    apply_with_mode(cfg, true)
}

fn apply_with_mode(cfg: &mut Config, virtual_input: bool) -> SyncReport {
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
    let policy = policy_for_mode(&raw, virtual_input);
    report.raw_values = raw.values.len();
    report.trackpad_domain_available = raw.trackpad_domain_available();
    report.global_domain_available = raw.global_domain_available();
    report.conflicts = raw.conflicts().to_vec();
    report.unsupported = policy.unsupported.clone();
    merge_policy(cfg, &policy, &mut report);

    #[cfg(target_os = "macos")]
    {
        if !raw.trackpad_domain_available {
            log::info!("macOS trackpad preference domain unavailable; using TOML/default values");
        }
        if raw.value(GLOBAL_TRACKPAD_SCALING_KEY).is_none() {
            log::debug!(
                "no macOS tracking speed preference; keeping cursor.sensitivity/accel defaults"
            );
        }
        if raw.value(GLOBAL_SCROLLWHEEL_SCALING_KEY).is_none() {
            log::debug!("no macOS scroll speed preference; keeping scroll.sensitivity default");
        }
        let clicking_disabled = raw.value("Clicking") == Some(PreferenceValue::Integer(0))
            || raw.value("Clicking") == Some(PreferenceValue::Boolean(false));
        if virtual_input && clicking_disabled {
            log::info!(
                "macOS Clicking=0 belongs to the physical trackpad driver; keeping virtual-input tap-to-click enabled unless TOML explicitly disables it"
            );
        } else if clicking_disabled {
            log::warn!(
                "macOS Tap to click is disabled (Clicking=0); phone taps will not emit left clicks unless TOML explicitly enables gestures.tap_to_click"
            );
        }
        if raw.value("DragLock").is_some() {
            log::debug!(
                "macOS DragLock is diagnostic-only; three-finger re-grip uses gestures.three_finger_drag.release_delay_ms"
            );
        }
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
    fn merge_respects_explicit_toml_and_keeps_three_finger_drag_lock() {
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
        assert!(cfg.scroll.natural);
        assert_eq!(cfg.macos.haptic_feedback, HapticSetting::On);
        assert_eq!(cfg.gestures.pinch.enable, GestureEnable::On);
        assert_eq!(cfg.gestures.tap_to_click, GestureEnable::Off);
        assert_eq!(cfg.gestures.three_finger_drag.release_delay_ms, 500);
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

    #[test]
    fn modifier_mask_keeps_known_bits_and_reports_unknown_bits() {
        let raw = RawTrackpadPreferences::from_pairs(&[(
            "HIDScrollZoomModifierMask",
            PreferenceValue::Integer(0x0104_0000),
        )]);
        let policy = normalize(&raw);
        assert_eq!(policy.modifier_zoom_mask, Some(0x0004_0000));
        assert!(
            policy
                .unsupported
                .iter()
                .any(|entry| entry.contains("unknown bits=0x1000000"))
        );

        let negative = RawTrackpadPreferences::from_pairs(&[(
            "HIDScrollZoomModifierMask",
            PreferenceValue::Integer(-1),
        )]);
        let policy = normalize(&negative);
        assert_eq!(policy.modifier_zoom_mask, None);
        assert!(
            policy
                .unsupported
                .iter()
                .any(|entry| entry.contains("negative"))
        );
    }

    #[test]
    fn scaling_keys_use_bounded_compatibility_mapping() {
        let raw = RawTrackpadPreferences::from_pairs(&[
            (GLOBAL_TRACKPAD_SCALING_KEY, PreferenceValue::Float(1.0)),
            (
                GLOBAL_SCROLLWHEEL_SCALING_KEY,
                PreferenceValue::Float(0.6875),
            ),
        ]);
        let policy = normalize(&raw);
        assert_eq!(policy.cursor_sensitivity, Some(28.0));
        assert_eq!(policy.scroll_sensitivity, Some(20.0));

        let max = RawTrackpadPreferences::from_pairs(&[(
            GLOBAL_TRACKPAD_SCALING_KEY,
            PreferenceValue::Integer(3),
        )]);
        assert_eq!(normalize(&max).cursor_sensitivity, Some(56.0));

        let linear = RawTrackpadPreferences::from_pairs(&[(
            GLOBAL_TRACKPAD_SCALING_KEY,
            PreferenceValue::Integer(-1),
        )]);
        assert_eq!(normalize(&linear).cursor_accel_exponent, Some(1.0));
    }

    #[test]
    fn invalid_scaling_is_ignored_and_reported() {
        let raw = RawTrackpadPreferences::from_pairs(&[
            (
                GLOBAL_TRACKPAD_SCALING_KEY,
                PreferenceValue::Float(f64::NAN),
            ),
            (GLOBAL_SCROLLWHEEL_SCALING_KEY, PreferenceValue::Integer(-1)),
        ]);
        let policy = normalize(&raw);
        assert_eq!(policy.cursor_sensitivity, None);
        assert_eq!(policy.scroll_sensitivity, None);
        assert!(
            policy
                .unsupported
                .iter()
                .any(|x| x.contains("trackpad.scaling"))
        );
        assert!(
            policy
                .unsupported
                .iter()
                .any(|x| x.contains("scrollwheel.scaling"))
        );
    }

    #[test]
    fn explicit_toml_sensitivity_wins_over_system_scaling() {
        let mut cfg = Config::parse_str(
            r#"
            [cursor]
            sensitivity = 41.0
            [scroll]
            sensitivity = 11.0
            "#,
        )
        .unwrap();
        let raw = RawTrackpadPreferences::from_pairs(&[
            (GLOBAL_TRACKPAD_SCALING_KEY, PreferenceValue::Integer(3)),
            (
                GLOBAL_SCROLLWHEEL_SCALING_KEY,
                PreferenceValue::Float(1.375),
            ),
        ]);
        let policy = normalize(&raw);
        let mut report = SyncReport {
            enabled: true,
            ..SyncReport::default()
        };
        merge_policy(&mut cfg, &policy, &mut report);
        assert_eq!(cfg.cursor.sensitivity, 41.0);
        assert_eq!(cfg.scroll.sensitivity, 11.0);
        assert!(
            report
                .explicit_overrides
                .contains(&"cursor.sensitivity".to_string())
        );
        assert!(
            report
                .explicit_overrides
                .contains(&"scroll.sensitivity".to_string())
        );
    }

    #[test]
    fn explicit_tap_to_click_override_wins_over_clicking_off() {
        let mut cfg = Config::parse_str(
            r#"
            [gestures]
            tap_to_click = "on"
            "#,
        )
        .unwrap();
        let raw = RawTrackpadPreferences::from_pairs(&[("Clicking", PreferenceValue::Integer(0))]);
        let policy = normalize(&raw);
        let mut report = SyncReport {
            enabled: true,
            ..SyncReport::default()
        };
        merge_policy(&mut cfg, &policy, &mut report);
        assert_eq!(cfg.gestures.tap_to_click, GestureEnable::On);
        assert!(
            report
                .explicit_overrides
                .contains(&"gestures.tap_to_click".to_string())
        );
    }

    #[test]
    fn missing_preference_snapshot_is_a_valid_noop() {
        let mut cfg = Config::default();
        let policy = normalize(&RawTrackpadPreferences::default());
        let mut report = SyncReport {
            enabled: true,
            ..SyncReport::default()
        };
        merge_policy(&mut cfg, &policy, &mut report);
        assert_eq!(report.applied.len(), 0);
        assert!(report.unsupported.is_empty());
        assert_eq!(cfg.cursor.sensitivity, 28.0);
        assert_eq!(cfg.scroll.sensitivity, 20.0);
    }

    #[test]
    fn virtual_input_keeps_tap_to_click_default() {
        let mut cfg = Config::default();
        let raw = RawTrackpadPreferences::from_pairs(&[("Clicking", PreferenceValue::Integer(0))]);
        let policy = policy_for_mode(&raw, true);
        let mut report = SyncReport {
            enabled: true,
            ..SyncReport::default()
        };
        merge_policy(&mut cfg, &policy, &mut report);
        assert_eq!(cfg.gestures.tap_to_click, GestureEnable::On);
    }

    #[test]
    fn virtual_input_respects_explicit_tap_to_click_off() {
        let mut cfg = Config::parse_str(
            r#"
            [gestures]
            tap_to_click = "off"
            "#,
        )
        .unwrap();
        let raw = RawTrackpadPreferences::from_pairs(&[("Clicking", PreferenceValue::Integer(0))]);
        let policy = policy_for_mode(&raw, true);
        let mut report = SyncReport {
            enabled: true,
            ..SyncReport::default()
        };
        merge_policy(&mut cfg, &policy, &mut report);
        assert_eq!(cfg.gestures.tap_to_click, GestureEnable::Off);
    }
}
