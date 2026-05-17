# Storage Model

The storage layer is the foundation of the 0.1 product. Query language breadth
does not matter if committed graph data is lost or reopened incorrectly.

## Files

- `.ndb`: primary page store.
- `.wal`: write-ahead log.

## Invariants

- File format changes must have an explicit epoch/version.
- Incompatible format versions must fail fast with a compatibility error.
- A committed write must survive process failure and reopen.
- A partial or uncommitted write must not become visible after recovery.
- Recovery validation must cover node, edge, label, and property data.

## Required Validation For Storage Changes

- Targeted storage tests for the changed invariant.
- Reopen or recovery-oriented tests.
- `bash scripts/core_crash_recovery.sh` for WAL/recovery changes.

