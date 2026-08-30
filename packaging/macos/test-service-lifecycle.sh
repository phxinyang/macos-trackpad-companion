#!/usr/bin/env bash
set -euo pipefail

# Static regression guard for the app-termination lock race. The macOS Swift
# target cannot be compiled on the Linux packaging host, so keep the critical
# synchronous teardown contract executable in CI and local checks.
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SOURCE=${1:-"$ROOT/macos/TrackpadCompanionSettings/Sources/TrackpadCompanionSettings/ServiceSupervisor.swift"}
if [[ "$SOURCE" == "-" ]]; then
  TEXT=$(cat)
else
  TEXT=$(<"$SOURCE")
fi

contains() { [[ "$TEXT" == *"$1"* ]]; }

contains 'MainActor.assumeIsolated' || {
  echo "termination observer must stop the helper synchronously" >&2
  exit 1
}
contains 'self?.stopForApplicationTermination()' || {
  echo "termination observer must use the bounded teardown path" >&2
  exit 1
}
contains 'func stopForApplicationTermination()' || {
  echo "application termination cleanup entry point is missing" >&2
  exit 1
}
contains 'stopProcess(waitForExit: true)' || {
  echo "application termination must wait for helper exit" >&2
  exit 1
}
contains 'let deadline = Date().addingTimeInterval(2)' || {
  echo "helper termination wait must be bounded" >&2
  exit 1
}
contains 'kill(process.processIdentifier, SIGKILL)' || {
  echo "wedged helper must have a final lock-release fallback" >&2
  exit 1
}
contains 'Self.terminateAndWait(process)' || {
  echo "deinitialization must also release a live helper lock" >&2
  exit 1
}

echo "service lifecycle lock regression guard passed"
