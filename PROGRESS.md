# PROGRESS

## Current Objective

NervusDB 0.0.3 is focused on graph integrity.

## Active Plan

`docs/plans/active/014-graph-integrity-0.0.3.md`

bd epic: `nervusdb-a1z`

## Current Phase

0.0.2 has been tagged, released on GitHub, and published to crates.io as the
single public `nervusdb` crate. 0.0.3 is now tightening graph write invariants
and delete cleanup.

## Now

- Reject dangling edges and mutations on missing or tombstoned graph entities.
- Make direct Rust API node deletion detach-clean related keyspaces.
- Keep property indexes, EdgeId, unsafe/buffered durability modes, vectors,
  multi-writer work, and advanced query work out of 0.0.3.

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
- ADR 0006 drafted: public 0.0.1 release should be a single `nervusdb` crate.
- Post-0.0.1 roadmap drafted as candidates, not current scope.
- Single-crate package shape implemented in the working tree:
  - `nervusdb` is self-contained and has no dependency on `nervusdb-api`,
    `nervusdb-storage`, or `nervusdb-query`.
  - `nervusdb-api`, `nervusdb-storage`, and `nervusdb-query` re-export
    `nervusdb` modules and are marked `publish = false`.
  - README, architecture, validation, and runbook docs describe `nervusdb` as
    the only 0.0.1 public crate.
  - `scripts/core_bench.sh` now benchmarks the public `nervusdb` crate.
- Single-crate package-shape commit created:
  `0cd081fc refactor(release): package nervusdb as single public crate`.
- Clean publish dry-run passed after commit:
  `cargo publish -p nervusdb --dry-run --registry crates-io`.
- Release notes for `v0.0.1` were written in `docs/releases/v0.0.1.md`.
- Tag `v0.0.1` created and pushed at `aa9315af`.
- GitHub release created:
  `https://github.com/ysankpia/nervusdb/releases/tag/v0.0.1`.
- crates.io package published:
  `https://crates.io/crates/nervusdb`.
- 0.0.2 write-path plan opened:
  `0c1de3b9 docs(plan): start 0.0.2 write path work`.
- 0.0.2 benchmark staging identified the real 0.0.1 bulk-write bug:
  `create_node` persisted `next_node_id` with `SyncAll` for every node before
  the transaction commit.
- 0.0.2 write-path changes in the working tree stage node ids inside
  `WriteTxn`, persist `next_node_id` in the commit batch, and stage edges in a
  `Vec<EdgeKey>` with commit-time sort/dedup.
- 0.0.2 release preparation is in progress: workspace package versions and
  current install docs are being updated to `0.0.2`.
- 0.0.2 release completed:
  - tag: `v0.0.2`
  - GitHub release: `https://github.com/ysankpia/nervusdb/releases/tag/v0.0.2`
  - crates.io: `https://crates.io/crates/nervusdb`
- Post-0.0.2 cleanup completed in the working tree:
  - removed the obsolete `fuzz/` workspace that targeted old query paths and
    carried the stale `rand` Dependabot alert source.
  - removed `docs/archive/legacy-platform-era/` from the working tree; deleted
    platform-era material is now historical evidence through git history only.
  - updated current docs so deleted archive/fuzz material cannot be mistaken for
    current 0.1 scope.
- 0.0.3 graph integrity implemented in the working tree:
  - storage rejects dangling edges and mutations on missing or tombstoned graph
    entities.
  - direct Rust API `tombstone_node` now detach-cleans labels, properties,
    incident adjacency, and incident edge properties.
  - `tombstone_edge` cleans edge properties.
  - Mini-Cypher plain `DELETE n` still rejects connected nodes, while
    `DETACH DELETE n` removes the node and relationships.

## Next

- Decide whether repeated read benchmark variance needs a separate
  benchmark plan.
- Wait for GitHub Dependabot to rescan after the stale `fuzz/Cargo.lock`
  removal is pushed.
- Update GitHub Actions if the Node.js 20 deprecation annotation becomes noisy.
- Decide whether 0.0.3 should be released immediately after review, or whether
  property equality index design should wait for a separate 0.0.4 ADR.

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
| 2026-06-22 | `cargo check -p nervusdb-api -p nervusdb-storage -p nervusdb-query -p nervusdb -p nervusdb-cli --lib --bins` | Passed after single-crate packaging |
| 2026-06-22 | `bash scripts/check.sh` | Passed after single-crate packaging |
| 2026-06-22 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 10 storage contract tests |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_rust_api && cargo test -p nervusdb --test core_0_1_examples` | Passed: 1 Rust API test and 10 example tests |
| 2026-06-22 | `bash scripts/core_examples.sh` | Passed: 10 CLI/file-driven examples |
| 2026-06-22 | `bash scripts/core_crash_recovery.sh` | Passed: 5 kill/reopen iterations |
| 2026-06-22 | `bash scripts/core_bench.sh --small` | Passed; artifact `artifacts/core-bench/core-bench-small-20260621-173958.json` |
| 2026-06-22 | `cargo test --workspace` | Passed after single-crate packaging |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io --allow-dirty` | Passed; dirty flag needed only because package files were not committed yet |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io` | Passed clean after commit `0cd081fc`; local patch warnings expected |
| 2026-06-22 | GitHub Actions `main` push run `27912993929` | Passed |
| 2026-06-22 | `bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000` | Passed in later manual run; artifact `artifacts/core-bench/core-bench-small-20260621-182012.json`; 100k nodes, 500k edges, insert 438.130s, hot 1,742,616 edges/sec, cold 976,857 edges/sec |
| 2026-06-22 | GitHub Actions `main` push run `27913320141` | Passed |
| 2026-06-22 | `git tag -a v0.0.1` and `git push origin v0.0.1` | Passed; tag points at `aa9315af` |
| 2026-06-22 | `gh release create v0.0.1 --verify-tag --title "NervusDB v0.0.1" --notes-file docs/releases/v0.0.1.md --latest=false` | Passed |
| 2026-06-22 | `cargo publish -p nervusdb --registry crates-io` | Published `nervusdb v0.0.1` |
| 2026-06-22 | `cargo search nervusdb --limit 10 --registry crates-io` | Confirmed `nervusdb = "0.0.1"` appears in crates.io search |
| 2026-06-22 | `cargo check -p nervusdb --examples` | Passed after 0.0.2 benchmark/write-path changes |
| 2026-06-22 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 11 storage contract tests after batched node id allocation and edge staging |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_rust_api` | Passed after 0.0.2 write-path changes |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_mini_cypher` | Passed after 0.0.2 write-path changes |
| 2026-06-22 | `bash scripts/core_bench.sh --small` | Passed; artifact `artifacts/core-bench/core-bench-small-20260621-190446.json`; insert 0.030s, 169,135 edges/sec |
| 2026-06-22 | `bash scripts/core_bench.sh --nodes 1000 --degree 5 --iters 100 --write-iters 20` | Passed; artifact `artifacts/core-bench/core-bench-custom-1000n-5d-20260621-190502.json`; custom naming verified |
| 2026-06-22 | `bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000` | Passed; artifact `artifacts/core-bench/core-bench-custom-100000n-5d-20260621-190510.json`; insert 0.415s, 1,204,516 edges/sec |
| 2026-06-22 | repeated `bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000` | Passed; artifacts `artifacts/core-bench/core-bench-custom-100000n-5d-20260621-190709.json` and `artifacts/core-bench/core-bench-custom-100000n-5d-20260621-190713.json`; insert stayed >881k edges/sec, read throughput varied |
| 2026-06-22 | GitHub Actions `main` push run `27914671878` | Passed for commit `a79b4bc8` |
| 2026-06-22 | `cargo test --workspace` | Passed after 0.0.2 write-path changes |
| 2026-06-22 | `bash scripts/core_examples.sh` | Passed: 10 CLI/file-driven examples after 0.0.2 write-path changes |
| 2026-06-22 | `bash scripts/core_crash_recovery.sh` | Passed: 5 kill/reopen iterations after 0.0.2 write-path changes |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io` | Passed after 0.0.2 write-path changes; existing `0.0.1` and unused local patch warnings expected |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after 0.0.2 version bump |
| 2026-06-22 | `cargo check -p nervusdb-api -p nervusdb-storage -p nervusdb-query -p nervusdb -p nervusdb-cli --lib --bins` | Passed after 0.0.2 version bump |
| 2026-06-22 | `bash scripts/check.sh` | Passed after 0.0.2 version bump |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io` | Passed clean after version bump; unused local patch warnings expected |
| 2026-06-22 | GitHub Actions `main` push run `27915458907` | Passed for commit `9c776651` |
| 2026-06-22 | `git tag -a v0.0.2` and `git push origin v0.0.2` | Passed; tag points at `9c776651` |
| 2026-06-22 | `gh release create v0.0.2 --verify-tag --title "NervusDB v0.0.2" --notes-file docs/releases/v0.0.2.md --latest=true` | Passed |
| 2026-06-22 | `cargo publish -p nervusdb --registry crates-io` | Published `nervusdb v0.0.2` |
| 2026-06-22 | `cargo search nervusdb --limit 5 --registry crates-io` | Confirmed `nervusdb = "0.0.2"` appears in crates.io search |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after removing stale fuzz workspace and legacy platform-era archive docs |
| 2026-06-22 | `bash scripts/check.sh` | Passed after removing stale fuzz workspace and legacy platform-era archive docs |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after 0.0.3 graph integrity changes |
| 2026-06-22 | `cargo check -p nervusdb --examples` | Passed after 0.0.3 graph integrity changes |
| 2026-06-22 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 16 storage integrity tests |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_rust_api` | Passed after 0.0.3 graph integrity changes |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_mini_cypher` | Passed: 12 Mini-Cypher tests including DELETE/DETACH DELETE regression |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_examples` | Passed: 10 example tests |
| 2026-06-22 | `bash scripts/check.sh` | Passed after 0.0.3 graph integrity changes |
| 2026-06-22 | `bash scripts/core_crash_recovery.sh` | Passed: 5 kill/reopen iterations |
| 2026-06-22 | `bash scripts/core_examples.sh` | Passed: 10 CLI/file-driven examples |
| 2026-06-22 | `cargo test --workspace` | Passed after 0.0.3 graph integrity changes |

## Last Checkpoint

2026-06-22: 0.0.3 graph integrity is implemented in the working tree. Storage
now rejects dangling edges and invalid mutations, direct Rust API node deletion
detach-cleans related keyspaces, and Mini-Cypher DELETE/DETACH DELETE behavior
is covered by regression tests. Full workspace validation passed.
