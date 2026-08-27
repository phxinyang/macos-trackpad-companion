# Engine feature backlog

Gaps between companion-net synthesis and Apple-trackpad driver-layer
behaviors, discovered during M4 phone testing (2026-08-27). Each needs
work inside `gesture.rs` (or output.rs) — the net path delivers frames
with the exact same fidelity as HID, so anything listed here applies to
both transports equally.

| Feature | Behavior to replicate | Status |
|---|---|---|
| 三指拖移 (3-finger drag) | Resting 3 fingers then moving drags windows / selects text — left-button-held + motion; survives async lifts (3→2→1), releases immediately when the pad empties | **Implemented v1** (`GestureKind::ThreeFingerDrag`, `[gestures.three_finger_drag] enable`); follows Apple's Three-Finger Drag release semantics; a positive internal `release_delay_ms` is reserved for explicit Drag Lock compatibility |
| Tap-drag (轻点两下拖动) | Double-tap-and-hold then move selects text continuously until finger lifts | Backlog — natural extension of the drag state machine onto the tap path |
| Settings sync | Read macOS `com.apple.AppleMultitouchTrackpad` prefs (Clicking / TrackpadScrollNatural / Dragging) so user-facing System Settings drive companion behavior ("设置即所得") without duplicate config | Backlog v1.1 — verify exact key semantics empirically first (`defaults read`) |
| Palm-edge suppression | Contact density near sensor edge ignored | N/A over network — phone clients own rejection (web/app trust-flag only) |

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
