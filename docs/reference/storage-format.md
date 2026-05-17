# Storage Format Reference

This reference records the 0.1 expectations for local storage. It is not a
complete byte-level specification yet.

## Files

- `.ndb`: primary local database file.
- `.wal`: write-ahead log used for committed-write recovery.

The database path should be predictable from `Db::open` or `Db::open_paths`.

## Versioning

The storage layer must carry an explicit format epoch or version. If a file is
too new, too old, or otherwise incompatible, the database must fail fast with a
clear compatibility error instead of silently reading corrupt semantics.

## Recovery Assumptions

- Committed writes survive process failure and reopen.
- Uncommitted or partial writes do not become visible after recovery.
- Recovery must preserve nodes, edges, labels, relationship types, and
  properties.
- Recovery errors must be surfaced as errors, not ignored.

## Not Stable Yet

- Long-term cross-version compatibility policy.
- Public byte-level file format guarantees.
- Backup, vacuum, compaction, and checkpoint behavior as user-facing 0.1
  promises.

Changes here require storage-model docs and crash recovery validation.
