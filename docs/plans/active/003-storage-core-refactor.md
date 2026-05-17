# Plan 003: Storage Core Refactor

## Status

Planned

## Goal

Make storage boring and trustworthy for 0.1: local files, explicit format
versioning, WAL recovery, and persistence of nodes, edges, labels, relationship
types, and properties.

## Scope

- Document current `.ndb` and `.wal` behavior.
- Identify the format epoch/version path.
- Prove committed writes survive reopen and recovery.
- Keep one-writer and snapshot-read assumptions explicit.
- Add or tighten tests for storage invariants before behavior changes.

## Not In Scope

- Backup, vacuum, compact, or checkpoint as 0.1 product promises.
- Distributed storage.
- Vector index durability work.
- Broad page-cache redesign without a narrower storage plan.

## Steps

1. Audit `nervusdb-storage` for file layout, WAL, recovery, and version checks.
2. Write a small invariant list near the code or reference docs.
3. Add targeted tests for any missing invariant before changing behavior.
4. Refactor only the storage path needed for local graph persistence.
5. Update `docs/architecture/storage-model.md` and
   `docs/reference/storage-format.md`.

## Validation

- Targeted storage tests for touched invariants.
- `bash scripts/core_crash_recovery.sh` for recovery-affecting changes.
- `bash scripts/check.sh` before commit.

## Docs To Update

- `docs/architecture/storage-model.md`
- `docs/reference/storage-format.md`
- `docs/runbooks/crash-recovery-validation.md`

## Completion Evidence

Record test commands, recovery evidence, and any remaining storage risks.
