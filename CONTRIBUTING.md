# Contributing

Trackpad Companion has three product surfaces: the Rust gesture engine,
the Android client, and the native macOS settings app. Keep protocol and
gesture behavior in Rust so all clients share one implementation. The SwiftUI
client should call the `companion-config` helper instead of parsing or writing
TOML itself.

Before opening a pull request:

- run `cargo test --workspace`;
- run `cargo fmt --all -- --check`;
- run `cargo clippy --workspace --all-targets -- -D warnings` when the local
  toolchain supports it;
- on macOS, run `./packaging/macos/build-app.sh` and
  `./packaging/macos/package-dmg.sh`;
- do not commit `target/`, Android build output, SwiftPM build output, local
  diagnostics, signing credentials, or generated DMGs.

Changes that affect gesture timing should include a deterministic Rust test or
a note in `docs/native-parity-matrix.md` describing the real-device evidence.
