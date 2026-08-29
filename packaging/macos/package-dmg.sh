#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${OUT_DIR:-"$ROOT/dist/macos"}
APP=${APP_PATH:-"$OUT/Trackpad Companion.app"}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "package-dmg.sh must run on macOS" >&2
  exit 2
fi
[[ -d "$APP" ]] || { echo "App bundle not found: $APP (run build-app.sh first)" >&2; exit 1; }
[[ -f "$APP/Contents/Info.plist" ]] || { echo "App bundle is missing Contents/Info.plist: $APP" >&2; exit 1; }
[[ -x "$APP/Contents/MacOS/TrackpadCompanion" ]] || { echo "App executable is missing: $APP/Contents/MacOS/TrackpadCompanion" >&2; exit 1; }
[[ -x "$APP/Contents/Resources/companion-net" ]] || { echo "Network helper is missing: $APP/Contents/Resources/companion-net" >&2; exit 1; }
[[ -x "$APP/Contents/Resources/companion-config" ]] || { echo "Config helper is missing: $APP/Contents/Resources/companion-config" >&2; exit 1; }
VERSION=${VERSION:-$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")}
mkdir -p "$OUT"
# `dist/macos` is a local build directory. Remove older DMGs so a shell glob
# cannot accidentally open a stale application after a successful rebuild.
for stale in "$OUT"/Trackpad-Companion-*-macos.dmg; do
  if [[ -e "$stale" ]]; then
    rm -f "$stale"
  fi
done
DMG="$OUT/Trackpad-Companion-$VERSION-macos.dmg"
rm -f "$DMG"
STAGE="$OUT/.dmg-stage"
RW_DMG="$OUT/.Trackpad-Companion-$VERSION-rw.dmg"
rm -rf "$STAGE"
MOUNT=""
cleanup() {
  if [[ -n "$MOUNT" ]]; then
    hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true
  fi
  rm -rf "$STAGE" "$RW_DMG"
}
trap cleanup EXIT
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/Trackpad Companion.app"
ln -s /Applications "$STAGE/Applications"
BACKGROUND="$ROOT/packaging/macos/dmg-background.png"
if [[ -f "$BACKGROUND" ]]; then
  mkdir -p "$STAGE/.background"
  cp "$BACKGROUND" "$STAGE/.background/dmg-background.png"
fi
hdiutil create -volname "Trackpad Companion" -srcfolder "$STAGE" -ov -format UDRW "$RW_DMG"
MOUNT=$(hdiutil attach "$RW_DMG" -readwrite -nobrowse -noautoopen | awk '/\/Volumes\// {print substr($0, index($0, "/Volumes/")); exit}')
if [[ -n "$MOUNT" && -x "$(command -v osascript 2>/dev/null || true)" ]]; then
  osascript <<APPLESCRIPT || echo "Warning: Finder DMG layout could not be applied; using the default layout." >&2
tell application "Finder"
  tell disk "Trackpad Companion"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set bounds of container window to {120, 120, 900, 620}
    set viewOptions to the icon view options of container window
    set icon size of viewOptions to 128
    set arrangement of viewOptions to not arranged
    if exists file ".background:dmg-background.png" then
      set background picture of viewOptions to file ".background:dmg-background.png"
    end if
    set position of item "Trackpad Companion.app" to {210, 250}
    set position of item "Applications" to {570, 250}
    close
    open
    update without registering applications
    delay 1
  end tell
end tell
APPLESCRIPT
fi
if [[ -n "$MOUNT" ]]; then
  hdiutil detach "$MOUNT" -quiet
  MOUNT=""
fi
hdiutil convert "$RW_DMG" -format UDZO -imagekey zlib-level=9 -ov -o "$DMG" >/dev/null
echo "Created $DMG"
