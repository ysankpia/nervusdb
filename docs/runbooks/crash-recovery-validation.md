# Crash Recovery Validation Runbook

Use this runbook when storage, WAL, file format, commit, reopen, or recovery
behavior changes.

## Default Command

```bash
bash scripts/core_crash_recovery.sh
```

This is a focused 0.1 check. It is not a replacement for every historical chaos
or soak script.

## When To Run

- WAL append, replay, checkpoint, or truncation changes.
- Page format or file epoch/version changes.
- Transaction commit visibility changes.
- Recovery error handling changes.
- Any storage refactor that can affect committed data after process failure.

## Evidence To Record

- Command.
- Git commit or working tree description.
- Data scale and temp database path policy.
- Whether committed nodes, edges, labels, and properties were visible after
  reopen.
- Any compatibility error observed for old or invalid formats.

## Larger Manual Runs

Run larger crash tests only when preparing a release or changing the recovery
model itself. Record hardware, OS, command, data size, and elapsed time.
