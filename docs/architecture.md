# Architecture

Trackpad Companion is intentionally split into a platform-neutral input core
and thin client shells. The Rust core owns the protocol, gesture state machine,
macOS event output, and configuration schema. Android, browser, TUI, and the
macOS app are clients of that contract.

```text
Android / browser / USB PTP
          |
          v
  ATP1 decoder + gesture engine (Rust)
          |
          +--> public Quartz events (pointer, click, scroll)
          +--> compatibility gesture events (pinch, rotate, Spaces)
          +--> config.toml via companion-config

  TrackpadCompanionSettings (SwiftUI)
          |-- supervises companion-net
          |-- presents native settings and permission guidance
          |-- invokes companion-config for reads/writes
```

## Repository layout

- `src/`: Rust daemon, gesture recognizer, macOS output, network listener, and
  command-line tools.
- `crates/touchpad-proto/`: versioned ATP1 wire-format crate shared by clients.
- `macos/TrackpadCompanionSettings/`: macOS 13+ SwiftUI settings app. The
  package keeps `App.swift` with the app entry point and service/config models, while
  reusable settings rows and overview components live under `Views/`.
- `packaging/macos/`: reproducible app bundle and DMG scripts. Helpers are
  embedded in `Contents/Resources`; no Homebrew dependency is required at
  runtime.
- `android/`: Android client and unit tests.
- `static/`: offline browser client and gesture test page.
- `docs/`: protocol, native-parity research, product plan, and release notes.
- `scripts/` and `tools/`: host diagnostics and deterministic protocol probes.

## Configuration boundary

`companion-config` is the single configuration API. It reads and writes the
same TOML file used by `companion-net` and exposes JSON for the GUI/TUI. Swift
does not parse TOML and does not write macOS `defaults`; this keeps Mac mini
installations (where the Trackpad pane is absent) fully supported and avoids a
second source of truth.

## Runtime ownership

The Swift app owns the user-facing lifecycle: it starts/stops the network
helper, publishes Bonjour metadata, checks Accessibility permission, and shows
diagnostics. The Rust helper owns all high-rate input and event synthesis. A
future login-item helper can reuse the same Rust binary without changing the
gesture protocol.

## Release boundary

Release artifacts are generated only on macOS runners. `build-app.sh` creates
an unsigned `.app` and ZIP for development; `package-dmg.sh` wraps that app in
a drag-to-Applications DMG. A Developer ID can be supplied through
`CODESIGN_IDENTITY` for a private release build; notarization credentials stay
outside the repository and are handled by the release operator.
