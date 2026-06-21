# Crash Recovery Validation Runbook

Use this runbook when storage, commit, reopen, or recovery behavior changes.
NervusDB 0.1 uses Fjall-backed local database directories; NervusDB no longer
owns a public `.ndb + .wal` file-pair format.

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

- `GraphEngine::open` succeeds after the interrupted writer.
- The database directory can be reopened after a killed writer process.
- The `CrashNode` label can be resolved and scanned through label lookup.
- Visible edges point to known visible nodes.
- Edge property decoding still works for committed graph data.
- External IDs do not resolve to invisible nodes.

## When To Run

- Fjall storage adapter changes.
- Logical storage format epoch changes.
- Transaction commit visibility changes.
- Recovery error handling changes.
- Any storage refactor that can affect committed data after process failure.

## Evidence To Record

- Command.
- Git commit or working tree description.
- Data scale and temp database path policy.
- Whether committed nodes, edges, labels, and properties were visible after
  reopen.
- Any compatibility error observed for invalid format epochs.

## Limits

This script does not prove full Cypher behavior, bindings, vector/HNSW
durability, optimizer behavior, Fjall internals, or long soak stability. It also
does not replace targeted storage tests for node, edge, label, relationship
type, property, snapshot, tombstone, and format-epoch invariants.

## Larger Manual Runs

Run larger crash tests only when preparing a release or changing the recovery
model itself. Record hardware, OS, command, data size, and elapsed time.
