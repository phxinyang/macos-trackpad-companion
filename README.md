# macos-trackpad-companion

Userspace bridge from a PTP (Microsoft Precision Touchpad / Windows
Precision Touchpad) HID device to native macOS gesture events. Reads the
project's canonical 6-byte/contact PTP profile from a matched digitizer, runs
touch frames through a gesture
state machine, and posts CGEvents — cursor, click, phased smooth scroll,
and (via private CGEvent gesture types) pinch, rotate, 3-finger swipe.

Linux and Windows handle PTP devices natively; this companion exists
because macOS has no built-in PTP consumer. macOS does have similar
support for Apple's own Magic Trackpads, but their driver will only
talk to USB devices using Apple's USB VID.

The project is a beta userspace bridge. It is designed to be useful on a real
LAN today, while keeping the boundary honest: CGEvent and synthetic gesture
payloads can be native-like in compatible apps, but they are not the private
MultitouchSupport stream or the pressure sensor in an Apple Force Touch
trackpad.

The Android and browser clients share a product-oriented presentation model:
a high-contrast touch surface, compact connection status, a small glass
control layer, and focused settings/diagnostic panels. The visual and
interaction decisions are documented in
[`docs/apple-product-design-spec.md`](docs/apple-product-design-spec.md).

## Build & run

```sh
cd companion
cargo build --release
./target/release/companion -v
```

### Phone / browser input

`companion-net` is the network sibling of `companion`: it feeds the same
gesture engine from a phone or browser instead of opening a USB HID device.
It binds UDP and TCP on the configured port, serves the touchpad page at
`http://<mac-ip>:4242/`, and accepts the same binary frames over WebSocket.

```sh
cargo build --release --bin companion-net
./target/release/companion-net -v
```

Open the printed URL on a phone connected to the same LAN. The Android app
also discovers a bundled macOS app advertising `_mtc-trackpad._tcp`; open the
connection sheet to select a nearby Mac, or use a `mtc://pair?...` link shared
by the macOS menu bar. Manual host/port/token entry remains available when
multicast DNS is blocked by the network. The Android MVP is under `android/`;
build it from that directory with `./gradlew assembleDebug`. A synthetic sender is available for
diagnostics:

```sh
python3 tools/synthetic_sender.py --host <mac-ip> --mode circle
```

When `[net].token` is configured, append `?token=<token>` to the browser URL,
enter the same value in the Android Token field, or pass
`--token <token>` to `tools/synthetic_sender.py`.

The macOS settings app creates a token automatically on first launch in its
managed configuration and advertises the authentication mode in Bonjour TXT.
Legacy CLI configurations without a token remain compatible; run
`companion-config ensure-token` explicitly when you want to enable pairing
authentication outside the GUI.

The web client exposes the main touch surface at `/` and the instrumented
gesture diagnostics at `/test`. Both pages use the system font stack, semantic
status colors, safe-area insets, keyboard focus rings, and reduced-motion or
reduced-transparency fallbacks. The Android APK keeps the native View stack so
the high-rate touch stream does not depend on a second UI runtime.

Android and browser clients encode coordinates in millimeters using one
isotropic pixel scale. This keeps equal finger motion equally sensitive in X
and Y; use the `A−` / `A＋` controls for overall calibration. The Android
client prefers the panel's reported physical DPI and falls back to
`densityDpi`; browsers use the CSS reference pixel because mobile browsers do
not expose reliable physical DPI.

The Android client also includes a configurable `深按条` overlay. Holding it
for the configured duration sends a standard left-button-down edge to the Mac;
releasing sends the matching button-up edge, so it can drag a window or select
text. Its center position, width, height, hold time, visibility, haptic toggle,
and deep-press haptic strength are stored locally in the APK. On Xiaomi/Redmi
devices that expose the same calibrated hardware-feedback effect as the system
launcher it uses that vendor click; other devices use an immediate short click
followed, after the hold threshold, by a sharper physical-emulation impact. The
system's haptic setting and master switch still cap the resulting output.
This is a software button-hold affordance, not a pressure-level Force Click
event: CGEvent cannot synthesize the private pressure transition used by a real
Force Touch trackpad.

The network listener needs Accessibility permission to post CGEvents, but it
does not need Input Monitoring because it never reads a local HID device.
For a LAN deployment set `[net].token`; without one, any host that can reach
the port can inject pointer or gesture events.

CLI flags (network binding can be overridden for one `companion-net` run):

| Flag | Default | Meaning |
| --- | --- | --- |
| `--config PATH` | XDG default | TOML config path. See **Configuration** below. |
| `-v`, `-vv` | info | Increase log level. Overrides `[log].level` from the file. |
| `--port PORT` | 4242 | `companion-net` UDP + HTTP/WebSocket port override. |
| `--listen-ip IP` | all interfaces | `companion-net` bind-address override. |
| `--token TOKEN` | unset | Bearer token for WebSocket and authenticated UDP clients. |

## Configuration

All tuning lives in a TOML file at
`$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml`, falling back to
`~/.config/macos-trackpad-companion/config.toml` when `XDG_CONFIG_HOME`
is unset. A missing file is fine — defaults take over. Unknown keys are
rejected so typos surface at startup.

```toml
[device]                    # optional — match a specific USB device
# vid = 0x1234              #   (omit either field for any PTP digitizer)
# pid = 0x5678

[net]                       # companion-net UDP + WebSocket listener
# listen_ip = "0.0.0.0"    # bind address (default: all interfaces)
# port      = 4242
# token     = "change-me"   # optional bearer token; see docs/wire-protocol.md

[log]
level = "info"              # error | warn | info | debug | trace
# file  = "~/Library/Logs/macos-trackpad-companion.log"
                            # if set, logs are appended here instead of stderr;
                            # `~/` is expanded and parent dirs are created.

[cursor]
sensitivity   = 28.0        # px per mm of finger motion at accel_ref
accel_exponent = 1.35       # 1.0 = linear; >1 boosts fast flicks
accel_ref     = 70.0        # mm/s — velocity at which sensitivity is the linear feel

[scroll]
sensitivity = 20.0          # px per mm
natural     = true          # finger-down → content-down (macOS default since 10.7)
enable      = true          # two-finger translation emits scroll events
horizontal  = true          # keep horizontal component of a scroll
momentum    = true          # seed momentum-phase coast after a fast lift
# modifier_zoom_mask = 262144  # Control/Option/Command mask override
shift_scroll_horizontal = false # compatibility only; native trackpads keep axis

[macos]
sync_system_settings = true # read System Settings at startup; TOML fields win
haptic_feedback = "auto"    # auto | on | off; auto follows ActuateDetents

# Each gesture has an `enable` key with three forms:
#   enable = "on"                                  # always
#   enable = "off"                                 # never
#   enable = { only   = ["com.apple.Safari"] }     # frontmost-app allowlist
#   enable = { except = ["com.apple.Terminal"] }   # frontmost-app denylist
# `only` and `except` are mutually exclusive. Bundle IDs are matched
# against the app owning the topmost normal window under the cursor,
# sampled at gesture *start* and held for the rest of the touch (so a
# mid-gesture window switch can't kill its own gesture). Under-cursor
# rather than frontmost because that's how macOS itself routes
# pinch/rotate/scroll/click — Mission Control / Spaces 3F/4F swipes
# are system-wide and ignore window targeting, but the same filter
# still expresses "don't fire this gesture when my cursor is parked
# over Terminal."
#
# To learn the bundle ID for an app, any of these work:
#   osascript -e 'id of app "Safari"'                       # by user-facing name
#   mdls -name kMDItemCFBundleIdentifier -r /Applications/Safari.app
#   lsappinfo info -only bundleid -app Safari               # currently running
#   lsappinfo info -only bundleid `lsappinfo front`         # whatever is frontmost now

# These switches mirror the corresponding macOS System Settings when
# `[macos].sync_system_settings = true` (the default). A value written here
# is an explicit override and wins over System Settings. `companion-net` is a
# virtual input surface, so the physical-only `Clicking` preference is ignored
# there and phone tap-to-click remains enabled by default:
[gestures]
tap_to_click = "on"       # Point & Click -> Tap to click
secondary_click = "on"    # two-finger secondary click
smart_zoom = "on"         # two-finger double-tap Smart Zoom
dictionary_lookup = "on"  # three-finger tap Look Up
right_edge_swipe = "on"   # two-finger right-edge Notification Center

[gestures.pinch]
enable = "on"

[gestures.rotate]
enable = "on"

[gestures.swipe.horizontal]   # left/right 3F/4F → Spaces / Full-Screen Apps
enable  = "on"
backend = "synthetic"         # synthetic | notification | off
                              #   (notification is silently `off` on this axis —
                              #    no Dock notification exists for switching spaces)

[gestures.swipe.vertical]     # up/down 3F/4F → Mission Control / App Exposé
enable  = "on"
backend = "synthetic"        # synthetic animates; notification commits on lift

[gestures.three_finger_drag]  # three fingers → left-button drag
enable = "on"                 # "off" restores three-finger swipes
release_delay_ms = 500        # 500 = 500ms 换把悬停延续 (0 = 抬手即松)

[gestures.one_finger_tap_drag] # double-tap, hold, then drag
enable = "on"

[gestures.press_and_hold_drag] # optional accessibility-style drag
enable = "off"                # stationary 1F hold; off matches stock default
```

When a three-finger drag is already holding a window and a fourth finger joins,
the gesture engine keeps the left mouse button held while entering the four-finger
Space-swipe state. macOS 26 rejects third-party DockSwipe events, so the
`synthetic` backend uses the system SymbolicHotKey registry (Space Left/Right,
Mission Control or App Exposé) with a short threshold and animation cooldown.
macOS 27 and later first use SkyLight's `SLEventSetIOHIDEvent` HID payload to
retain the continuous DockSwipe animation; if that private setter is unavailable,
the current gesture falls back to the same SymbolicHotKey path. macOS 25 and
earlier keep the animated legacy payload. Cancelled or broken sessions always
release the held button.

On macOS, the process reads `com.apple.AppleMultitouchTrackpad` at startup,
falls back to `com.apple.driver.AppleBluetoothMultitouch.trackpad` for missing
keys, and reads natural scrolling from `.GlobalPreferences`. This uses the
Core Foundation preferences API rather than spawning `defaults`. Set
`[macos] sync_system_settings = false` to keep the TOML defaults entirely
self-contained. Settings that belong to hardware or WindowServer (pressure
thresholds, palm rejection, Force Touch, five-finger gestures and USB-mouse
device management) are reported as unsupported and are not faked. For
feedback, macOS builds use Apple's `NSHapticFeedbackManager.defaultPerformer()`
for device-aware generic, alignment and level-change cues; this is a haptic
confirmation, not a synthetic Force Touch or pressure event. Devices without
a Taptic Engine silently ignore the cue.
The sync is startup-only; restart the process after changing System Settings.

Keyboard modifiers are preserved on every emitted mouse, scroll, and gesture
event. This lets macOS and the active app apply their normal meanings:

| Combination | Behavior |
| --- | --- |
| Control-click | Secondary click / contextual menu (the macOS definition of right-click) |
| Command-click, Option-click, Shift-click | App-level selection, alternate action, tab, and drag semantics; no custom remapping |
| Control/Option/Command + two-finger scroll | Magnification route when the selected `HIDScrollZoomModifierMask` is active; the mask is latched for the whole scroll session |
| Shift + two-finger scroll | Native mode preserves the original scroll axis. Set `shift_scroll_horizontal = true` only for the compatibility remap to horizontal scrolling |

Apple Accessibility Zoom exposes Control, Option, or Command as the selectable
scroll modifier; Shift is not one of its choices. The companion therefore does
not treat Shift as Zoom and does not convert it to horizontal scrolling unless
the explicit compatibility switch is enabled.

The legacy global `com.apple.trackpad.scaling` and
`com.apple.scrollwheel.scaling` values are converted to bounded compatibility
scalars for `cursor.sensitivity` and `scroll.sensitivity`; Apple does not
publish a stable px/mm transfer function, so these values are not claimed to
be a physical calibration. `com.apple.trackpad.scaling = -1` is represented as
the linear cursor curve (`cursor.accel_exponent = 1.0`). Explicit TOML values
still win.

For the local HID companion, `Clicking = 0` follows the native physical
trackpad behavior. For `companion-net` on a Mac mini or any Mac without an
attached trackpad, stale `Clicking = 0` preferences are ignored because they
do not describe the phone/browser surface. No System Settings change is
needed. To disable virtual tap-to-click explicitly:

```toml
[gestures]
tap_to_click = "off"
```

Missing Trackpad settings are normal on Macs without an internal pad or when
only an external device is present. The process logs the missing domain and
continues with TOML/default values. `DragLock` is reported for diagnostics but
does not change the companion's three-finger re-grip timeout; configure
`[gestures.three_finger_drag].release_delay_ms` directly (the default is
500 ms).

### Mac mini without a Trackpad pane

macOS only exposes the Trackpad section when an internal trackpad or a wireless
trackpad is present. These deep links can open the section when it exists, but
they do not bypass that hardware check:

```sh
# macOS Ventura and later
open "x-apple.systempreferences:com.apple.Trackpad-Settings.extension"

# legacy System Preferences path
open /System/Library/PreferencePanes/Trackpad.prefPane
```

Writing `com.apple.AppleMultitouchTrackpad` with `defaults`, refreshing
`cfprefsd`, or running `activateSettings -u` only changes stored values. It
cannot register a Mac mini as an Apple trackpad, so the native pane is not a
supported configuration surface in that setup. Connect a Magic Trackpad if
the Apple pane itself is required.

For the phone/browser surface, use the bundled TUI instead. It edits every
companion setting that has a defensible application-level equivalent (including
the scroll-to-zoom modifier mask) and shows
Mac preference-domain availability as a read-only diagnostic. Hardware-only
keys remain unsupported and are reported in the companion startup log; the TUI
never writes macOS defaults:

```sh
cargo run --release --bin companion-tui
# or, from a release bundle:
./target/release/companion-tui
```

The repository also includes a native macOS SwiftUI settings app. It follows
the same three Apple sections (`Point & Click`, `Scroll & Zoom`, and `More
Gestures`) and keeps virtual-input-only controls under `Companion`. Its
`Overview` page and menu bar item supervise `companion-net`, show Accessibility
state, and expose a local pairing link. Changes are written immediately through
the shared `companion-config` helper, so the GUI and TUI cannot silently
diverge from `companion-net`'s TOML parser:

```sh
# Build the helper first, then build the GUI on macOS 13+
cargo build --release --bin companion-config
repo="$PWD"
cd macos/TrackpadCompanionSettings
COMPANION_CONFIG_BIN="$repo/target/release/companion-config" swift build -c release
```

For a bundled app, use `./packaging/macos/build-app.sh`; it places both Rust
helpers under `Contents/Resources` and emits a ZIP. SwiftUI is intentionally
kept in a macOS-only package; Linux CI validates the Rust helper and
configuration contract, while final window, dark-mode, permission, Bonjour,
and accessibility checks belong on a Mac host.

The TUI writes the same `config.toml` used by `companion-net`. Save the changes
and restart `companion-net`; explicit TOML values take precedence over any
Mac preference snapshot. On a Mac mini, stale `Clicking=0` values from the
physical-trackpad domain are ignored for virtual phone taps by design.

Three-finger drag is on by default. Three fingers must move past a small
jitter guard before the engine posts `LeftMouseDown`; movement is then
emitted as standard Quartz `LeftMouseDragged` events and the final finger
lift posts `LeftMouseUp`. That reproduces the application-level behavior of
macOS's Three-Finger Drag style. It is not a real Apple multitouch stream:
macOS receives synthesized mouse events, not the original three-finger
contacts.

Set `enable = "off"` to get the stock-macOS arrangement instead, where three
fingers drive Mission Control / Spaces swipes and a stationary three-finger
tap looks a word up. Drag then lives on four fingers only.

`[gestures.one_finger_tap_drag]` (tap twice, keep the second contact down,
drag) is also on by default. The second contact does not press the button on
the frame it lands: if it lifts again within 200 ms without moving, the pair
is dispatched as a double-click instead. Only a contact that moves past the
jitter guard or outlasts that window becomes a drag.

## Permissions

The first run on a fresh macOS install will prompt for two privacy
permissions; without them the companion exits with an actionable error.

- **Input Monitoring** — required to read raw HID input reports from
  the trackpad. macOS surfaces error `0xE00002C5` from `IOHIDManagerOpen`
  if this isn't granted.
- **Accessibility** — required to post synthetic CGEvents (cursor moves,
  clicks, scroll, gestures). Granted via System Settings → Privacy &
  Security → Accessibility.

## Reading the logs

When a two-finger gesture locks (the moment the companion commits to
either scroll or pinch+rotate), an `INFO` line records all three
candidate scores and the geometric inputs that drove the choice:

```
2F lock=pinch+rotate scores[pinch=1.56 rot=1.35 pan=0.42 disq:margin] common=0.42mm diff=1.45mm align=0.62 balance=0.45
```

A score `≥ 1.00` means that signal crossed its lock threshold. Pan is
mutually exclusive with pinch+rotate and only wins if it both crosses
*and* dominates; otherwise the pair locks.

Tags after a score say *why* it didn't compete:

| Tag | Meaning |
| --- | --- |
| `disq:margin` | Pan: centroid translation didn't beat differential motion by 20% — most of the motion is asymmetric, not translational. |
| `disq:participation` | Pan: margin OK, but neither finger balance (slower ≥ 30% of faster) nor alignment (motion vectors near-parallel) qualified. |
| `gated:noise` | Pinch/rot: one finger sat in the 0.3–1.0 mm noise band where differential signal is dominated by jitter; lock deferred. |
| `gated:policy` | Pinch/rot: the under-cursor app's `enable` policy blocked this gesture, so the score was zeroed for selection. |

Trailing fields:

- `common` — magnitude of the shared (centroid) translation in mm.
- `diff` — magnitude of the per-finger differential motion in mm.
- `align` — cosine of the angle between the two fingers' motion vectors. ~1.0 = parallel, 0 = perpendicular, <0 = anti-parallel.
- `balance` — slower finger's motion / faster finger's motion. 1.0 = symmetric, 0 = one finger anchored.

For the contrasting case, `2F lock=scroll` uses the same format with
`pan` first.

## Wire-format contract

The companion parses the device's HID report descriptor at runtime, so
firmware is free to choose VID/PID, contact count, and physical/logical
coordinate scale. To remain compatible:

- Expose a Digitizer Application Collection at usage page `0x0D`,
  usage `0x05` (Touch Pad).
- Inside it, declare N nested Logical collections of usage page `0x0D`,
  usage `0x22` (Finger). Each finger collection must input these fields,
  in this order, with these sizes:
  - Confidence — Digitizer 0x47 — 1 bit
  - Tip Switch — Digitizer 0x42 — 1 bit
  - 6 bits padding (so the contact-id falls on a byte boundary)
  - Contact Identifier — Digitizer 0x51 — 8 bits
  - X — Generic Desktop 0x30 — 16 bits. Set Logical Max to your
    coordinate space *and* Physical Max + Unit + Unit Exponent so the
    companion can derive mm/pixel. SI Linear cm (Unit `0x11`) and
    English Linear inches (Unit `0x13`) are both supported. Without
    physical units, descriptor parse fails (gesture thresholds and
    cursor sensitivity are expressed in mm).
  - Y — Generic Desktop 0x31 — 16 bits, same
- After the finger collections, declare:
  - Scan Time — Digitizer 0x56 — 16 bits (100 µs ticks per spec)
  - Contact Count — Digitizer 0x54 — 8 bits
  - Button 1 — Button 0x01 — 1 bit (then 7 bits padding)

The reference firmware therefore produces a **6-byte-per-contact** layout.
The decoder also preserves descriptor bit offsets and widths, so compatible
bit-packed contact fields can be decoded when the descriptor describes them;
the HID path now reassembles supported hybrid report fragments by Scan Time;
the network protocol remains one complete frame per message.

The Microsoft "PTPHQA" feature report is needed for Windows certification
but ignored by macOS, so it's optional from the companion's perspective.

The HID decoder is descriptor-driven for contact field offsets and widths,
including non-byte-aligned fields. The project reference remains the 6-byte
profile above; broader real-device descriptor coverage and macOS validation
are tracked in the execution plan's Phase C.

## Reference firmware

A working PTP firmware lives at commit `7f3ee1c:firmware/src/main.rs` in
this repo. It produces a composite USB device:

- Interface 0 — boot Mouse (gives macOS a working cursor before the
  companion is running, and is a sane fallback everywhere)
- Interface 1 — PTP digitizer (5 contacts, 65×40 mm, logical 3936×2424,
  PTPHQA blob, all four feature reports)

The companion's unit test
`descriptor::tests::parses_wpt_descriptor` reproduces the bytes that
7f3ee1c emits. That descriptor is the canonical "this works" reference.

Don't put the Mouse collection in the *same* HID interface as the
digitizer — macOS can route by primary usage and bind a different driver
that intercepts cursor before the digitizer becomes visible. Keep them
on separate interfaces.

## Module map

| File | Responsibility |
| --- | --- |
| `descriptor.rs` | Walks a HID report descriptor and extracts the touch-report `Layout` (contact count, X/Y max, field offsets). |
| `report.rs` | Decodes one input-report buffer into a `Frame` of normalized contacts. |
| `gesture.rs` | Pure state machine — classifies 1F/2F/3F/4F gestures, locks 2F mode on first significant motion. Tested without I/O. |
| `output.rs` | macOS event synthesis. Public CGEvent for cursor/click/scroll, private CGEvent type/field IDs for pinch/rotate/swipe. |
| `hid.rs` | IOHIDManager FFI: device matching, descriptor + input-report subscription, run-loop pumping. |
| `net.rs` | UDP + WebSocket ingestion, strict frame decoding, sequence/loss filtering, and idle-lift recovery. |
| `crates/touchpad-proto` | Shared ATP1 wire-format encoder/decoder used by network clients. |
| `main.rs` | CLI parsing, logging, wiring. |
| `src/bin/companion_tui.rs` | Native-shaped terminal configuration editor for Mac mini/virtual-input deployments; writes only `config.toml`. |
| `src/bin/companion_config.rs` | JSON/TOML bridge used by the macOS SwiftUI settings app; preserves unrelated TOML fields and writes atomically. |
| `macos/TrackpadCompanionSettings` | macOS 13+ SwiftUI settings app with native three-section navigation, Chinese/English switching, and Companion extensions. |

## Caveats

### Native-event boundary

The network clients send ATP1 contact frames to the same gesture engine, but
they do not register as an Apple trackpad or feed `MultitouchSupport`. The
result is intentionally mixed:

| Gesture | Event path | Native status |
| --- | --- | --- |
| Cursor, click, three-finger drag | Public Quartz mouse events | Native app-visible mouse behavior; not native multitouch input |
| Two-finger scroll | Public phased pixel-scroll events plus our own inertia timer | App-compatible and momentum-aware; not the Apple PTP stream |
| Pinch, rotate | Reverse-engineered private gesture CGEvent payload | Works in some AppKit apps; undocumented and macOS-version-sensitive |
| Three-/four-finger Spaces and Mission Control | Private DockSwipe CGEvent or Dock notification | Uses the Dock gesture path, but is still synthesized |

Receiving the same raw events as an Apple trackpad would require a virtual HID
device accepted by macOS's trackpad stack, not just `CGEventPost`. That is a
separate driver-level project and is outside this userspace bridge.

- **Private CGEvent gesture types are reverse-engineered.** Pinch,
  rotate, and swipe injection use undocumented CGEvent types (18, 19,
  20, 30, 31) and field IDs (110, 113, 115, 132). Some of these shapes have
  worked across recent macOS releases and are used by tools such as
  BetterTouchTool and Karabiner-Elements, but they are not in any public
  Apple header and have already changed for DockSwipe on macOS 27. Use the
  per-gesture `enable`
  policies in the configuration to disable them; there is no
  `--no-private-gestures` CLI flag. Cursor / click / phased scroll all use
  public CGEvent APIs and won't be affected.
- **Two-finger ambiguity is resolved by first-significant-motion lock.**
  Once the centroid moves, the distance changes by 4%, or the angle
  changes by 6°, that mode wins for the duration of the touch. The
  thresholds in `gesture.rs` may need tuning once we have hardware.
