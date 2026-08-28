#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${OUT_DIR:-"$ROOT/dist/macos"}
APP=${APP_PATH:-"$OUT/Trackpad Companion.app"}
VERSION=${VERSION:-$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "package-dmg.sh must run on macOS" >&2
  exit 2
fi
[[ -d "$APP" ]] || { echo "App bundle not found: $APP (run build-app.sh first)" >&2; exit 1; }
mkdir -p "$OUT"
DMG="$OUT/Trackpad-Companion-$VERSION-macos.dmg"
rm -f "$DMG"
STAGE="$OUT/.dmg-stage"
rm -rf "$STAGE"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/Trackpad Companion.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Trackpad Companion" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"
echo "Created $DMG"
