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

contains 'set dmgWindow to container window of disk' || {
  echo "DMG layout must resolve Finder's live container window" >&2
  exit 1
}
if contains 'tell container window'; then
  echo "DMG layout must not use an unqualified Finder container window target" >&2
  exit 1
fi
contains 'set current view of dmgWindow to icon view' || {
  echo "DMG layout must select icon view on the live container window" >&2
  exit 1
}
contains 'repeat with attempt from 1 to 16' || {
  echo "DMG layout must retry while Finder creates the container window" >&2
  exit 1
}
contains 'delay 1' || {
  echo "DMG layout must wait for Finder to create the container window" >&2
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
if contains 'printf %s\\n ".hidden"' || contains "printf '%s\\n' \".hidden\""; then
  echo "helper .hidden manifest must not be created in the DMG staging tree" >&2
  exit 1
fi
contains 'rm -f "$STAGE/.hidden"' || {
  echo "DMG staging must remove a legacy .hidden manifest" >&2
  exit 1
}
contains 'rm -f "$MOUNT/.hidden"' || {
  echo "mounted DMG must remove a legacy .hidden manifest" >&2
  exit 1
}

echo "DMG Finder layout regression guard passed"
