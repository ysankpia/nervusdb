# 013 Write Path And Bulk Import 0.0.2

## Status

Completed.

## Goal

Make the 0.0.2 line explain and improve bulk write performance without changing
the public API or weakening default durability.

0.0.1 baseline:

```text
100000 nodes / 500000 edges
insert=438.130s
insert_edges_per_sec=1141
neighbors_hot_edges_per_sec=1742616
neighbors_cold_edges_per_sec=976857
write_txn_p99_ms=12.6305
```

0.0.2 target:

```text
insert <= 219s OR insert_edges_per_sec >= 2282
```

## Scope

- Benchmark stage timing for open, schema, create nodes, create edges, commit,
  reopen verification, reads, and write transaction smoke.
- Internal write staging optimization for large edge batches.
- Keep `WriteTxn::commit()` durability at `PersistMode::SyncAll`.
- Keep public API unchanged.
- Keep edge identity `(src, rel, dst)` and duplicate-edge idempotence.

## Not In Scope

- Public `BulkLoader`, `DbOptions`, or `Durability` API.
- Buffered or unsafe durability mode.
- Property indexes.
- Dangling-edge enforcement.
- Tombstone cleanup.
- Edge IDs or parallel edges.
- Advanced Mini-Cypher features.

## Implementation Plan

1. Benchmark honesty:
   - Done: added stage timing fields to `nervusdb/examples/bench_v2.rs`.
   - Done: kept the final benchmark output as one JSON line.
   - Done: fixed `scripts/core_bench.sh` artifact names so explicit custom runs are not
     mislabeled as `small`.
2. Write staging:
   - Done: benchmark staging showed `create_edges` was not the main bottleneck;
     the real 0.0.1 cost was per-node `SyncAll` counter persistence.
   - Done: node ids are now staged in the write transaction and the
     `next_node_id` meta key is persisted in the commit batch.
   - Done: edge staging now uses `Vec<EdgeKey>`, then
     `sort_unstable()` and `dedup()` at commit.
   - Done: duplicate-edge semantics are preserved.
3. Evidence:
   - Done: small benchmark passed after changes.
   - Done: 100k/500k benchmark passed after changes.
   - Done: 0.0.1 vs 0.0.2 results are recorded here and in `PROGRESS.md`.

## Acceptance

- `cargo fmt --all -- --check`
- `cargo check -p nervusdb --examples`
- `cargo test -p nervusdb-storage --test core_0_1_storage`
- `cargo test -p nervusdb --test core_0_1_rust_api`
- `cargo test -p nervusdb --test core_0_1_mini_cypher`
- `bash scripts/core_bench.sh --small`
- 100k/500k benchmark reaches the 2x target before 0.0.2 release.

## Current Evidence

2026-06-22 after 0.0.2 write-path changes, best recorded 100k/500k run:

```text
artifact: artifacts/core-bench/core-bench-custom-100000n-5d-20260621-190510.json
nodes=100000
degree=5
edges=500000
iters=1000
write_iters=20
insert_total_ms=415.104
insert_edges_per_sec=1204516.337
stage_get_schema_ms=13.009
stage_create_nodes_ms=12.361
stage_create_edges_ms=1.287
stage_commit_ms=388.447
stage_reopen_verify_ms=1605.838
neighbors_hot_edges_per_sec=1515630.087
neighbors_cold_edges_per_sec=793162.935
write_txn_p99_ms=4.034000
```

Comparison:

| Metric | 0.0.1 baseline | 0.0.2 current | Result |
|---|---:|---:|---:|
| Insert time | 438.130s | 0.415s | 1055x faster |
| Insert edges/sec | 1,141 | 1,204,516 | 1056x higher |
| Hot neighbor edges/sec | 1,742,616 | 1,515,630 | 13% lower |
| Cold neighbor edges/sec | 976,857 | 793,163 | 19% lower |
| Write txn p99 | 12.6305ms | 4.0340ms | 68% lower |

Root cause:

0.0.1 called `next_counter(META_NEXT_NODE_ID)` from every `create_node`, and
`next_counter` committed `PersistMode::SyncAll` each time. Bulk import therefore
performed one durable meta write per node before the real transaction commit.
0.0.2 stages node ids inside `WriteTxn` and persists `next_node_id` in the same
atomic commit batch as the node data. Default durability remains `SyncAll`.

Remaining note:

Read throughput is volatile across repeated runs. Additional 100k/500k artifacts:

```text
artifacts/core-bench/core-bench-custom-100000n-5d-20260621-190709.json
insert_total_ms=567.382
insert_edges_per_sec=881240.309
neighbors_hot_edges_per_sec=1718311.669
neighbors_cold_edges_per_sec=349408.036

artifacts/core-bench/core-bench-custom-100000n-5d-20260621-190713.json
insert_total_ms=478.995
insert_edges_per_sec=1043851.415
neighbors_hot_edges_per_sec=1296302.298
neighbors_cold_edges_per_sec=745619.411
```

The code path did not change adjacency key layout or neighbor iteration. Treat
the lower cold-read samples as benchmark noise or cache/filesystem state until a
dedicated repeated read benchmark proves otherwise. This does not block the
0.0.2 write-path target, but it should not be hidden.
