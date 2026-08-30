# Trackpad Companion

[简体中文](README.zh-CN.md) | English

Trackpad Companion turns a phone, browser, or compatible PTP (Precision
Touchpad) device into a touch surface for macOS. The Rust engine recognizes
pointer movement, clicks, scrolling, pinch, rotation, and 3/4-finger gestures,
then emits macOS events that applications can consume.

This is a beta userspace bridge. It does not register as Apple's private
MultitouchSupport device and it cannot synthesize Force Touch pressure levels.
Public Quartz events are broadly compatible; private gesture events remain
macOS-version and application dependent.

## What you get

| Surface | Use it for | Permission |
| --- | --- | --- |
| macOS SwiftUI app | Settings, service control, pairing, diagnostics | Accessibility |
| Android client | Full touch surface with haptics and deep-press bar | Network |
| Browser client | No-install touch surface at `http://<mac>:4242/` | Network |
| USB PTP daemon | A compatible HID touch device | Input Monitoring + Accessibility |
| TUI and CLI | SSH, Mac mini, automation, recovery | None for config; daemon still needs its normal permission |

The macOS app and TUI use the same `companion-config` helper. There is one
configuration model and one gesture engine, so changing a value in one client
does not create a second incompatible behavior.

## Choose a setup

### Mac mini, no physical trackpad

Install the macOS app or run `companion-net`, then use the Android or browser
client. macOS intentionally hides Apple's Trackpad pane when no internal or
wireless trackpad is present. Writing `com.apple.AppleMultitouchTrackpad` with
`defaults` does not register virtual hardware and does not make that pane
appear. Trackpad Companion keeps its own settings, so no system workaround is
needed.

### Mac with a USB PTP device

Run `companion` to open the HID digitizer directly. The device must expose a
Digitizer Touch Pad collection with descriptor-defined contact fields. The
decoder reads the descriptor at runtime, including supported bit-packed
layouts; the reference profile is six bytes per contact.

### Browser only

Run `companion-net`, open the printed URL on a device on the same LAN, and use
the touch surface. This is the fastest way to test the gesture engine without
installing Android software.

## Install on macOS

Download the DMG from a GitHub Release, open it, and drag `Trackpad Companion`
to `Applications`. On first launch, open Overview > Permissions and click the
Accessibility action; PermissionFlow opens the correct Privacy & Security page
and guides you through dragging the app into the authorization list. The app
runs on macOS 13 or newer and includes the Rust network and configuration
helpers inside the application bundle.

Unsigned development packages are produced when release signing is not
configured. A trusted distribution build must be signed with Developer ID and
notarized before sharing it outside a development machine.

## Build from source

The Rust binaries are host-specific. Build them on the Mac that will run them;
do not copy an ELF binary built on Linux to macOS.

```sh
git clone https://github.com/scottlamb/macos-trackpad-companion.git
cd macos-trackpad-companion
cargo build --release
```

Run the HID daemon:

```sh
./target/release/companion -v
```

Run the network daemon:

```sh
./target/release/companion-net -v
```

Build the native settings app and DMG on macOS:

The current PermissionFlow package requires Swift 6.2 (Xcode 26 or newer) to
build the macOS settings target.

```sh
./packaging/macos/build-app.sh
./packaging/macos/package-dmg.sh
open dist/macos/Trackpad-Companion-*-macos.dmg
```

The app bundle contains `companion-net` and `companion-config` under
`Contents/Resources`, so a user does not need a separate Homebrew install.
The menu bar provides Web/phone switches, launch-at-login, service recovery,
live frame counters, and copy actions without reopening the full settings window.
It also watches the active network interface, rebinds after Wi-Fi/Ethernet
changes, retries one unexpected helper exit, and keeps the last local endpoint
for troubleshooting without storing the pairing token in macOS preferences.
Quitting the app synchronously stops the embedded `companion-net` helper and
waits for its instance lock to be released before exit, so an immediate relaunch
does not collide with the old service.

## Connect a phone or browser

The macOS app's **Connections** page has two independent switches:

- **Web access** exposes the browser page and WebSocket on TCP.
- **Phone access** exposes the Android/native client on UDP and publishes the
  Bonjour pairing service.

Both are enabled by default for backwards compatibility. Turning one off
stops its socket entirely and leaves the other connection path untouched. If
both are off, the helper exits without opening a port. The Android app reads
the capability flags from Bonjour/`mtc://pair` and can connect to a UDP-only
Mac without requiring a TCP health probe.

`companion-net` accepts ATP1 frames over the enabled transports. The Android
app discovers `_mtc-trackpad._tcp` through Bonjour when the network allows
multicast DNS. You can also paste a manual address or an `mtc://pair?...` link
from the macOS app.

Recommended flow: on the macOS **Connections** page, enable only the entry you
need. With **Phone access** enabled, choose **Scan QR code** in the Android
connection sheet; the local address, port, and token are filled in and the app
connects immediately. The QR payload stays on the LAN and never goes through a
cloud service. The scanner uses a barcode model bundled in the APK, so it does
not depend on a deferred Google Play scanner module; the first use only needs
camera permission. If the camera is unavailable, scroll to **IP connection
(backup)** and enter the Mac's LAN address, port, and pairing token. Both
devices should be on the same Wi-Fi. With only **Web access** enabled, copy the
Web address shown by the macOS app into a browser.

```sh
./target/release/companion-net --port 4242 -v
```

For a quick protocol check without a phone:

```sh
python3 tools/synthetic_sender.py --host 127.0.0.1 --mode circle
python3 tools/ws_probe.py --host 127.0.0.1 --mode scroll
# Guided, one-gesture-at-a-time visual verification on the Mac
python3 tools/gesture_probe.py
```

For an authenticated UDP listener, pass the same `--token` to
`synthetic_sender.py` or `gesture_probe.py`.

The Android and browser clients use millimetres as their coordinate unit and a
single isotropic scale. The browser maps the surface to the configured 65 mm
virtual width. Android prefers the panel's physical DPI and falls back to the
device density.

## Security and permissions

The network listener can inject pointer and gesture events into the Mac. Use a
token on every untrusted LAN and never expose the listener to the public
internet:

```sh
./target/release/companion-config ensure-token
./target/release/companion-config dump
```

The macOS app creates a token for its managed configuration on first launch.
The browser sends it as `?token=...`; WebSocket clients may use a bearer
header; UDP clients wrap ATP1 in the documented ATK1 envelope.

When `token` is absent or empty and `listen_ip` is omitted, `companion-net`
binds only to `127.0.0.1`. It refuses an explicit non-loopback `listen_ip`
without a token. When a token is configured and `listen_ip` is omitted, the
default remains `0.0.0.0` so authenticated LAN clients can connect.

Permission requirements depend on the input path:

- `companion-net` needs Accessibility to post synthetic events. It does not
  need Input Monitoring because it reads no local HID device.
- `companion` needs Input Monitoring to read raw HID reports and Accessibility
  to post synthetic events.
- Android and browser clients need local network access only.

Pairing links contain the network token. Treat them as secrets and redact
tokens, host names, full paths, and raw diagnostic logs before sharing an issue.
See [SECURITY.md](SECURITY.md) for the reporting policy.

## Configuration

The default file is:

```text
$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml
```

If `XDG_CONFIG_HOME` is unset, the fallback is
`~/.config/macos-trackpad-companion/config.toml`. A missing file is valid and
uses defaults. Unknown keys are rejected.

Use the GUI or TUI for interactive editing. For scripts, use the JSON helper:

```sh
./target/release/companion-config dump
./target/release/companion-config set \
  --path cursor.sensitivity --value 28
./target/release/companion-config doctor

# Disable one transport without opening the GUI
./target/release/companion-config set --path net.web_enabled --value false
./target/release/companion-config set --path net.phone_enabled --value true
```

A compact configuration example:

```toml
[net]
port = 4242
web_enabled = true
phone_enabled = true
# listen_ip = "192.168.1.20" # requires a non-empty token
# token = "replace-with-a-random-token"

[cursor]
sensitivity = 28.0
accel_exponent = 1.35
accel_ref = 70.0

[scroll]
sensitivity = 20.0
natural = true
horizontal = true
momentum = true

[macos]
sync_system_settings = true
haptic_feedback = "auto" # auto | on | off

[gestures]
tap_to_click = "on"
secondary_click = "on"
smart_zoom = "on"
dictionary_lookup = "on"
right_edge_swipe = "on"
parameter_profile = "native" # native | chromium_os
surface_width_mm = 65.0

[gestures.pinch]
enable = "on"
gain = 1.0

[gestures.rotate]
enable = "on"
gain = 1.0

[gestures.three_finger_drag]
enable = "on"
persistent_drag_lock = true
release_delay_ms = 500
```

See [docs/configuration.md](docs/configuration.md) for the complete schema,
per-app policies, swipe backends, and all defaults.

## Gesture behavior

- **1 Finger**:
  - Point & Move: Linear/accelerated cursor motion.
  - Tap to Click / Double Click / Tap-and-Drag / Press-and-Hold Drag.
- **2 Fingers**:
  - Smooth Scrolling: Phased 2D scroll with natural/inverted direction, momentum inertia, and Shift-scroll compatibility remap.
  - Pinch to Zoom & Two-Finger Rotate (AppKit/Safari compatible).
  - Right-Edge Swipe: Swipe inward from right edge to toggle macOS Notification Center.
  - Smart Zoom: Double-tap with two fingers to magnify.
- **3 Fingers**:
  - Three-Finger Drag: Left-button drag with jitter filtering and optional `persistent_drag_lock` (carry a drag across Space switches).
  - Three-Finger Tap: Dictionary lookup and data detectors.
- **4 Fingers**:
  - Swipe Up: Mission Control (调度中心).
  - Swipe Down: App Exposé (应用程序窗口).
  - Swipe Left / Right: Switch between Fullscreen Spaces and Desktops.
  - Radial Pinch In: Launchpad (启动台).
  - Radial Spread Out: Show Desktop (显示桌面).
- **Modifier Keys**:
  - Control, Option, Command, and Shift modifiers are preserved on app-facing mouse, scroll, pinch, rotate, and drag events. System shortcuts use only the registered chord when additional modifiers would make WindowServer reject it.

The native macOS settings page exposes gesture enable switches, but Apple does
not publish a pinch or rotation sensitivity preference. `gestures.pinch.gain`
and `gestures.rotate.gain` are explicit Companion compatibility controls, not
claimed Apple calibration values. The `chromium_os` profile is an experiment
based on a public recognizer, not a macOS setting.

## Native boundary

Trackpad Companion is explicit about where it can match macOS and where it
cannot:

| Gesture | Implementation | Compatibility |
| --- | --- | --- |
| Pointer, click, drag | Public Quartz mouse events | Broad application support |
| Scroll | Public phased scroll plus optional inertia | App-compatible, not Apple's private stream |
| Pinch, rotate | Reverse-engineered private CGEvent fields | Works in some apps and OS versions |
| Spaces, Mission Control | Private Dock/System shortcut paths | Version-sensitive and synthesized |
| Force Click pressure | Not available through public CGEvent | No pressure-level emulation |

The complete research record is in
[docs/reverse-engineering-sources.md](docs/reverse-engineering-sources.md),
including captured fields, open-source comparisons, and the limits of the
available evidence.

## Diagnostics and development

```sh
./target/release/companion-config doctor
./target/release/companion-tui
./scripts/diagnose-mac.sh
```

Run the test suite before submitting changes:

```sh
cargo test --workspace
cargo check --all-targets
```

macOS packaging is validated in GitHub Actions on a macOS runner. Pushing a
tag such as `v0.2.0` runs `.github/workflows/release-macos.yml` and publishes
the generated ZIP and DMG as release assets. Signing and notarization stay
outside the repository.

## Repository map

| Directory | Responsibility |
| --- | --- |
| `src/` | Rust daemon, network listener, gesture engine, and macOS output |
| `crates/touchpad-proto/` | Shared ATP1 encoder and decoder |
| `macos/TrackpadCompanionSettings/` | Native SwiftUI settings app |
| `android/` | Android touch client and tests |
| `static/` | Browser touch surface and gesture test page |
| `packaging/macos/` | App bundle and DMG scripts |
| `docs/` | Architecture, configuration, protocol, research, and plans |
| `tools/` | Deterministic senders and protocol probes |

See [docs/architecture.md](docs/architecture.md) for runtime ownership and
[CONTRIBUTING.md](CONTRIBUTING.md) for the development checks.

## License

MIT. See [LICENSE](LICENSE). Third-party research sources and asset origins
are recorded in the relevant documents under `docs/` and `static/assets/`.
The macOS settings app also includes [PermissionFlow](https://github.com/jaywcjlove/PermissionFlow)
via SwiftPM under its [MIT license](https://github.com/jaywcjlove/PermissionFlow/blob/v2.11.2/LICENSE).
