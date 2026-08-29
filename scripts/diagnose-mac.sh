#!/usr/bin/env bash
set -u

# Read-only macOS diagnostics for Trackpad Companion.
# Bash 3.2 compatible: this is the default shell on supported macOS hosts.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIAG_DIR="${DIAG_DIR:-$REPO_ROOT/diagnostics}"
APP_PATH="${APP_PATH:-/Applications/Trackpad Companion.app}"
PORT="${PORT:-4242}"
COMMAND="collect"
OPEN_ACCESSIBILITY=0
SERVICE_BIN=""
REPORT_PATH=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/diagnose-mac.sh collect [--output PATH]
  scripts/diagnose-mac.sh probe [--port PORT]
  scripts/diagnose-mac.sh trace [--bin PATH] [--port PORT]
  scripts/diagnose-mac.sh permissions [--open]

Commands:
  collect      Read-only environment, app, config, process, port, and log report.
  probe        Read-only local HTTP/port checks for companion-net.
  trace        Collect a preflight report, run companion-net in the foreground
               with trace logging, then collect the exit state. Press Ctrl-C
               after reproducing the issue.
  permissions  Show the permission handoff and optionally open Accessibility
               settings. TCC still requires a user click; this never grants it.

Options:
  --output PATH  Write the report to PATH (default: diagnostics/mac-debug-*.txt).
  --bin PATH     Use this companion-net binary for trace.
  --app PATH     Inspect this .app bundle (default: /Applications/Trackpad Companion.app).
  --port PORT    Probe or override companion-net's port (default: 4242).
  --open         Open System Settings -> Privacy & Security -> Accessibility.

The script does not use sudo, change TCC, modify config, upload data, or open a
new listener. Reports are mode 0600 and redact common home, volume, and token
values. Share only a reviewed copy.
USAGE
}

redact() {
  # Keep reports useful while avoiding the most common local identifiers.
  sed -E \
    -e 's#(/Users/)[^/[:space:]]+#\1<user>#g' \
    -e 's#(/Volumes/)[^/[:space:]]+#\1<volume>#g' \
    -e 's#([Tt]oken[^:=,[:space:]]*[[:space:]]*[:=][[:space:]]*)[^,[:space:]}]+#\1<redacted>#g'
}

section() {
  printf '\n== %s ==\n' "$1"
}

command_exists() {
  command -v "$1" >/dev/null 2>&1
}

resolve_service_bin() {
  if [ -n "$SERVICE_BIN" ] && [ -x "$SERVICE_BIN" ]; then
    printf '%s\n' "$SERVICE_BIN"
    return 0
  fi
  if [ -x "$APP_PATH/Contents/Resources/companion-net" ]; then
    printf '%s\n' "$APP_PATH/Contents/Resources/companion-net"
    return 0
  fi
  if [ -x "$REPO_ROOT/target/release/companion-net" ]; then
    printf '%s\n' "$REPO_ROOT/target/release/companion-net"
    return 0
  fi
  if [ -x /opt/homebrew/bin/companion-net ]; then
    printf '%s\n' /opt/homebrew/bin/companion-net
    return 0
  fi
  if [ -x /usr/local/bin/companion-net ]; then
    printf '%s\n' /usr/local/bin/companion-net
    return 0
  fi
  return 1
}

resolve_config_bin() {
  if [ -x "$APP_PATH/Contents/Resources/companion-config" ]; then
    printf '%s\n' "$APP_PATH/Contents/Resources/companion-config"
    return 0
  fi
  if [ -x "$REPO_ROOT/target/release/companion-config" ]; then
    printf '%s\n' "$REPO_ROOT/target/release/companion-config"
    return 0
  fi
  if [ -x /opt/homebrew/bin/companion-config ]; then
    printf '%s\n' /opt/homebrew/bin/companion-config
    return 0
  fi
  if [ -x /usr/local/bin/companion-config ]; then
    printf '%s\n' /usr/local/bin/companion-config
    return 0
  fi
  return 1
}

collect_report() {
  section "report metadata"
  printf 'generated_at=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || date)"
  printf 'repo=%s\n' "$REPO_ROOT" | redact
  if [ -d "$REPO_ROOT/.git" ] && command_exists git; then
    git -C "$REPO_ROOT" rev-parse --short HEAD 2>&1
    git -C "$REPO_ROOT" status --short --branch 2>&1 | redact
  else
    printf 'git=unavailable\n'
  fi

  section "system"
  if command_exists sw_vers; then sw_vers 2>&1; else printf 'sw_vers=unavailable\n'; fi
  uname -a 2>&1
  if command_exists sysctl; then sysctl -n hw.model hw.machine 2>&1; fi
  printf 'uid=%s\n' "$(id -u 2>/dev/null || printf unknown)"

  section "app bundle"
  printf 'app_path=%s\n' "$APP_PATH" | redact
  if [ -f "$APP_PATH/Contents/Info.plist" ] && [ -x /usr/libexec/PlistBuddy ]; then
    /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP_PATH/Contents/Info.plist" 2>&1
    /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_PATH/Contents/Info.plist" 2>&1
    /usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$APP_PATH/Contents/Info.plist" 2>&1
  else
    printf 'bundle_info=unavailable\n'
  fi
  if [ -d "$APP_PATH" ]; then
    if command_exists codesign; then
      codesign --verify --deep --strict "$APP_PATH" 2>&1
      printf 'codesign_status=%s\n' "$?"
    fi
    for helper in \
      "$APP_PATH/Contents/MacOS/TrackpadCompanion" \
      "$APP_PATH/Contents/Resources/companion-net" \
      "$APP_PATH/Contents/Resources/companion-config"; do
      if [ -e "$helper" ]; then
        if command_exists stat; then stat -f '%Sp %N' "$helper" 2>&1 | redact; else printf 'present=%s\n' "$helper" | redact; fi
      else
        printf 'missing=%s\n' "$helper" | redact
      fi
    done
  fi

  section "configuration"
  config_bin=""
  if config_bin="$(resolve_config_bin 2>/dev/null)"; then
    printf 'companion_config=%s\n' "$config_bin" | redact
    "$config_bin" doctor 2>&1 | redact
  else
    printf 'companion-config=not-found\n'
  fi
  config_path="${XDG_CONFIG_HOME:-$HOME/.config}/macos-trackpad-companion/config.toml"
  printf 'config_path=%s\n' "$config_path" | redact
  if [ -e "$config_path" ]; then
    if command_exists stat; then stat -f '%Sp %N' "$config_path" 2>&1 | redact; else printf 'config_present=true\n'; fi
  else
    printf 'config_exists=false\n'
  fi

  section "accessibility handoff"
  printf 'The helper checks AXIsProcessTrustedWithOptions at startup.\n'
  printf 'A green label in the GUI can refer to the GUI process; the embedded helper may still be denied.\n'
  printf 'Run trace and share the exact helper error. This script does not change TCC.\n'
  if [ "$OPEN_ACCESSIBILITY" -eq 1 ]; then
    if command_exists open; then
      open 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility' 2>&1
      printf 'opened_accessibility_settings=true\n'
    else
      printf 'open=unavailable\n'
    fi
  fi

  section "processes and listeners"
  if command_exists pgrep; then pgrep -afil 'TrackpadCompanion|companion-net|companion' 2>&1 || true; fi
  if command_exists lsof; then
    lsof -nP -iTCP:"$PORT" -sTCP:LISTEN 2>&1 || true
    lsof -nP -iUDP:"$PORT" 2>&1 || true
  else
    printf 'lsof=unavailable\n'
  fi

  section "recent macOS logs"
  if command_exists log; then
    log show --last 15m --style compact --info --debug \
      --predicate '(process == "TrackpadCompanion" OR process == "companion-net")' 2>&1 \
      | tail -n 240 | redact
  else
    printf 'unified_log=unavailable\n'
  fi
  if [ -d "$HOME/Library/Logs/DiagnosticReports" ]; then
    for crash in \
      "$HOME/Library/Logs/DiagnosticReports"/Trackpad* \
      "$HOME/Library/Logs/DiagnosticReports"/companion*; do
      [ -f "$crash" ] || continue
      printf '%s\n' "$crash" | redact
    done
  fi
}

probe() {
  section "probe metadata"
  printf 'port=%s\n' "$PORT"
  section "tcp listener"
  if command_exists lsof; then
    lsof -nP -iTCP:"$PORT" -sTCP:LISTEN 2>&1 || true
  else
    printf 'lsof=unavailable\n'
  fi
  section "http endpoint"
  if command_exists curl; then
    curl --silent --show-error --max-time 3 \
      --output /dev/null --write-out 'http_status=%{http_code} remote=%{remote_ip} time=%{time_total}s\n' \
      "http://127.0.0.1:$PORT/" 2>&1 || true
  else
    printf 'curl=unavailable\n'
  fi
  section "websocket upgrade hint"
  printf 'The browser/WebSocket endpoint shares the HTTP listener; use trace to capture protocol errors.\n'
  section "udp listener"
  if command_exists lsof; then
    lsof -nP -iUDP:"$PORT" 2>&1 || true
  else
    printf 'lsof=unavailable\n'
  fi
}

trace() {
  trace_file="$DIAG_DIR/mac-trace-$(date '+%Y%m%d-%H%M%S').log"
  bin=""
  if ! bin="$(resolve_service_bin 2>/dev/null)"; then
    printf 'companion-net was not found. Use --bin PATH or build the macOS binary first.\n'
    return 1
  fi
  section "trace preflight"
  printf 'binary=%s\n' "$bin" | redact
  printf 'trace_log=%s\n' "$trace_file" | redact
  collect_report
  section "companion-net trace"
  printf 'Reproduce the issue now. Press Ctrl-C when finished.\n'
  printf 'RUST_LOG=macos_trackpad_companion=trace\n'
  : > "$trace_file"
  RUST_LOG="${RUST_LOG:-macos_trackpad_companion=trace}" \
  RUST_BACKTRACE=1 \
  "$bin" --port "$PORT" -vv 2>&1 | tee -a "$trace_file"
  status=${PIPESTATUS[0]}
  printf 'companion_net_exit_status=%s\n' "$status"
  section "trace postflight"
  probe
  section "trace log tail"
  tail -n 240 "$trace_file" 2>&1 | redact
  return "$status"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    collect|probe|trace|permissions)
      COMMAND="$1"
      shift
      ;;
    --output)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      REPORT_PATH="$2"
      shift 2
      ;;
    --bin)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      SERVICE_BIN="$2"
      shift 2
      ;;
    --app)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      APP_PATH="$2"
      shift 2
      ;;
    --port)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      PORT="$2"
      shift 2
      ;;
    --open)
      OPEN_ACCESSIBILITY=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

FINAL_STATUS=0
case "$COMMAND" in
  permissions)
    REPORT_PATH="${REPORT_PATH:-$DIAG_DIR/mac-permissions-$(date '+%Y%m%d-%H%M%S').txt}"
    ;;
  *)
    REPORT_PATH="${REPORT_PATH:-$DIAG_DIR/mac-debug-$(date '+%Y%m%d-%H%M%S').txt}"
    ;;
esac

mkdir -p "$(dirname "$REPORT_PATH")"
umask 077
: > "$REPORT_PATH"
chmod 600 "$REPORT_PATH" 2>/dev/null || true
exec > >(tee -a "$REPORT_PATH") 2>&1

printf 'Trackpad Companion macOS diagnostics\n'
printf 'report=%s\n' "$REPORT_PATH" | redact
printf 'This run is local and read-only unless you passed --open.\n'

case "$COMMAND" in
  collect)
    collect_report
    probe
    ;;
  probe)
    probe
    ;;
  trace)
    trace || FINAL_STATUS=$?
    ;;
  permissions)
    section "permissions"
    printf 'Accessibility is controlled by macOS TCC and cannot be granted by this script.\n'
    printf 'The GUI and embedded companion-net are separate processes and can have separate TCC entries.\n'
    if [ "$OPEN_ACCESSIBILITY" -eq 1 ]; then
      if command_exists open; then
        open 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility' 2>&1
        printf 'opened_accessibility_settings=true\n'
      else
        printf 'open=unavailable\n'
      fi
    else
      printf 'Run with --open to open the Accessibility pane.\n'
    fi
    ;;
esac

printf '\nReport complete: %s\n' "$REPORT_PATH" | redact
exit "$FINAL_STATUS"
