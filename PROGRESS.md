# PROGRESS

## Current Objective

Close the Fjall storage refactor, query scope pruning, and 0.1 readiness
follow-up so the repository describes one coherent embedded Rust graph core.

## Active Plan

`docs/plans/active/010-fjall-storage-refactor.md`

bd epic: `nervusdb-a1z`

## Current Phase

Fjall storage refactor, non-0.1 query residue pruning, post-refactor public
surface synchronization, and 0.1 API hook cleanup are complete in the working
tree.

## Now

- Validate and commit the 0.1 API hook cleanup.
- Prepare a release-readiness pass once the cleanup commit lands.

## Done

- 001 to 008 — Harness normalization, core classification, refactors, examples,
  doc backfill, codebase analysis.
- 009 — Slimming toward 0.1 completed and merged to `main`.
- `main` HEAD before the storage refactor: `ac91257b ci: add basic check workflow`.
- Fjall storage refactor committed:
  `2b63caa6 refactor(storage): replace custom backend with Fjall`.
- Query residue pruning committed:
  `22eae210 refactor(query): remove non-0.1 query residue`.
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
- Query cleanup completed: the public query path now rejects non-0.1 syntax and
  stale physical plan paths for optional match, aggregation, ordering, skip,
  distinct, unwind, union, and variable-length traversal were removed.
- D6 completed in the working tree: README, README_CN, CLI help, rustdoc,
  current architecture docs, current codebase analysis, and progress records
  now match the committed Fjall directory-storage model and Mini-Cypher 0.1
  query surface.
- API hook cleanup completed in the working tree: `Db::compact`,
  `Db::create_index`, and `GraphSnapshot::lookup_index` were removed from the
  public API. `Db::checkpoint` and `Db::close` remain as explicit lifecycle
  helpers over Fjall persistence.

## Next

- Commit the API hook cleanup after validation.
- Prepare a 0.1 readiness checklist after this cleanup lands.
- Run release-scale manual smoke on recorded hardware before release prep.

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
| 2026-06-21 | `cargo check -p nervusdb-query --lib` | Passed after query residue pruning |
| 2026-06-21 | `cargo clippy -p nervusdb-query --lib -- -D warnings` | Passed after query residue pruning |
| 2026-06-21 | `cargo test -p nervusdb-query` | Passed after query residue pruning |
| 2026-06-21 | `cargo test -p nervusdb --test core_0_1_mini_cypher` | Passed after query residue pruning |
| 2026-06-21 | `cargo test -p nervusdb --test core_0_1_examples` | Passed after query residue pruning |
| 2026-06-21 | `cargo fmt --all -- --check` | Passed after query residue pruning |
| 2026-06-21 | `bash scripts/check.sh` | Passed after query residue pruning |
| 2026-06-21 | `cargo fmt --all -- --check` | Passed after D6 public-surface cleanup |
| 2026-06-21 | `cargo check -p nervusdb-cli -p nervusdb-api -p nervusdb-query -p nervusdb` | Passed after D6 public-surface cleanup |
| 2026-06-21 | `bash scripts/check.sh` | Passed after D6 public-surface cleanup |
| 2026-06-21 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 10 storage contract tests |
| 2026-06-21 | `cargo test -p nervusdb --test core_0_1_examples` | Passed: 10 example tests |
| 2026-06-21 | `bash scripts/core_examples.sh` | Passed: 10 CLI/file-driven examples |
| 2026-06-21 | `bash scripts/core_crash_recovery.sh` | Passed: 5 kill/reopen iterations |
| 2026-06-21 | `cargo test --workspace` | Passed after D6 public-surface cleanup |
| 2026-06-21 | `cargo fmt --all -- --check` | Passed after API hook cleanup |
| 2026-06-21 | `cargo check -p nervusdb-api -p nervusdb-storage -p nervusdb -p nervusdb-cli -p nervusdb-query` | Passed after API hook cleanup |
| 2026-06-21 | `bash scripts/check.sh` | Passed after API hook cleanup |
| 2026-06-21 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 10 storage contract tests |
| 2026-06-21 | `bash scripts/core_crash_recovery.sh` | Passed after API hook cleanup |
| 2026-06-21 | `cargo test --workspace` | Passed after API hook cleanup |

## Last Checkpoint

2026-06-21: Fjall-backed directory storage and non-0.1 query pruning are both
committed. D6 public-surface cleanup is committed. API hook cleanup is complete
and validated in the working tree: false compaction/index hooks are gone, and
the remaining lifecycle helpers are `checkpoint` and `close`. Remaining work is
commit, then a 0.1 release-readiness pass.
