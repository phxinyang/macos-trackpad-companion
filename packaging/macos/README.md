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
The DMG uses a Finder icon layout with a branded background, the app icon, and
the conventional `Applications` shortcut for drag-and-drop installation.
Packaging is intentionally non-interactive: neither script opens the resulting
DMG. Double-click the generated `Trackpad-Companion-*-macos.dmg` (or run
`open` yourself) when you want Finder to show the installation window.
The Finder layout guard can run on any host without mounting a disk image:

```sh
bash ./packaging/macos/test-package-dmg.sh
```

`package-dmg.sh` removes older local DMGs first, so opening a wildcard after a
rebuild cannot select a stale application. If `build-app.sh` fails during
compilation, it removes the previous local app bundle so a following packaging
step cannot silently package an old build.

`AppIcon.png` is the 1024px source for the generated `.icns`; `build-app.sh`
creates the standard 1x/2x iconset with macOS `sips` and `iconutil` in the
ignored `dist/macos` directory. `dmg-background.png` is embedded only in the
DMG staging volume and is not copied into the installed app.
The installer background is a `900x620` asset. Finder receives a fixed
`900x620` window and explicit icon-center coordinates, `{245,340}` for the app
and `{655,340}` for the Applications shortcut. The staging `.background`
directory and `.hidden` manifest are hidden before the image is created and
again after Finder saves its icon view, using filesystem flags plus Finder
metadata. The background contains only the title, one install instruction, and
the transfer arrow; Finder owns the two icon labels so explanatory copy cannot
overlap them.
Set `CODESIGN_IDENTITY='Developer ID Application: ...'` for a hardened runtime
signature. Notarization remains a release-pipeline concern: after signing,
submit the DMG with `xcrun notarytool submit` and staple it with
`xcrun stapler staple`. Credentials stay outside the repository.
