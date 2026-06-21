# PROGRESS

## Current Objective

Implement the Fjall storage refactor and reset the 0.1 contract around embedded
Rust graph core.

## Active Plan

`docs/plans/active/010-fjall-storage-refactor.md`

bd epic: `nervusdb-a1z`

## Current Phase

Fjall storage refactor implemented and validated in the current working tree.

## Now

- Review the Fjall refactor diff before commit.
- Decide whether to clean query warning debt in a separate follow-up; it is not
  blocking the Fjall storage contract.

## Done

- 001 to 008 — Harness normalization, core classification, refactors, examples,
  doc backfill, codebase analysis.
- 009 — Slimming toward 0.1 completed and merged to `main`.
- `main` HEAD before this refactor: `ac91257b ci: add basic check workflow`.
- Fjall refactor epic created in bd: `nervusdb-a1z`.
- ADR 0005 accepted: Fjall replaces self-built Pager/WAL/B+Tree/CSR direction
  for 0.1.
- D0 completed: product, architecture, reference, engineering, roadmap, and
  plan docs now describe Fjall-backed local database directory storage.
- D1 completed: `nervusdb-query` no longer depends on `nervusdb-storage`;
  `WriteableGraph` and `PropertyValue` are storage-neutral API contracts.
- D2 completed: `nervusdb-storage` now uses Fjall keyspaces for nodes, labels,
  reltypes, adjacency, and properties.
- D3 completed: `Db::open(path)` opens a database directory; `open_paths`,
  `ndb_path`, and `wal_path` were removed from the public facade.
- D4 completed: old Pager/WAL/B+Tree/CSR/read-path files were deleted from the
  storage crate; core query tests were narrowed to documented 0.1 behavior.
- D5 completed: focused checks, core examples, crash recovery, default check,
  and full workspace tests passed.

## Next

- Commit the Fjall storage refactor once the diff is reviewed.
- Clean existing query warnings and MSRV/clippy drift as a separate maintenance
  task.
- Decide whether `Db::compact/checkpoint/close` should stay as explicit Fjall
  persist wrappers or be simplified post-0.1.

## Blockers

None yet.

## Validation Log

| Date | Check | Result |
|---|---|---|
| 2026-06-21 | `git status --short --branch` | Clean `main...origin/main` before changes |
| 2026-06-21 | CodeGraph exploration | Before refactor, facade/storage exposed `.ndb/.wal` and query depended on storage |
| 2026-06-21 | `bd ready` | Existing ready queue blocked/empty; new Fjall epic created |
| 2026-06-21 | `cargo check -p nervusdb-storage --lib --bins` | Passed after replacing crash-test with Fjall graph-level smoke |
| 2026-06-21 | `cargo check -p nervusdb` | Passed |
| 2026-06-21 | `cargo test -p nervusdb-api` | Passed |
| 2026-06-21 | `cargo test -p nervusdb-query` | Passed: 68 unit tests, doctests ignored as documented |
| 2026-06-21 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 9 storage contract tests |
| 2026-06-21 | `cargo test -p nervusdb --test core_0_1_rust_api` | Passed |
| 2026-06-21 | `cargo test -p nervusdb --test core_0_1_mini_cypher` | Passed: 10 core Mini-Cypher tests |
| 2026-06-21 | `bash scripts/core_examples.sh` | Passed |
| 2026-06-21 | `bash scripts/core_crash_recovery.sh` | Passed: 5 kill/reopen iterations |
| 2026-06-21 | `cargo fmt --all -- --check` | Passed |
| 2026-06-21 | `bash scripts/check.sh` | Passed |
| 2026-06-21 | `cargo test --workspace` | Passed |

## Last Checkpoint

2026-06-21: Fjall-backed directory storage is implemented in the working tree.
The current code path no longer compiles or ships the old Pager/WAL/B+Tree/CSR
storage engine. Full workspace tests pass. Existing query warnings remain as
separate cleanup debt, not a storage correctness blocker.
