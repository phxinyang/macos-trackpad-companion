#!/usr/bin/env bash
set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIAGNOSE="$SCRIPT_DIR/diagnose-mac.sh"

input='companion-net --token plain-secret --token=equal-secret token: config-secret /Users/alice/private /Volumes/Backup/data'
output="$(printf '%s\n' "$input" | "$DIAGNOSE" redact)"

for secret in plain-secret equal-secret config-secret alice Backup; do
  if printf '%s\n' "$output" | grep -F "$secret" >/dev/null; then
    printf 'diagnostic redaction leaked %s\n' "$secret" >&2
    exit 1
  fi
done

printf '%s\n' "$output" | grep -F -- '--token <redacted>' >/dev/null
printf '%s\n' "$output" | grep -F -- '--token=<redacted>' >/dev/null
printf '%s\n' "$output" | grep -F -- 'token: <redacted>' >/dev/null
printf '%s\n' "$output" | grep -F -- '/Users/<user>/private' >/dev/null
printf '%s\n' "$output" | grep -F -- '/Volumes/<volume>/data' >/dev/null

printf 'diagnostic redaction guard passed\n'
