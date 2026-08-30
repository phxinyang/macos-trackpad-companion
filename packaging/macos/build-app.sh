#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${OUT_DIR:-"$ROOT/dist/macos"}
APP_NAME="Trackpad Companion.app"
APP="$OUT/$APP_NAME"
CONFIG=${CONFIGURATION:-release}
ICON=${APP_ICON:-"$OUT/AppIcon.icns"}
ICON_SOURCE=${APP_ICON_SOURCE:-"$ROOT/packaging/macos/AppIcon.png"}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "build-app.sh must run on macOS (SwiftUI and codesign are macOS tools)" >&2
  exit 2
fi
command -v swift >/dev/null || { echo "swift is required" >&2; exit 2; }
command -v cargo >/dev/null || { echo "cargo is required" >&2; exit 2; }

# Never let package-dmg reuse an older app when this build fails halfway
# through. The app is recreated only after all compilation steps succeed.
rm -rf "$APP"

VERSION=${VERSION:-$(git -C "$ROOT" describe --tags --always --dirty 2>/dev/null || echo 0.1.0)}
# CFBundleVersion cannot contain a slash or whitespace. Git descriptions are
# otherwise kept verbatim so nightly builds remain easy to identify.
VERSION=$(printf '%s' "$VERSION" | tr '/ ' '--')
if [[ -n "${RUST_TARGET:-}" ]]; then
  (cd "$ROOT" && cargo build --locked --release --target "$RUST_TARGET" --bin companion-net --bin companion-config)
else
  # Do not expand an empty array here. Bash 3.2, still shipped by macOS,
  # treats an empty "${array[@]}" as unset while `set -u` is active.
  (cd "$ROOT" && cargo build --locked --release --bin companion-net --bin companion-config)
fi
(cd "$ROOT/macos/TrackpadCompanionSettings" && swift build -c "$CONFIG")

SWIFT_BIN="$ROOT/macos/TrackpadCompanionSettings/.build/$CONFIG/TrackpadCompanionSettings"
SWIFT_BUILD_DIR="$ROOT/macos/TrackpadCompanionSettings/.build/$CONFIG"
RUST_BIN_DIR="$ROOT/target/release"
if [[ -n "${RUST_TARGET:-}" ]]; then RUST_BIN_DIR="$ROOT/target/$RUST_TARGET/release"; fi
for binary in companion-net companion-config; do
  [[ -x "$RUST_BIN_DIR/$binary" ]] || { echo "missing $RUST_BIN_DIR/$binary" >&2; exit 1; }
done
[[ -x "$SWIFT_BIN" ]] || { echo "missing $SWIFT_BIN" >&2; exit 1; }

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$SWIFT_BIN" "$APP/Contents/MacOS/TrackpadCompanion"
cp "$RUST_BIN_DIR/companion-net" "$APP/Contents/Resources/companion-net"
cp "$RUST_BIN_DIR/companion-config" "$APP/Contents/Resources/companion-config"
cp "$ROOT/packaging/macos/Info.plist" "$APP/Contents/Info.plist"
chmod 755 "$APP/Contents/MacOS/TrackpadCompanion" "$APP/Contents/Resources/companion-net" "$APP/Contents/Resources/companion-config"

# SwiftPM keeps package resources next to the executable. The PermissionFlow
# UI resolves its localized strings from this bundle at runtime, so include it
# in the manually assembled app bundle used by this project.
PERMISSIONFLOW_BUNDLE="$SWIFT_BUILD_DIR/PermissionFlow_PermissionFlow.bundle"
if [[ ! -d "$PERMISSIONFLOW_BUNDLE" ]]; then
  # Newer SwiftPM toolchains put architecture-specific products below
  # `.build/<triple>/<configuration>` while keeping the executable symlink at
  # `.build/<configuration>`.
  PERMISSIONFLOW_BUNDLE=$(find "$ROOT/macos/TrackpadCompanionSettings/.build" \
    -type d -name "PermissionFlow_PermissionFlow.bundle" -print -quit 2>/dev/null || true)
fi
if [[ -n "$PERMISSIONFLOW_BUNDLE" && -d "$PERMISSIONFLOW_BUNDLE" ]]; then
  cp -R "$PERMISSIONFLOW_BUNDLE" "$APP/Contents/Resources/"
else
  echo "Warning: PermissionFlow resource bundle not found under .build; using fallback strings." >&2
fi
if [[ ! -f "$ICON" && -f "$ICON_SOURCE" && -x "$(command -v sips 2>/dev/null || true)" && -x "$(command -v iconutil 2>/dev/null || true)" ]]; then
  ICONSET="$OUT/.AppIcon.iconset"
  rm -rf "$ICONSET"
  mkdir -p "$ICONSET"
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$ICON_SOURCE" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$ICON_SOURCE" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$ICONSET" -o "$ICON"
  rm -rf "$ICONSET"
fi
if [[ -f "$ICON" ]]; then
  cp "$ICON" "$APP/Contents/Resources/AppIcon.icns"
else
  echo "No AppIcon.icns found; using the default application icon."
fi
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"
if [[ -f "$ICON" ]]; then
  /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string AppIcon" "$APP/Contents/Info.plist" 2>/dev/null || \
    /usr/libexec/PlistBuddy -c "Set :CFBundleIconFile AppIcon" "$APP/Contents/Info.plist"
fi

if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$APP/Contents/Resources/companion-net"
  codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$APP/Contents/Resources/companion-config"
  codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$APP/Contents/MacOS/TrackpadCompanion"
  codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$APP"
  codesign --verify --deep --strict --verbose=2 "$APP"
else
  echo "No CODESIGN_IDENTITY set; leaving app unsigned for development."
fi

mkdir -p "$OUT"
(cd "$OUT" && ditto -c -k --sequesterRsrc --keepParent "$APP" "Trackpad-Companion-$VERSION-macos.zip")
echo "Created $APP"
echo "Created $OUT/Trackpad-Companion-$VERSION-macos.zip"
