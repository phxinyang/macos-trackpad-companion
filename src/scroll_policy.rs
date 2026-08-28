//! Cross-platform state policy for modifier-based scroll routing.
//!
//! The macOS emitter owns the actual CGEvent posting, but the decision about
//! whether a scroll session is redirected to magnification is pure state and
//! should remain testable on every host.

pub(crate) const MOD_CMD: u64 = 0x0010_0000;
pub(crate) const MOD_CTRL: u64 = 0x0004_0000;
pub(crate) const DEFAULT_MODIFIER_ZOOM_MASK: u64 = MOD_CMD | MOD_CTRL;

/// Sample Command/Control only at session begin. Once latched, modifier
/// changes cannot switch the event family before the session ends.
#[cfg(any(target_os = "macos", test))]
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
}
