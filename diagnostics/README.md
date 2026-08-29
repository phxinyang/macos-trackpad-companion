# Diagnostics

This directory is for local captures only. Diagnostic reports and trace logs are
ignored by Git because they can include operating-system build identifiers,
paths, process names, and timing data.

Run the script from the repository root on the Mac:

```sh
# Safe first pass: no daemon, no config changes, no network writes.
./scripts/diagnose-mac.sh collect

# Check whether the local service is listening and serving its browser page.
./scripts/diagnose-mac.sh probe --port 4242

# Reproduce a failure while the helper runs with trace logging.
./scripts/diagnose-mac.sh trace --port 4242

# Open the Accessibility pane. The user must still enable the correct entry.
./scripts/diagnose-mac.sh permissions --open
```

`collect` writes `diagnostics/mac-debug-*.txt`; `trace` writes an additional
`diagnostics/mac-trace-*.log`. Both files are mode `0600`. The script checks the
installed app, embedded helpers, configuration health, process CPU/memory and
port state, recent unified logs, and the HTTP endpoint. It does not use `sudo`,
grant TCC, modify configuration, upload data, or open a new listener.

When reporting an issue, paste the command output and the relevant report
sections only after reviewing them. Remove pairing tokens, host names, full
paths, and any raw user data. The exact `companion_net_exit_status` and the
first error line from `trace` are the most useful fields for diagnosing a
service that shows `Needs attention` in the GUI.
