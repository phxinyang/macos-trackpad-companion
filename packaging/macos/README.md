# macOS packaging

Builds run on macOS 13 or newer because the settings client is SwiftUI. The
development bundle keeps the Rust helpers in `Contents/Resources`, where the
app's service supervisor can find them without a Homebrew installation.

```sh
./packaging/macos/build-app.sh
./packaging/macos/package-dmg.sh
```

The scripts create an unsigned `.app`, `.zip`, and `.dmg` under `dist/macos`.
The DMG includes the conventional `Applications` shortcut for drag-and-drop
installation.
Set `CODESIGN_IDENTITY='Developer ID Application: ...'` for a hardened runtime
signature. Notarization remains a release-pipeline concern and only runs when
the repository's signing secrets are present.
