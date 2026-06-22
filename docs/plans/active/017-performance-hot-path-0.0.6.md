# 017 Performance Hot Path 0.0.6

## Status

In progress.

## Goal

Make the 0.0.6 performance work evidence-driven: first fix benchmark attribution,
then remove the storage hot-path costs already proven by code inspection.

## Scope

- Keep the cross-database benchmark as the 0.0.6 baseline.
- Split `cross_db_bench` load and reopen timings so numbers are attributable.
- Add env-gated internal storage profiling with no public API change.
- Remove `idx_node_props` cleanup paths that scan the whole property index for a
  single node or property.
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
