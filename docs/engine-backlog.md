# Engine feature backlog

Gaps between companion-net synthesis and Apple-trackpad driver-layer
behaviors, discovered during M4 phone testing (2026-08-27). Each needs
work inside `gesture.rs` (or output.rs) — the net path delivers frames
with the exact same fidelity as HID, so anything listed here applies to
both transports equally.

| Feature | Behavior to replicate | Status |
|---|---|---|
| 三指拖移 (3-finger drag) | Resting 3 fingers then moving drags windows / selects text — left-button-held + motion; survives async lifts (3→2→1), releases when the pad is empty and the configured drag-lock deadline expires | **Implemented, on by default** (`GestureKind::ThreeFingerDrag`, `[gestures.three_finger_drag] enable`). A fourth finger transitions to the four-finger swipe state without releasing the held button, allowing cross-Space window dragging. `release_delay_ms` defaults to **500 ms** in this project; set it to `0` for immediate release. During the deadline, transient 1F/2F re-grip frames are held as contact acquisition; the deadline heartbeat is the failsafe. This is synthesized mouse behavior, not Apple's raw drag-lock stream |
| Tap-drag (轻点两下拖动) | Double-tap-and-hold then move selects text continuously until finger lifts | **Implemented, on by default** (`[gestures.one_finger_tap_drag]`). The second contact does not press on its landing frame: lifting within `TAP_DRAG_CONFIRM` (200 ms) without moving dispatches the pair as a double-click, and only motion past `DRAG_ENGAGE_MM` or outlasting that window commits to a drag |
| 启动台 (Launchpad) | 4-finger radial pinch-in ($R/R_0 \le 0.72$) | **Code path implemented; private/discrete command, macOS behavior pending** — triggers `CoreDockSendNotification("com.apple.launchpad.toggle")` + SkyLight HotKey 160 |
| 显示桌面 (Show Desktop) | 4-finger radial spread-out ($R/R_0 \ge 1.28$) | **Code path implemented; private/discrete command, macOS behavior pending** — triggers `CoreDockSendNotification("com.apple.showdesktop.awake")` + SkyLight HotKey 36 |
| 通知中心 (Notification Center) | 2-finger swipe in from right edge ($x \ge 28\text{mm}, \Delta x \le -3.8\text{mm}$) | **Code path implemented; private hotkey, macOS behavior pending** — real-time trigger during swipe via SkyLight HotKey 163 and ControlCenter clock anchor |
| 单指长按拖拽 (Press-and-Hold Drag) | 1-finger stationary hold $\ge 450\text{ms}$ latches left mouse button for dragging | **Implemented, opt-in** (`[gestures.press_and_hold_drag] enable = "on"`) — default is `off` to preserve ordinary macOS tap behavior |
| Link-fault handling | A silent client is not a lift | **Implemented** — `State::cancel_touch` ends the gesture with every tap path suppressed. Clients heartbeat resting contacts (Android 16 ms, browser per animation frame) so a held finger never looks like a disconnect |
| Multi-client session isolation | A late packet from another UDP/WebSocket source must not continue the active touch stream | **Implemented** — frames carry `PeerId`; a source switch cancels the old stream, resets scan-time alignment, and quarantines the old endpoint for 600 ms |
| Network authentication | A LAN peer cannot inject events when an operator configures a token | **Implemented** — WebSocket bearer/query auth and `ATK1`-wrapped UDP; the envelope authenticates but does not encrypt |
| Settings sync | Read macOS `com.apple.AppleMultitouchTrackpad` prefs with Core Foundation, normalize supported keys, and merge them below explicit TOML overrides | **Implemented startup snapshot** (`src/macos_preferences.rs`); unsupported hardware/WindowServer keys are logged, and changing System Settings requires restart |
| Palm-edge suppression | Contact density near sensor edge ignored | N/A over network — phone clients own rejection (web/app trust-flag only) |
| Browser history swipe | Two-finger swipe navigating back/forward inside Safari or Chrome | **Not achievable via CGEvent.** Chromium's `HistorySwiper` requires real `NSTouch` data and Safari behaves the same; synthetic phased scrolls are rejected by both regardless of phase/`ScrollCount`/`mayBegin` shaping. Probe write-up: <https://github.com/aislopware/slop-desk/blob/dc64b6fa/docs/05-input-window-control.md>. Anything beyond plain scroll has to be translated to a key equivalent (⌘[ / ⌘]) instead |

| 三指拖拽 + 四指切 Space | Hold a window with three fingers, add a fourth finger, switch Space, then release | **Implemented state transition; macOS 26 keeps DockSwipe, macOS 27+ prefers HIDEvent and falls back to SymbolicHotKey** — left button remains held through the switch, horizontal threshold 10mm, vertical threshold 7mm, 350ms cooldown. Requires macOS application/WindowServer verification |

## Verification foundation (2026-08)

The gesture engine now has a platform-neutral output contract. Apple-only
dependencies and emitters are target-gated, while Linux builds use
`src/output_portable.rs`; this makes the 104 portable unit tests runnable before
an Apple host is available. The tests cover tap, split-lift, pan, pinch/rotate
classification, drag, four-finger centroid re-anchoring, sequence handling,
and link-timeout cancellation.

The remaining parity work is tracked in
[`native-parity-matrix.md`](native-parity-matrix.md). In particular, DockSwipe
payload attachment on macOS 27 and preference synchronization require native
macOS captures; they are deliberately not marked complete based on Linux
compilation alone.

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
