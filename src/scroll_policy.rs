//! Cross-platform state policy for modifier-based scroll routing.
//!
//! The macOS emitter owns the actual CGEvent posting, but the decision about
//! whether a scroll session is redirected to magnification is pure state and
//! should remain testable on every host.

pub(crate) const MOD_CMD: u64 = 0x0010_0000;
pub(crate) const MOD_CTRL: u64 = 0x0004_0000;
#[allow(dead_code)]
pub(crate) const MOD_OPTION: u64 = 0x0008_0000;
#[allow(dead_code)]
pub(crate) const MOD_SHIFT: u64 = 0x0002_0000;
pub(crate) const DEFAULT_MODIFIER_ZOOM_MASK: u64 = MOD_CMD | MOD_CTRL;
/// Accessibility Zoom's supported scroll modifiers. Shift is intentionally
/// excluded: Apple exposes it for ordinary keyboard/mouse semantics, not for
/// the scroll-to-zoom setting.
pub(crate) const SUPPORTED_ZOOM_MODIFIER_MASK: u64 = MOD_CMD | MOD_CTRL | MOD_OPTION;

/// Sample Command/Control only at session begin. Once latched, modifier
/// changes cannot switch the event family before the session ends.
#[cfg(any(target_os = "macos", test))]
#[allow(dead_code)]
pub(crate) fn modifier_zoom_session(begin: bool, modifiers: u64, latched: bool) -> bool {
    modifier_zoom_session_with_mask(begin, modifiers, latched, DEFAULT_MODIFIER_ZOOM_MASK)
}

/// Same session latch with a Quartz modifier mask sourced from macOS
/// `HIDScrollZoomModifierMask`. Unknown bits are harmless: the caller may
/// pass the raw mask, and only the intersection with current flags can match.
#[allow(dead_code)]
pub(crate) fn modifier_zoom_session_with_mask(
    begin: bool,
    modifiers: u64,
    latched: bool,
    mask: u64,
) -> bool {
    if begin {
        modifiers & mask != 0
    } else {
        latched
    }
}

/// Apply the optional Shift-scroll compatibility mapping. Native macOS
/// trackpad scrolling does not document Shift as an axis switch, so callers
/// must opt into this legacy convenience explicitly.
#[allow(dead_code)]
pub(crate) fn shift_scroll_delta(
    modifiers: u64,
    dx_mm: f64,
    dy_mm: f64,
    enabled: bool,
) -> (f64, f64) {
    if enabled && modifiers & MOD_SHIFT != 0 && dx_mm.abs() < 1e-4 && dy_mm.abs() > 1e-4 {
        (dy_mm, 0.0)
    } else {
        (dx_mm, dy_mm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_zoom_stays_latched_until_session_end() {
        let latched = modifier_zoom_session(true, MOD_CMD, false);
        assert!(latched);
        assert!(modifier_zoom_session(false, 0, latched));

        let ordinary = modifier_zoom_session(true, 0, false);
        assert!(!ordinary);
        assert!(!modifier_zoom_session(false, MOD_CTRL, ordinary));
    }

    #[test]
    fn modifier_zoom_accepts_option_when_selected() {
        assert!(modifier_zoom_session_with_mask(
            true, MOD_OPTION, false, MOD_OPTION
        ));
        assert!(!modifier_zoom_session_with_mask(
            true, MOD_SHIFT, false, MOD_OPTION
        ));
    }

    #[test]
    fn zoom_mask_excludes_shift_from_accessibility_route() {
        assert_eq!(
            (MOD_SHIFT | MOD_OPTION) & SUPPORTED_ZOOM_MODIFIER_MASK,
            MOD_OPTION
        );
    }

    #[test]
    fn shift_scroll_mapping_is_explicitly_opt_in() {
        assert_eq!(shift_scroll_delta(MOD_SHIFT, 0.0, 4.0, true), (4.0, 0.0));
        assert_eq!(shift_scroll_delta(MOD_SHIFT, 0.0, 4.0, false), (0.0, 4.0));
        assert_eq!(shift_scroll_delta(MOD_SHIFT, 1.0, 4.0, true), (1.0, 4.0));
    }
}
