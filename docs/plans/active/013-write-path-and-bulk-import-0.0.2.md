# 013 Write Path And Bulk Import 0.0.2

## Status

Active.

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
   - Add stage timing fields to `nervusdb/examples/bench_v2.rs`.
   - Keep the final benchmark output as one JSON line.
   - Fix `scripts/core_bench.sh` artifact names so explicit custom runs are not
     mislabeled as `small`.
2. Write staging:
   - Replace edge staging with a cheaper internal representation if profiling
     shows `create_edges` is expensive.
   - Default internal change: stage edges in `Vec<EdgeKey>`, then
     `sort_unstable()` and `dedup()` at commit.
   - Preserve duplicate-edge semantics.
3. Evidence:
   - Run small benchmark after changes.
   - Run 100k/500k benchmark before considering 0.0.2 complete.
   - Record 0.0.1 vs 0.0.2 results here and in `PROGRESS.md`.

## Acceptance

- `cargo fmt --all -- --check`
- `cargo check -p nervusdb --examples`
- `cargo test -p nervusdb-storage --test core_0_1_storage`
- `cargo test -p nervusdb --test core_0_1_rust_api`
- `cargo test -p nervusdb --test core_0_1_mini_cypher`
- `bash scripts/core_bench.sh --small`
- 100k/500k benchmark reaches the 2x target before 0.0.2 release.

## Current Evidence

Pending.
