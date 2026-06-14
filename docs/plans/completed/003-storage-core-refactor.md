# Plan 003: Storage Core Refactor

## Status

In progress

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

## Current Audit

| Invariant | Current evidence | Gap |
|---|---|---|
| `.ndb` / `.wal` path derivation | `Db::open`, `Db::open_paths`, storage crash tool path derivation | Needs a facade baseline test that opens, drops, reopens, and queries through the default path. |
| Format epoch exists | `nervusdb-storage/src/lib.rs` defines `STORAGE_FORMAT_EPOCH = 1`; `pager.rs` stores it in the meta page | Needs a focused fail-fast test for mismatched epoch. |
| Committed node survives reopen | `nervusdb-storage/tests/m1_graph.rs` | Proven, but should be concentrated in a 0.1 storage baseline file. |
| Committed edge direction/type survives reopen | `m1_graph.rs`, `nervusdb/tests/resilience_labels.rs` | Needs a baseline test with a name tied to 0.1 storage. |
| Node property survives WAL replay | `nervusdb-storage/tests/properties.rs` | Needs reopen coverage in the same 0.1 storage baseline. |
| Edge property survives reopen | `nervusdb/tests/t155_edge_persistence.rs` | Proven outside the quick path; reference as evidence, do not make it default. |
| Label name/id survives reopen | WAL `CreateLabel` replay in `engine.rs`, facade resilience test | Existing evidence is split; add focused storage baseline coverage. |
| Uncommitted tx invisible after reopen | `m1_graph.rs`, `tombstone_semantics.rs` | Proven, but should be concentrated in the storage baseline. |
| WAL crash verifier exists | `scripts/core_crash_recovery.sh`, `nervusdb-v2-crash-test` | Document what it proves and what it does not prove. |

## Evidence Classes

- Proven: format epoch is encoded in pager metadata; committed graph writes,
  uncommitted rollback, snapshot isolation, and property replay have existing
  tests.
- Implemented but weakly proven: label and relationship-type persistence works
  through WAL `CreateLabel` replay, but needs a named 0.1 storage test.
- Out of 0.1 baseline: compaction as a product promise, vacuum, backup,
  vector/HNSW durability, advanced indexes, binding parity, and full Cypher
  compatibility.

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
- Do not run `bash scripts/workspace_full_test.sh` for this phase unless a
  change crosses broad workspace boundaries.

## Docs To Update

- `docs/architecture/storage-model.md`
- `docs/reference/storage-format.md`
- `docs/runbooks/crash-recovery-validation.md`

## Completion Evidence

Record test commands, recovery evidence, and any remaining storage risks.
