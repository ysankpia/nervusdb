# 017 Performance Hot Path 0.0.6

## Status

Implemented locally; release prep not started.

## Goal

Make the 0.0.6 performance work evidence-driven: first fix benchmark attribution,
then remove the storage hot-path costs already proven by code inspection.

## Scope

- Keep the cross-database benchmark as the 0.0.6 baseline.
- Split `cross_db_bench` load and reopen timings so numbers are attributable.
- Add env-gated internal storage profiling with no public API change.
- Remove `idx_node_props` cleanup paths that scan the whole property index for a
  single node or property.
- Remove repeated created-node label lookup from bulk property index writes.
- Stream traversal prefix scans instead of materializing every neighbor key into
  a `Vec<Vec<u8>>`.
- Preserve default `SyncAll` durability and existing Mini-Cypher behavior.

## Not In Scope

- C storage rewrite.
- Keyspace merge as the first optimization.
- Unsafe/buffered durability mode.
- Kuzu or broader database comparison.
- Query executor row symbolization.
- New query features, vector search, EdgeId, or parallel edges.

## Baseline Evidence

Cross-database medium benchmark:

```text
command: bash scripts/cross_db_bench.sh --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-103209.ndjson
dataset: 100,000 nodes / 500,000 edges / 10,000 read iterations / 100 mutation iterations
correctness_hash: d4b70801ad0bb15b for NervusDB, SQLite simple, and SQLite materialized
```

Observed NervusDB baseline:

| Metric | Baseline |
|---|---:|
| commit | 8,018.821 ms |
| mixed reopen + count verify | 2,913.563 ms |
| lookup p99 | 2.000 us |
| one-hop cold | 1,043,008 edges/s |
| two-hop | 1,107,359 paths/s |
| update p99 | 85,827.125 us |
| detach delete p99 | 82,955.750 us |
| disk | 84,595,889 bytes / 31 files |

## Acceptance

- `cross_db_bench` emits `load_total_ms`, `reopen_open_ms`, and
  `reopen_count_verify_ms`.
- `NERVUSDB_PROFILE_STORAGE=1` emits useful storage-stage timings without
  changing public API or normal output.
- Node property update/remove, label remove, and tombstone-node cleanup no
  longer scan all of `idx_node_props` for a single node/property cleanup.
- `neighbors()` and `incoming_neighbors()` no longer allocate a `Vec<Vec<u8>>`
  for every prefix scan.
- Medium benchmark keeps matching correctness hashes across all systems.
- Medium NervusDB target:
  - update p99 `< 30,000 us`, or profile evidence explains remaining cost.
  - detach delete p99 `< 30,000 us`, or profile evidence explains remaining cost.
  - two-hop `>= 2,000,000 paths/s`, or profile evidence explains remaining cost.

## Implementation Evidence

Commits:

```text
83cfbb6b test(bench): add embedded graph cross-db baseline
187e53a9 docs(plan): start 0.0.6 performance hot path
32a3895d test(bench): split cross-db load and reopen timings
3a8a7a8e perf(storage): profile and trim graph hot paths
61f92163 perf(storage): trust maintained adjacency scans
```

Storage changes:

- `cross_db_bench` now emits `load_total_ms`, `reopen_open_ms`, and
  `reopen_count_verify_ms`; old `reopen_verify_ms` remains as the sum for
  compatibility.
- `NERVUSDB_PROFILE_STORAGE=1` emits internal open, count, commit, batch commit,
  cleanup, property/index, and aggregated traversal scan timings to stderr.
- Node property index cleanup no longer scans all of `idx_node_props` for
  property update/remove, label remove, or tombstone-node cleanup. It derives
  exact old index keys from canonical node labels and node properties.
- Bulk property index writes no longer linearly search `created_nodes` for every
  staged property. Commit builds a one-time created-node label map and skips old
  index cleanup for nodes created in the same transaction.
- `neighbors()` and `incoming_neighbors()` consume Fjall prefix iterators
  directly instead of first materializing all keys into `Vec<Vec<u8>>`.
- Adjacency reads trust the 0.0.3 write-path graph integrity invariant and no
  longer perform two `node_is_live` point reads per edge. Corruption detection
  remains fsck-lite's job; normal reads should not pay that cost.

Medium acceptance run:

```text
command: bash scripts/cross_db_bench.sh --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-115122.ndjson
dataset: 100,000 nodes / 500,000 edges / 10,000 read iterations / 100 mutation iterations
correctness_hash: d4b70801ad0bb15b for NervusDB, SQLite simple, and SQLite materialized
```

| System | Load total ms | Reopen open ms | Reopen count verify ms | Two-hop paths/s | Update p99 us | Detach delete p99 us | Disk bytes / files |
|---|---:|---:|---:|---:|---:|---:|---:|
| NervusDB | 8,789.159 | 2,835.628 | 76.482 | 3,085,997.505 | 3,998.917 | 5,001.000 | 84,595,889 / 31 |
| SQLite simple | 580.107 | 0.292 | 13.635 | 7,299,536.479 | 1,473.458 | 117.167 | 29,319,168 / 1 |
| SQLite materialized | 787.731 | 0.299 | 8.935 | 7,451,333.404 | 3,162.833 | 8,457.041 | 38,244,352 / 1 |

NervusDB medium targets are met:

- update p99 target `< 30,000 us`: actual `3,998.917 us`.
- detach delete p99 target `< 30,000 us`: actual `5,001.000 us`.
- two-hop target `>= 2,000,000 paths/s`: actual `3,085,997.505 paths/s`.

Profile run:

```text
command: NERVUSDB_PROFILE_STORAGE=1 bash scripts/cross_db_bench.sh --system nervusdb --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-115442.ndjson
```

Initial profile evidence identified a second bulk property-index hot path:
`final_node_labels()` linearly searched `created_nodes` for every staged
property. That made the bulk property-index phase scale with
`created_nodes * node_props` even after the old full-index cleanup scan was
removed.

Follow-up profiled run:

```text
command: NERVUSDB_PROFILE_STORAGE=1 bash scripts/cross_db_bench.sh --system nervusdb --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-120913.ndjson
```

| Stage | Before follow-up | After follow-up |
|---|---:|---:|
| `WriteTxn::commit.property_index_writes` | ~7.08 s | 244.979 ms |
| `WriteTxn::commit.batch_commit` | ~1.25 s | 1.408 s |
| `WriteTxn::commit` | ~8.38 s | 1.699 s |

Unprofiled follow-up run:

```text
command: bash scripts/cross_db_bench.sh --system nervusdb --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-120945.ndjson
```

- Load total is now `1,674.287 ms`, down from the earlier local
  `8,789.159 ms` accepted run.
- Two-hop traversal is `3,356,928.783 paths/s`.
- Update p99 is `5,010.542 us`.
- Detach delete p99 is `6,480.459 us`.
- Raw reopen remains about `3.25s`; count verification is about `105ms`.

The property/index staging bug is no longer the remaining hard gap. The next
large costs are `batch.commit` and raw `GraphEngine::open.database`, both of
which point toward storage-layout/Fjall-file-structure work. 0.0.6 should still
not immediately merge keyspaces in this plan; that is a storage-format project
that needs a separate ADR and migration/fsck story.

## Validation

Development:

```bash
cargo fmt --all -- --check
cargo check -p nervusdb --examples
cargo clippy -p nervusdb --examples -- -D warnings
bash scripts/cross_db_bench.sh --small
```

Storage correctness:

```bash
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_rust_api
cargo test -p nervusdb --test core_0_1_mini_cypher
cargo test -p nervusdb --test core_0_1_agent_memory
cargo test -p nervusdb --features unstable-admin --test core_0_1_agent_memory
bash scripts/core_crash_recovery.sh
```

Performance evidence:

```bash
bash scripts/cross_db_bench.sh --medium
NERVUSDB_PROFILE_STORAGE=1 bash scripts/cross_db_bench.sh --system nervusdb --medium
```

## Completion Evidence

Record commits, validation output, medium benchmark artifact, and any remaining
profiled bottleneck before moving this plan to `docs/plans/completed/`.
