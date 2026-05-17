# Crash Recovery Validation Runbook

Use this runbook when storage, WAL, file format, commit, reopen, or recovery
behavior changes.

## Default Command

```bash
bash scripts/core_crash_recovery.sh
```

This is a focused 0.1 check. It is not a replacement for every historical chaos
or soak script.

## What It Does

The script runs `nervusdb-v2-crash-test` in driver mode. The driver bootstraps a
small committed graph, starts a writer process, randomly kills it, and then runs
verification after each kill.

Verification currently checks:

- WAL committed transactions can be replayed.
- Manifest epochs do not decrease.
- Last manifest segments can be loaded from `.ndb`.
- `GraphEngine::open` succeeds after the interrupted writer.
- Visible edges point to known visible nodes.

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

## Limits

This script does not prove full Cypher behavior, bindings, vector/HNSW
durability, optimizer behavior, or long soak stability. It also does not replace
targeted storage tests for node, edge, label, relationship type, property, and
format-epoch invariants.

## Larger Manual Runs

Run larger crash tests only when preparing a release or changing the recovery
model itself. Record hardware, OS, command, data size, and elapsed time.
