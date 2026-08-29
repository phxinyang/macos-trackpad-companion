# Trackpad Companion Settings

This is the macOS-native settings client. It runs on macOS 13 or newer; building
the current PermissionFlow dependency requires a Swift 6.2 toolchain (Xcode 26
or newer). It uses the Rust `companion-config` executable for all configuration
I/O. The app
also owns the menu-bar service supervisor, Accessibility guidance, and local
Bonjour advertisement used by phone clients.

The Swift package is deliberately macOS-only. `App.swift` owns the app shell,
service supervisor, and settings composition; shared state lives in
`ServiceSupervisor.swift` owns the helper lifecycle, Accessibility gate,
network-path recovery, login-item state, and Bonjour publishing. Shared state
lives in `AppModels.swift`, the `companion-config` bridge lives in
`SettingsModel.swift`, and the menu-bar quick controls in
`Views/MenuBarView.swift`. Reusable settings rows and overview components are
under `Views/`. This keeps the high-rate Rust input path independent from the
UI and makes it safe to run the settings app on a Mac mini without a physical
trackpad.

From the repository root on a Mac:

```sh
cargo build --release --bin companion-config
cd macos/TrackpadCompanionSettings
COMPANION_CONFIG_BIN="$OLDPWD/target/release/companion-config" swift run
```

For distribution, copy `companion-config` next to the built app executable or
set `COMPANION_CONFIG_BIN` in the app's launch environment. The app follows
Apple's `Point & Click`, `Scroll & Zoom`, and `More Gestures` organization, and
keeps virtual-input-only behavior in `Companion`.

Accessibility setup uses [PermissionFlow](https://github.com/jaywcjlove/PermissionFlow) `v2.11.2`.
Clicking the permission action opens the correct Privacy & Security page and
shows its native floating drag-to-authorize guide. The DMG build includes
PermissionFlow's localized resource bundle so the guide follows the app's
English/Chinese language switch.

The menu bar is the quick-control surface: Web and phone exposure, launch at
login, service recovery, live frame counters, and copy actions are available
there. Detailed gesture tuning remains in the settings window.

The GUI does not write `defaults`, register a fake trackpad, or require the
Trackpad pane to exist in System Settings. It is therefore usable on a Mac mini
without a physical trackpad.

For a user-installable development bundle, run from the repository root:

```sh
./packaging/macos/build-app.sh
open "dist/macos/Trackpad Companion.app"
```
