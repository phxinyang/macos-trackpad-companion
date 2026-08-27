# Engine feature backlog

Gaps between companion-net synthesis and Apple-trackpad driver-layer
behaviors, discovered during M4 phone testing (2026-08-27). Each needs
work inside `gesture.rs` (or output.rs) — the net path delivers frames
with the exact same fidelity as HID, so anything listed here applies to
both transports equally.

| Feature | Behavior to replicate | Status |
|---|---|---|
| 三指拖移 (3-finger drag) | Resting 3 fingers then moving drags windows / selects text — left-button-held + motion; survives async lifts (3→2→1), releases immediately when the pad empties | **Implemented, on by default** (`GestureKind::ThreeFingerDrag`, `[gestures.three_finger_drag] enable`, set `"off"` for stock-macOS three-finger swipes). `release_delay_ms` defaults to 0, matching Apple's Three-Finger Drag; a positive value selects the separate Drag Lock behavior |
| Tap-drag (轻点两下拖动) | Double-tap-and-hold then move selects text continuously until finger lifts | **Implemented, on by default** (`[gestures.one_finger_tap_drag]`). The second contact does not press on its landing frame: lifting within `TAP_DRAG_CONFIRM` (200 ms) without moving dispatches the pair as a double-click, and only motion past `DRAG_ENGAGE_MM` or outlasting that window commits to a drag |
| Link-fault handling | A silent client is not a lift | **Implemented** — `State::cancel_touch` ends the gesture with every tap path suppressed. Clients heartbeat resting contacts (Android 16 ms, browser per animation frame) so a held finger never looks like a disconnect |
| Settings sync | Read macOS `com.apple.AppleMultitouchTrackpad` prefs (Clicking / TrackpadScrollNatural / Dragging) so user-facing System Settings drive companion behavior ("设置即所得") without duplicate config | Backlog v1.1 — verify exact key semantics empirically first (`defaults read`) |
| Palm-edge suppression | Contact density near sensor edge ignored | N/A over network — phone clients own rejection (web/app trust-flag only) |
| Browser history swipe | Two-finger swipe navigating back/forward inside Safari or Chrome | **Not achievable via CGEvent.** Chromium's `HistorySwiper` requires real `NSTouch` data and Safari behaves the same; synthetic phased scrolls are rejected by both regardless of phase/`ScrollCount`/`mayBegin` shaping. Probe write-up: <https://github.com/aislopware/slop-desk/blob/dc64b6fa/docs/05-input-window-control.md>. Anything beyond plain scroll has to be translated to a key equivalent (⌘[ / ⌘]) instead |

## Reference material for a possible BLE Magic-Trackpad-emulation easter egg

Not on the critical path (the WiFi/UDP + engine route already yields
native-quality events), but if we ever want the phone to impersonate an
Apple trackpad at the HID layer:

- Linux kernel [hid-magicmouse.c](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-magicmouse.c)
  documents Apple's VIDs/PIDs and the full report descriptors /
  multitouch report formats for Magic Trackpads.
- Classic-BT SDP vendor fields are stack-controlled on Android
  (`BluetoothHidDeviceAppSdpSettings` exposes no VID override) — the
  workable variant would be **BLE HOGP with a custom GATT Device
  Information service carrying Apple's PnP VID**, which app-side GATT
  servers *can* set freely. Feasibility on MIUI peripheral mode +
  macOS consumer depth for non-apple digitizers = unproven experiments.
