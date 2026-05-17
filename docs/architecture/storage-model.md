# Storage Model

The storage layer is the foundation of the 0.1 product. Query language breadth
does not matter if committed graph data is lost or reopened incorrectly.

## Files

- `.ndb`: primary page store.
- `.wal`: write-ahead log.

## Current Layout

The `.ndb` file starts with a meta page and bitmap page. The meta page currently
stores file magic, major/minor version, page size, bitmap page id, next page id,
ID-map root/length state, index catalog root, next index id, and
`storage_format_epoch`.

The `.wal` file stores transaction records. A committed transaction is replayed
only when the WAL contains a complete `BeginTx ... CommitTx` sequence.

## Invariants

- File format changes must have an explicit epoch/version.
- Incompatible format versions must fail fast with a compatibility error.
- A committed write must survive process failure and reopen.
- A partial or uncommitted write must not become visible after recovery.
- Recovery validation must cover node, edge, label, and property data.
- Relationship direction and relationship type must survive reopen.
- WAL replay must be safe across repeated reopen attempts.
- Recovery failure must surface as an error, not silently continue.

## Durability Baseline

The current commit path appends graph changes to WAL, appends `CommitTx`, then
calls `wal.fsync()`. That is the 0.1 durability baseline. Changes to this path
must add or update recovery tests before implementation.

## Required Validation For Storage Changes

- Targeted storage tests for the changed invariant.
- Reopen or recovery-oriented tests.
- `bash scripts/core_crash_recovery.sh` for WAL/recovery changes.
- No full workspace test by default; use it only for broad cross-workspace
  changes.
