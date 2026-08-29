# macOS packaging

Builds run on macOS 13 or newer because the settings client is SwiftUI. The
development bundle keeps the Rust helpers in `Contents/Resources`, where the
app's service supervisor can find them without a Homebrew installation.

The Rust helpers are compiled for the host that runs the build. When the
repository lives on a shared volume, run the packaging script on macOS rather
than reusing `target/release` binaries built on Linux.

```sh
./packaging/macos/build-app.sh
./packaging/macos/package-dmg.sh
```

The scripts create an unsigned `.app`, `.zip`, and `.dmg` under `dist/macos`.
The DMG includes the conventional `Applications` shortcut for drag-and-drop
installation. `package-dmg.sh` removes older local DMGs first, so opening a
wildcard after a rebuild cannot select a stale application.
Set `CODESIGN_IDENTITY='Developer ID Application: ...'` for a hardened runtime
signature. Notarization remains a release-pipeline concern: after signing,
submit the DMG with `xcrun notarytool submit` and staple it with
`xcrun stapler staple`. Credentials stay outside the repository.
