# Trackpad Companion Settings

This is the macOS-native settings client. It requires macOS 13 or newer and
uses the Rust `companion-config` executable for all configuration I/O. The app
also owns the menu-bar service supervisor, Accessibility guidance, and local
Bonjour advertisement used by phone clients.

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

The GUI does not write `defaults`, register a fake trackpad, or require the
Trackpad pane to exist in System Settings. It is therefore usable on a Mac mini
without a physical trackpad.

For a user-installable development bundle, run from the repository root:

```sh
./packaging/macos/build-app.sh
open "dist/macos/Trackpad Companion.app"
```
