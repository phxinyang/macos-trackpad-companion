#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${OUT_DIR:-"$ROOT/dist/macos"}
APP=${APP_PATH:-"$OUT/Trackpad Companion.app"}
DMG_WIDTH=900
DMG_HEIGHT=620
WINDOW_LEFT=120
WINDOW_TOP=120
WINDOW_RIGHT=$((WINDOW_LEFT + DMG_WIDTH))
WINDOW_BOTTOM=$((WINDOW_TOP + DMG_HEIGHT))
SETFILE=""
if [[ -x "$(command -v SetFile 2>/dev/null || true)" ]]; then
  SETFILE=$(command -v SetFile)
elif [[ -x "$(command -v xcrun 2>/dev/null || true)" ]]; then
  SETFILE=$(xcrun --find SetFile 2>/dev/null || true)
fi

mark_hidden() {
  chflags hidden "$1" 2>/dev/null || true
  if [[ -n "$SETFILE" && -x "$SETFILE" ]]; then
    "$SETFILE" -a V "$1" 2>/dev/null || true
  fi
}

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
  # Older revisions wrote a helper `.hidden` manifest into the staging root.
  # Remove it explicitly so a reused workspace or interrupted packaging run
  # can never copy that implementation detail into the released image.
  rm -f "$STAGE/.hidden"
  # Mark the staging directory invisible before creating the image. The
  # filesystem flag covers normal Finder sessions; SetFile also writes the
  # Finder invisible bit when the Xcode command-line tools provide it.
  mark_hidden "$STAGE/.background"
fi
hdiutil create -volname "Trackpad Companion" -fs HFS+ -srcfolder "$STAGE" -ov -format UDRW "$RW_DMG"
MOUNT=$(hdiutil attach "$RW_DMG" -readwrite -nobrowse -noautoopen | awk '/\/Volumes\// {print substr($0, index($0, "/Volumes/")); exit}')
MOUNT_NAME=${MOUNT##*/}
if [[ -n "$MOUNT" ]]; then
  # hdiutil/Finder can preserve files from an older read-write image. The
  # manifest is never needed by Finder, so remove it before saving the layout.
  rm -f "$MOUNT/.hidden"
fi
if [[ -n "$MOUNT" && -d "$MOUNT/.background" ]]; then
  mark_hidden "$MOUNT/.background"
fi
if [[ -n "$MOUNT" && -x "$(command -v osascript 2>/dev/null || true)" ]]; then
  # Finder must briefly open the hidden staging volume to persist its icon
  # view. Finder is hidden for this internal step, and the final DMG is never
  # opened by this packaging script.
  osascript <<APPLESCRIPT || echo "Warning: Finder DMG layout could not be applied; using the default layout." >&2
tell application "Finder"
  set finderWasVisible to visible
  try
    set visible to false
    tell disk "${MOUNT_NAME}"
      open
      delay 1
      try
        set visible of item ".background" of disk "${MOUNT_NAME}" to false
      end try
      set layoutReady to false
      set lastLayoutError to "unknown Finder error"
      set dmgWindow to missing value
      repeat with attempt from 1 to 16
        try
          -- Finder creates the container window asynchronously. Re-resolve
          -- the live reference on every attempt so a previous failed lookup
          -- cannot leave us with a stale window object.
          if not (exists container window of disk "${MOUNT_NAME}") then error "container window not ready"
          set dmgWindow to container window of disk "${MOUNT_NAME}"
          -- Keep the explicit Finder object form here. Setting properties
          -- implicitly inside `tell dmgWindow` is rejected by some Finder
          -- builds with error -10006 even when the window is valid.
          set current view of dmgWindow to icon view
          set toolbar visible of dmgWindow to false
          set statusbar visible of dmgWindow to false
          set bounds of dmgWindow to {${WINDOW_LEFT}, ${WINDOW_TOP}, ${WINDOW_RIGHT}, ${WINDOW_BOTTOM}}
          set viewOptions to icon view options of dmgWindow
          set icon size of viewOptions to 128
          set arrangement of viewOptions to not arranged
          try
            set background picture of viewOptions to file ".background:dmg-background.png"
          end try
          if exists item "Trackpad Companion.app" of dmgWindow then
            set position of item "Trackpad Companion.app" of dmgWindow to {245, 340}
          end if
          if exists item "Applications" of dmgWindow then
            set position of item "Applications" of dmgWindow to {655, 340}
          end if
          set layoutReady to true
          exit repeat
        on error errMsg number errNum
          set lastLayoutError to errMsg & " (" & errNum & ")"
          delay 0.25
        end try
      end repeat
      if not layoutReady then error "Finder DMG window did not become ready: " & lastLayoutError
      close dmgWindow
      open
      delay 0.2
      update without registering applications
    end tell
  on error errMsg number errNum
    set visible to finderWasVisible
    error errMsg number errNum
  end try
  set visible to finderWasVisible
end tell
APPLESCRIPT
fi
if [[ -n "$MOUNT" && -d "$MOUNT/.background" ]]; then
  # Finder may rewrite visibility metadata while saving the icon view.
  # Re-apply it immediately before detaching so the released image never
  # exposes its staging folder in a normal Finder window.
  mark_hidden "$MOUNT/.background"
fi
if [[ -n "$MOUNT" ]]; then
  # Keep the final image free of the legacy helper even if Finder recreated it
  # while persisting the icon view.
  rm -f "$MOUNT/.hidden"
fi
if [[ -n "$MOUNT" ]]; then
  hdiutil detach "$MOUNT" -quiet
  MOUNT=""
fi
hdiutil convert "$RW_DMG" -format UDZO -imagekey zlib-level=9 -ov -o "$DMG" >/dev/null
echo "Created $DMG"
