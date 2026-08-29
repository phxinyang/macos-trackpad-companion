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
VERSION=${VERSION:-$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")}
mkdir -p "$OUT"
DMG="$OUT/Trackpad-Companion-$VERSION-macos.dmg"
rm -f "$DMG"
STAGE="$OUT/.dmg-stage"
rm -rf "$STAGE"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/Trackpad Companion.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Trackpad Companion" -srcfolder "$STAGE" -ov -format UDZO -imagekey zlib-level=9 "$DMG"
echo "Created $DMG"
