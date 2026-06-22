# PROGRESS

## Current Objective

NervusDB 0.0.7 storage-layout work is active. The goal is to collapse the
physical Fjall layout from the old many-keyspace graph model to four hot/cold
keyspaces without changing the public Rust API.

## Active Plan

`docs/plans/active/018-storage-layout-0.0.7.md`

bd epic: `nervusdb-a1z`

## Current Phase

0.0.7 is in release preparation as a destructive storage-format cleanup
release, not feature expansion.
`STORAGE_FORMAT_EPOCH` is now 3; epoch 2 database directories are rejected and
must be rebuilt or reimported.

## Now

- Prepare `nervusdb = "0.0.7"` for downstream projects after release validation
  passes and the tag/crates.io publication is complete.
- Treat the 0.0.7 cross-database benchmark as current storage-layout evidence
  after release, with the documented traversal regression caveat.
- 0.0.7 release scope is now clean reopen and storage footprint, not universal
  traversal/commit performance.
- Keep public index-management APIs, range indexes, EdgeId, unsafe/buffered
  durability modes, vectors, multi-writer work, and advanced Cypher out of scope
  unless a new ADR explicitly changes priority.

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
- 0.0.3 release preparation is in progress: workspace package versions and
  current install docs are being updated to `0.0.3`.
- 0.0.3 release completed:
  - tag: `v0.0.3`
  - GitHub release: `https://github.com/ysankpia/nervusdb/releases/tag/v0.0.3`
  - crates.io: `https://crates.io/crates/nervusdb`
- 0.0.4 property equality index planning started:
  - ADR: `docs/decisions/0007-node-property-equality-index.md`
  - active plan: `docs/plans/active/015-property-equality-index-0.0.4.md`
  - completed 0.0.3 plan moved to `docs/plans/completed/014-graph-integrity-0.0.3.md`
- 0.0.4 property equality index implemented:
  - `0f693a54 docs(plan): start 0.0.4 property equality index`
  - `a51995f1 feat(storage): maintain node property equality index`
  - `8b4101e8 feat(query): anchor node scans by property equality`
  - `adfd69b8 test(bench): measure property equality lookup`
  - Storage maintains `idx_node_props` atomically with node properties, labels,
    and node tombstones.
  - Query anchors label-qualified scalar property equality through
    `GraphSnapshot::nodes_with_label_and_property`.
  - No public `create_index` / `lookup_index` API was restored.
  - 100k/500k benchmark artifact:
    `artifacts/core-bench/core-bench-custom-100000n-5d-20260622-050241.json`.
  - Property lookup scan baseline: 68,519.803 ms.
  - Property lookup indexed path: 1.435 ms.
  - Property lookup speedup: 47,757.312x.
  - Insert throughput with index maintenance: 222,707.841 edges/sec.
- 0.0.4 release preparation started:
  - workspace package versions updated to `0.0.4`.
  - release notes added at `docs/releases/v0.0.4.md`.
- 0.0.4 release completed:
  - tag: `v0.0.4`
  - GitHub release: `https://github.com/ysankpia/nervusdb/releases/tag/v0.0.4`
  - crates.io: `https://crates.io/crates/nervusdb`
- 0.0.5 stability freeze planning started:
  - ADR: `docs/decisions/0008-stability-freeze-and-fsck-lite.md`
  - active plan: `docs/plans/active/016-stability-freeze-0.0.5.md`
  - completed 0.0.4 plan moved to
    `docs/plans/completed/015-property-equality-index-0.0.4.md`
- 0.0.5 stability freeze implemented locally:
  - `4a5472b5 docs(plan): start 0.0.5 stability freeze`
  - `532b04b5 feat(admin): add fsck-lite core`
  - `c701327f feat(cli): expose v2 fsck command`
  - `245b11cc test(core): add fsck and agent memory smoke`
  - `nervusdb::admin` exists only behind `unstable-admin`.
  - `nervusdb v2 fsck` supports `--repair` and `--json`.
  - repair rebuilds only `label_nodes` and `idx_node_props`.
  - Agent Memory smoke covers Character/Event/Fact graph usage, property-index
    lookup, traversal, update, detach delete, reopen, and feature-gated fsck.
  - small benchmark artifact:
    `artifacts/core-bench/core-bench-small-20260622-081528.json`.
- 0.0.5 release completed:
  - tag: `v0.0.5`
  - GitHub release: `https://github.com/ysankpia/nervusdb/releases/tag/v0.0.5`
  - crates.io: `https://crates.io/crates/nervusdb`
- 0.0.6 performance baseline started:
  - `83cfbb6b test(bench): add embedded graph cross-db baseline`
  - `187e53a9 docs(plan): start 0.0.6 performance hot path`
  - `32a3895d test(bench): split cross-db load and reopen timings`
  - `3a8a7a8e perf(storage): profile and trim graph hot paths`
  - `61f92163 perf(storage): trust maintained adjacency scans`
  - Cross-database medium benchmark artifact:
    `artifacts/cross-db-bench/cross-db-bench-medium-20260622-103209.ndjson`.
  - NervusDB, SQLite simple, and SQLite materialized shared correctness hash:
    `d4b70801ad0bb15b`.
- 0.0.6 storage hot-path follow-up completed in the working tree:
  - current code no longer has the old full `idx_node_props` scan cleanup path.
  - bulk property index writes now build a one-time created-node label map
    instead of linearly searching `created_nodes` per property.
  - staged properties on newly created nodes skip old-index cleanup because no
    committed old index can exist.
  - profiled medium artifact:
    `artifacts/cross-db-bench/cross-db-bench-medium-20260622-120913.ndjson`.
  - `WriteTxn::commit.property_index_writes` dropped from about `7.08s` to
    `244.979ms` on 100k/500k bulk load.
  - unprofiled medium artifact:
    `artifacts/cross-db-bench/cross-db-bench-medium-20260622-120945.ndjson`.
  - current NervusDB medium load total: `1,674.287ms`.
  - current remaining hard costs: durable `batch.commit` around `1.4-1.6s` and
    raw reopen around `3.0s+`; do not attack these without a storage-layout ADR.
- 0.0.6 performance hot path implemented locally:
  - benchmark schema now splits `load_total_ms`, `reopen_open_ms`, and
    `reopen_count_verify_ms`.
  - `NERVUSDB_PROFILE_STORAGE=1` emits env-gated storage-stage timings to
    stderr without changing public API or normal JSON output.
  - storage no longer scans all of `idx_node_props` for single node/property
    cleanup; it removes exact derived index keys from canonical labels/properties.
  - `neighbors()` and `incoming_neighbors()` stream Fjall prefix iterators and
    trust 0.0.3 write-path adjacency integrity instead of doing per-edge
    endpoint liveness point reads.
  - acceptance artifact:
    `artifacts/cross-db-bench/cross-db-bench-medium-20260622-115122.ndjson`.
  - acceptance hash: `d4b70801ad0bb15b` for NervusDB, SQLite simple, and
    SQLite materialized.
  - NervusDB medium result: load total `8,789.159 ms`, raw reopen
    `2,835.628 ms`, count verify `76.482 ms`, two-hop
    `3,085,997.505 paths/sec`, update p99 `3,998.917 us`, detach delete p99
    `5,001.000 us`.
  - profile artifact:
    `artifacts/cross-db-bench/cross-db-bench-medium-20260622-115442.ndjson`.
  - after the storage hot-path follow-up, profile evidence points to
    `batch.commit` and raw reopen as the remaining hard storage gaps.
  - release notes added at `docs/releases/v0.0.6.md`.
  - workspace package versions updated to `0.0.6`.
- 0.0.7 storage layout implementation started:
  - ADR: `docs/decisions/0009-storage-keyspace-consolidation.md`.
  - active plan: `docs/plans/active/018-storage-layout-0.0.7.md`.
  - `STORAGE_FORMAT_EPOCH` bumped from `2` to `3`.
  - physical Fjall keyspaces first collapsed to `meta` and `graph_data`; medium
    benchmark evidence showed that pure two-keyspace layout fixed clean reopen
    but regressed traversal locality.
  - current direction is four physical keyspaces: `meta`, `graph_data`,
    `adj_out`, and `adj_in`.
  - `graph_data` uses one-byte logical tags for nodes, names, labels,
    adjacency, properties, and node property equality index records.
  - `GraphEngine::open` validates `meta/format_epoch` before opening
    `graph_data`, so rejected epoch 2 directories are not polluted with the new
    keyspace.
  - fsck-lite scans tagged `graph_data` prefixes and still repairs only derived
    label and node-property indexes.
  - focused validation passed:
    `cargo test -p nervusdb-storage --test core_0_1_storage` with 22 tests,
    including epoch 2 rejection and keyspace-count coverage.
  - broader focused validation passed:
    `cargo fmt --all -- --check`,
    `cargo check -p nervusdb --examples`,
    `cargo test -p nervusdb --test core_0_1_mini_cypher`,
    `cargo test -p nervusdb-cli`,
    `cargo test -p nervusdb --test core_0_1_agent_memory`,
    `cargo test -p nervusdb --features unstable-admin --test core_0_1_agent_memory`,
    `bash scripts/check.sh`,
    `bash scripts/core_examples.sh`,
    `bash scripts/core_crash_recovery.sh`.
  - `Db::close()` now performs a clean shutdown flush by persisting the journal
    and waiting for `meta` and `graph_data` memtables to rotate; this preserves
    commit durability semantics while avoiding heavy journal replay on clean
    reopen.
  - 4-keyspace medium benchmark artifact:
    `artifacts/cross-db-bench/cross-db-bench-medium-20260622-150028.ndjson`.
  - current result is not release-complete: raw reopen improved from
    `3,249.434ms` to `3.185ms` and disk footprint from `84,595,889` to
    `38,315,826` bytes, but durable commit is still `1,476.589ms`, file count is
    `24`, and two-hop traversal regressed to `1,810,341 paths/sec`.
  - release scope explicitly re-scoped: 0.0.7 is a clean-reopen and footprint
    release. Traversal regression is documented in `docs/releases/v0.0.7.md`
    and is not hidden as a success.
  - release notes added at `docs/releases/v0.0.7.md`.
  - workspace package versions updated to `0.0.7`.
- 0.0.7 storage layout planning started:
  - active plan: `docs/plans/active/018-storage-layout-0.0.7.md`.
  - focus: durable commit, raw reopen, file count, and storage footprint.
  - implementation is blocked on a storage-layout ADR.
- 0.0.6 release completed:
  - tag: `v0.0.6`
  - GitHub release: `https://github.com/ysankpia/nervusdb/releases/tag/v0.0.6`
  - crates.io: `https://crates.io/crates/nervusdb`
  - confirmed via `cargo search nervusdb --limit 5 --registry crates-io`.

## Next

- If database work continues, write a storage-layout ADR before any keyspace
  merge or storage-format rewrite.
- Wait for GitHub Dependabot to rescan after the stale `fuzz/Cargo.lock`
  removal is pushed.
- Update GitHub Actions if the Node.js 20 deprecation annotation becomes noisy.

## Blockers

None.

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
| 2026-06-22 | `cargo check -p nervusdb --examples` | Passed after 0.0.3 version bump |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after 0.0.3 release preparation |
| 2026-06-22 | `bash scripts/check.sh` | Passed after 0.0.3 release preparation |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io --allow-dirty` | Passed after 0.0.3 release preparation; unused local patch warnings expected |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io` | Passed clean after 0.0.3 version bump; unused local patch warnings expected |
| 2026-06-22 | GitHub Actions `main` push run `27928160247` | Passed for commit `e1bc3726` |
| 2026-06-22 | `git tag -a v0.0.3` and `git push origin v0.0.3` | Passed; tag points at `e1bc3726` |
| 2026-06-22 | `gh release create v0.0.3 --verify-tag --title "NervusDB v0.0.3" --notes-file docs/releases/v0.0.3.md --latest=true` | Passed |
| 2026-06-22 | `cargo publish -p nervusdb --registry crates-io` | Published `nervusdb v0.0.3` |
| 2026-06-22 | `cargo search nervusdb --limit 5 --registry crates-io` | Confirmed `nervusdb = "0.0.3"` appears in crates.io search |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after 0.0.4 query planner and benchmark changes |
| 2026-06-22 | `cargo check -p nervusdb --examples` | Passed after 0.0.4 benchmark changes |
| 2026-06-22 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 20 storage tests including property equality index maintenance |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_mini_cypher` | Passed: 13 Mini-Cypher tests including property equality index query shapes |
| 2026-06-22 | `bash scripts/core_bench.sh --small` | Passed; artifact `artifacts/core-bench/core-bench-small-20260622-044017.json`; property lookup speedup 483.013x on 1k nodes |
| 2026-06-22 | `bash scripts/check.sh` | Passed after 0.0.4 property equality index changes |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_rust_api` | Passed after 0.0.4 property equality index changes |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_examples` | Passed: 10 example tests after 0.0.4 property equality index changes |
| 2026-06-22 | `bash scripts/core_examples.sh` | Passed: 10 CLI/file-driven examples after 0.0.4 property equality index changes |
| 2026-06-22 | `bash scripts/core_crash_recovery.sh` | Passed: 5 kill/reopen iterations after 0.0.4 property equality index changes |
| 2026-06-22 | `cargo test --workspace` | Passed after 0.0.4 property equality index changes |
| 2026-06-22 | `bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000` | Passed; artifact `artifacts/core-bench/core-bench-custom-100000n-5d-20260622-050241.json`; 100k nodes, 500k edges, scan 68,519.803 ms, index 1.435 ms, speedup 47,757.312x, insert 222,707.841 edges/sec |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after 0.0.4 version bump |
| 2026-06-22 | `cargo check -p nervusdb --examples` | Passed after 0.0.4 version bump |
| 2026-06-22 | `bash scripts/check.sh` | Passed after 0.0.4 release preparation |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io --allow-dirty` | Passed before release-prep commit; unused local patch warnings expected |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io` | Passed clean after release-prep commit; unused local patch warnings expected |
| 2026-06-22 | GitHub Actions `main` push run `27931274177` | Passed for commit `6f19ab8c` |
| 2026-06-22 | `git tag -a v0.0.4` and `git push origin v0.0.4` | Passed; tag points at `6f19ab8c` |
| 2026-06-22 | `gh release create v0.0.4 --verify-tag --title "NervusDB v0.0.4" --notes-file docs/releases/v0.0.4.md --latest=true` | Passed |
| 2026-06-22 | `cargo publish -p nervusdb --registry crates-io` | Published `nervusdb v0.0.4` |
| 2026-06-22 | `cargo search nervusdb --limit 5 --registry crates-io` | Confirmed `nervusdb = "0.0.4"` appears in crates.io search |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after 0.0.5 fsck-lite and Agent Memory smoke |
| 2026-06-22 | `cargo test -p nervusdb --lib --features unstable-admin admin::tests` | Passed: 7 fsck-lite focused tests |
| 2026-06-22 | `cargo clippy -p nervusdb --lib --features unstable-admin -- -D warnings` | Passed after fsck-lite core |
| 2026-06-22 | `cargo test -p nervusdb-cli` | Passed: CLI fsck text/unit tests and repair integration tests |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_agent_memory` | Passed: Agent Memory smoke |
| 2026-06-22 | `cargo test -p nervusdb --features unstable-admin --test core_0_1_agent_memory` | Passed: Agent Memory smoke plus offline fsck |
| 2026-06-22 | `bash scripts/core_smoke.sh` | Passed: CLI write/query/fsck plus Agent Memory smoke |
| 2026-06-22 | `cargo check -p nervusdb --examples` | Passed after 0.0.5 changes |
| 2026-06-22 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 20 storage tests after 0.0.5 changes |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_mini_cypher` | Passed: 13 Mini-Cypher tests after 0.0.5 changes |
| 2026-06-22 | `bash scripts/check.sh` | Passed after 0.0.5 changes |
| 2026-06-22 | `bash scripts/core_examples.sh` | Passed: 10 CLI/file-driven examples after 0.0.5 changes |
| 2026-06-22 | `bash scripts/core_crash_recovery.sh` | Passed: 5 kill/reopen iterations after 0.0.5 changes |
| 2026-06-22 | `bash scripts/core_bench.sh --small` | Passed; artifact `artifacts/core-bench/core-bench-small-20260622-081528.json`; property lookup speedup 483.069x on 1k nodes |
| 2026-06-22 | `cargo test --workspace` | Passed after 0.0.5 changes |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io --allow-dirty` | Passed before release-prep commit; unused local patch warnings expected |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io` | Passed clean after release-prep commit; unused local patch warnings expected |
| 2026-06-22 | GitHub Actions `main` push run `27940134969` | Passed for commit `0beba820` |
| 2026-06-22 | `git tag -a v0.0.5` and `git push origin v0.0.5` | Passed; tag points at `0beba820` |
| 2026-06-22 | `gh release create v0.0.5 --verify-tag --title "NervusDB v0.0.5" --notes-file docs/releases/v0.0.5.md --latest=true` | Passed |
| 2026-06-22 | `cargo publish -p nervusdb --registry crates-io` | Published `nervusdb v0.0.5` |
| 2026-06-22 | `cargo search nervusdb --limit 5 --registry crates-io` | Confirmed `nervusdb = "0.0.5"` appears in crates.io search |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after 0.0.6 created-node label-map hot-path fix |
| 2026-06-22 | `cargo clippy -p nervusdb --examples -- -D warnings` | Passed after 0.0.6 hot-path fix |
| 2026-06-22 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 20 storage tests after 0.0.6 hot-path fix |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_mini_cypher` | Passed: 13 Mini-Cypher tests after 0.0.6 hot-path fix |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_rust_api` | Passed after 0.0.6 hot-path fix |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_agent_memory` | Passed after 0.0.6 hot-path fix |
| 2026-06-22 | `cargo test -p nervusdb --features unstable-admin --test core_0_1_agent_memory` | Passed after 0.0.6 hot-path fix |
| 2026-06-22 | `NERVUSDB_PROFILE_STORAGE=1 bash scripts/cross_db_bench.sh --system nervusdb --medium` | Passed; artifact `artifacts/cross-db-bench/cross-db-bench-medium-20260622-120913.ndjson`; property_index_writes `244.979ms` |
| 2026-06-22 | `bash scripts/cross_db_bench.sh --system nervusdb --medium` | Passed; artifact `artifacts/cross-db-bench/cross-db-bench-medium-20260622-120945.ndjson`; load total `1,674.287ms`, two-hop `3,356,928.783 paths/s` |
| 2026-06-22 | GitHub Actions `main` push run `27955075842` | Passed for commit `c7d9c140` |
| 2026-06-22 | `git tag -a v0.0.6` and `git push origin v0.0.6` | Passed; tag points at `c7d9c140` |
| 2026-06-22 | `gh release create v0.0.6 --verify-tag --title "NervusDB v0.0.6" --notes-file docs/releases/v0.0.6.md --latest=true` | Passed |
| 2026-06-22 | `cargo publish -p nervusdb --registry crates-io` | Published `nervusdb v0.0.6` |
| 2026-06-22 | `cargo search nervusdb --limit 5 --registry crates-io` | Confirmed `nervusdb = "0.0.6"` appears in crates.io search |
| 2026-06-22 | `cargo fmt --all -- --check` | Passed after 0.0.7 release preparation |
| 2026-06-22 | `cargo check -p nervusdb --examples` | Passed after 0.0.7 release preparation |
| 2026-06-22 | `cargo test -p nervusdb-storage --test core_0_1_storage` | Passed: 22 storage tests including epoch 2 rejection and epoch 3 keyspace-count coverage |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_mini_cypher` | Passed: 13 Mini-Cypher tests |
| 2026-06-22 | `cargo test -p nervusdb-cli` | Passed: CLI fsck tests |
| 2026-06-22 | `cargo test -p nervusdb --test core_0_1_agent_memory` | Passed: Agent Memory smoke |
| 2026-06-22 | `cargo test -p nervusdb --features unstable-admin --test core_0_1_agent_memory` | Passed: Agent Memory smoke plus admin/fsck path |
| 2026-06-22 | `bash scripts/check.sh` | Passed after 0.0.7 release preparation |
| 2026-06-22 | `bash scripts/core_examples.sh` | Passed: 10 CLI/file-driven examples |
| 2026-06-22 | `bash scripts/core_crash_recovery.sh` | Passed: 5 kill/reopen iterations |
| 2026-06-22 | `cargo test --workspace` | Passed after 0.0.7 release preparation |
| 2026-06-22 | `cargo publish -p nervusdb --dry-run --registry crates-io` | Passed clean after release-prep commit; unused local patch warnings expected |
| 2026-06-22 | GitHub Actions `main` push run `27965344261` | Passed for commit `1ab64213` |
| 2026-06-22 | `git tag -a v0.0.7` and `git push origin v0.0.7` | Passed; tag points at `1ab64213` |
| 2026-06-22 | `gh release create v0.0.7 --verify-tag --title "NervusDB v0.0.7" --notes-file docs/releases/v0.0.7.md --latest=true` | Passed: `https://github.com/ysankpia/nervusdb/releases/tag/v0.0.7` |
| 2026-06-22 | `cargo publish -p nervusdb --registry crates-io` | Published `nervusdb v0.0.7` |
| 2026-06-22 | `cargo search nervusdb --limit 5 --registry crates-io` | Confirmed `nervusdb = "0.0.7"` appears in crates.io search |

## Last Checkpoint

2026-06-22: 0.0.7 has been tagged, released on GitHub, published to crates.io,
and confirmed via `cargo search`. The release succeeds as storage epoch 3,
clean-reopen, and footprint cleanup. It does not solve traversal throughput;
that regression is documented in `docs/releases/v0.0.7.md` and must not be
hidden in future planning.
