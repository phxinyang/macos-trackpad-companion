# Configuration reference

Trackpad Companion loads one TOML file for the HID daemon, `companion-net`,
the TUI, and the macOS settings app. The default path is
`$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml`, falling back to
`~/.config/macos-trackpad-companion/config.toml`.

The file may be absent. Defaults are used. Unknown keys are rejected. GUI and
TUI edits go through `companion-config`; writes are atomic and preserve fields
that the editor did not change.

## Complete example

```toml
[device]
# vid = 0x1234
# pid = 0x5678

[net]
# listen_ip = "0.0.0.0"
port = 4242
# token = "replace-with-a-random-token"

[log]
level = "info" # error | warn | info | debug | trace
# file = "~/Library/Logs/macos-trackpad-companion.log"

[cursor]
sensitivity = 28.0
accel_exponent = 1.35
accel_ref = 70.0

[scroll]
sensitivity = 20.0
natural = true
enable = true
horizontal = true
momentum = true
# modifier_zoom_mask = 262144
shift_scroll_horizontal = false

[macos]
sync_system_settings = true
haptic_feedback = "auto" # auto | on | off

[gestures]
tap_to_click = "on"
secondary_click = "on"
smart_zoom = "on"
dictionary_lookup = "on"
right_edge_swipe = "on"
dynamic_transform_compat = false
parameter_profile = "native" # native | chromium_os
surface_width_mm = 65.0

[gestures.pinch]
enable = "on"
gain = 1.0

[gestures.rotate]
enable = "on"
gain = 1.0

[gestures.swipe.horizontal]
enable = "on"
backend = "synthetic" # synthetic | notification | off

[gestures.swipe.vertical]
enable = "on"
backend = "synthetic"

[gestures.three_finger_drag]
enable = "on"
release_delay_ms = 500
persistent_drag_lock = true

[gestures.one_finger_tap_drag]
enable = "on"

[gestures.press_and_hold_drag]
enable = "off"

[overlay]
enable = false
duration_ms = 600
```

## Gesture enable policies

Every gesture `enable` field accepts one of these forms:

```toml
enable = "on"
enable = "off"
enable = { only = ["com.apple.Safari"] }
enable = { except = ["com.apple.Terminal"] }
```

`only` and `except` cannot be used together. Bundle IDs are sampled when a
gesture starts and held until that touch session ends. Pinch, rotate, scroll,
and click use the application under the pointer. Spaces and Mission Control
are system-wide, but the same policy still lets you avoid firing them while
the pointer is over a selected app.

## Parameter notes

- `cursor.sensitivity` is pixels per millimetre at `cursor.accel_ref`.
- `cursor.accel_exponent = 1.0` is linear. Higher values boost fast motion.
- `scroll.natural` follows the macOS natural-scroll direction.
- `scroll.modifier_zoom_mask` selects the Control, Option, or Command route
  used for scroll-to-zoom compatibility. Shift is not an Apple Accessibility
  Zoom modifier.
- `gestures.pinch.gain` and `gestures.rotate.gain` are Companion-only response
  multipliers. Apple does not expose a public numeric transform sensitivity.
- `parameter_profile = "chromium_os"` enables a comparison profile based on
  public ChromiumOS recognizer thresholds. It is not an Apple calibration.
- `gestures.three_finger_drag.persistent_drag_lock` enables the staged
  `3F -> 0F -> 4F -> 0F -> 3F` handoff. Set it to `false` for a finite
  `release_delay_ms` grace period, or use `release_delay_ms = 0` for strict
  lift-to-release behavior.

## macOS preference sync

When `macos.sync_system_settings = true`, macOS builds read available values
from `com.apple.AppleMultitouchTrackpad`, fall back to
`com.apple.driver.AppleBluetoothMultitouch.trackpad`, and read natural
scrolling from `.GlobalPreferences`. Explicit TOML values win.

On a Mac mini, missing Trackpad domains are normal. For `companion-net`, a
stale physical `Clicking = 0` value is ignored so phone tap-to-click remains
available. Hardware-only values such as Force Touch pressure, palm rejection,
and five-finger hardware gestures are reported as unsupported instead of being
faked.

## CLI helpers

```sh
companion-config dump
companion-config set --path gestures.pinch.gain --value 1.1
companion-config doctor
companion-config ensure-token
```

Restart the daemon after changing configuration. System preference sync is
startup-only as well.
