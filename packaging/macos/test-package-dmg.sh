#!/usr/bin/env bash
set -euo pipefail

# Static guard for the Finder AppleScript layout path. Pass a script path, or
# pipe a historical version with `git show <commit>:<path> | ... -` when
# checking that a regression is actually caught.
SCRIPT_PATH=${1:-"$(cd "$(dirname "$0")" && pwd)/package-dmg.sh"}
if [[ "$SCRIPT_PATH" == "-" ]]; then
  SCRIPT_TEXT=$(cat)
else
  SCRIPT_TEXT=$(<"$SCRIPT_PATH")
fi

contains() {
  [[ "$SCRIPT_TEXT" == *"$1"* ]]
}

contains 'set current view of container window to icon view' || {
  echo "DMG layout must operate on Finder's live container window" >&2
  exit 1
}
contains 'repeat with attempt from 1 to 12' || {
  echo "DMG layout must retry while Finder creates the container window" >&2
  exit 1
}
contains 'delay 0.25' || {
  echo "DMG layout retry delay is missing" >&2
  exit 1
}
contains 'MOUNT_NAME=${MOUNT##*/}' || {
  echo "DMG layout must bind the AppleScript to the mounted volume name" >&2
  exit 1
}
if contains 'set current view of dmgWindow to icon view'; then
  echo "stale Finder window reference is still used" >&2
  exit 1
fi

echo "DMG Finder layout regression guard passed"
