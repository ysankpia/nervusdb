# 018 Storage Layout 0.0.7

## Status

Active implementation. ADR 0009 accepts a destructive epoch 3 storage layout:
`meta + graph_data + adj_out + adj_in` physical Fjall keyspaces. General graph
records are tagged in `graph_data`; adjacency keeps dedicated hot keyspaces.

## Goal

Attack the remaining large 0.0.6 performance costs: durable bulk commit, raw
database reopen, file count, and storage footprint.

The target is not more graph features. The target is a simpler physical layout
that behaves more like an embedded database users can carry around without
paying unnecessary multi-keyspace overhead.

## Evidence From 0.0.6

0.0.6 removed the easy hot-path bugs:

- old full `idx_node_props` cleanup scans are gone,
- adjacency scans stream directly,
- traversal no longer performs redundant per-edge node liveness reads,
- bulk property-index writes no longer linearly search `created_nodes` for every
  staged property.

Current medium evidence:

```text
command: bash scripts/cross_db_bench.sh --system nervusdb --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-120945.ndjson
dataset: 100,000 nodes / 500,000 edges
```

| Metric | Current |
|---|---:|
| load total | 1,674.287 ms |
| durable commit | 1,570.171 ms |
| raw reopen | 3,249.434 ms |
| count verify after reopen | 105.118 ms |
| disk footprint | 84,595,889 bytes / 31 files |

Profile evidence:

```text
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-120913.ndjson
```

`WriteTxn::commit.property_index_writes` is down to `244.979 ms`; it is no
longer the main bottleneck. `batch.commit` and raw `GraphEngine::open.database`
are now the controlling costs.

## Accepted Direction

Collapse the 0.0.6 many-keyspace physical layout into four Fjall keyspaces:

```text
meta
graph_data
adj_out
adj_in
```

`meta` stores only `format_epoch` and ID counters. `graph_data` stores
non-adjacency graph records, derived indexes, name mappings, and properties
with a one byte logical tag. `adj_out` and `adj_in` keep raw adjacency keys for
traversal locality.

```text
0x01 NODE             [tag][iid:u32] -> encode_node_value(external_id, flags)
0x02 EXT2NODE         [tag][external_id:u64] -> iid:u32
0x10 LABEL_NAME       [tag][name_len:u16][name_bytes] -> label_id:u32
0x11 LABEL_ID         [tag][label_id:u32] -> name_bytes
0x12 REL_NAME         [tag][name_len:u16][name_bytes] -> rel_id:u32
0x13 REL_ID           [tag][rel_id:u32] -> name_bytes
0x20 NODE_LABEL       [tag][iid:u32][label_id:u32] -> empty
0x21 LABEL_NODE       [tag][label_id:u32][iid:u32] -> empty
0x40 NODE_PROP        [tag][iid:u32][key_len:u32][key_bytes] -> PropertyValue::encode()
0x41 EDGE_PROP        [tag][src:u32][rel:u32][dst:u32][key_len:u32][key_bytes] -> PropertyValue::encode()
0x50 NODE_PROP_INDEX  [tag][label_id:u32][key_len:u16][key_bytes][value_len:u32][value_bytes][iid:u32] -> empty
```

Adjacency keyspaces:

```text
adj_out [src:u32][rel:u32][dst:u32] -> empty
adj_in  [dst:u32][rel:u32][src:u32] -> empty
```

`STORAGE_FORMAT_EPOCH` is bumped from `2` to `3`. Epoch 2 databases are rejected
with `StorageFormatMismatch`; no migration is provided before 0.1.

## ADR Answers

- Destructive format change is accepted because 0.x has no stable disk
  compatibility promise.
- No old-format migration is included; users export/reimport or recreate data.
- Prefix locality is preserved by tag plus big-endian integer ordering for
  `graph_data`, and by raw big-endian adjacency keys for `adj_out` / `adj_in`.
- fsck-lite scans tag prefixes plus adjacency keyspaces and still only repairs
  derived `LABEL_NODE` and `NODE_PROP_INDEX` records.
- Crash recovery remains Fjall-backed `PersistMode::SyncAll`; graph-level
  crash validation is still required.
- Performance completion requires medium benchmark evidence, not just a passing
  test suite.

## Acceptance

Before implementation:

- ADR accepted: `docs/decisions/0009-storage-keyspace-consolidation.md`.
- Current storage format documented as old epoch 2.
- New epoch 3 key layout documented with byte-level tags and prefix scan ranges.
- Destructive-reset decision documented.

Implementation target:

- Public Rust API unchanged.
- Default durability remains `SyncAll`.
- Mini-Cypher behavior unchanged.
- Fsck-lite still works or is explicitly updated with tests.
- Fresh epoch 3 databases create only `meta`, `graph_data`, `adj_out`, and
  `adj_in` logical Fjall keyspaces.
- Synthetic epoch 2 databases fail fast with `StorageFormatMismatch`.
- Medium benchmark correctness hash remains stable.
- Current property lookup and traversal wins do not regress materially.
- Raw reopen and durable commit improve enough to justify the storage format
  change.

## Current Implementation Evidence

Correctness checks currently pass:

```text
cargo fmt --all -- --check
cargo check -p nervusdb --examples
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_mini_cypher
cargo test -p nervusdb-cli
cargo test -p nervusdb --test core_0_1_agent_memory
cargo test -p nervusdb --features unstable-admin --test core_0_1_agent_memory
bash scripts/check.sh
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
```

Medium performance evidence after moving adjacency into separate keyspaces:

```text
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-150028.ndjson
correctness_hash: d4b70801ad0bb15b
```

| Metric | 0.0.6 baseline | 0.0.7 current | Target | Status |
|---|---:|---:|---:|---|
| durable commit | 1,570.171 ms | 1,476.589 ms | <= 1,200 ms | miss |
| raw reopen | 3,249.434 ms | 3.185 ms | <= 1,500 ms | pass |
| file count | 31 | 24 | <= 16 | miss |
| disk footprint | 84,595,889 bytes | 38,315,826 bytes | lower is better | pass |
| two-hop traversal | 3,356,928 paths/s | 1,810,341 paths/s | >= 2,500,000 | miss |
| property lookup p99 | microsecond-class | 7.167 us | microsecond-class | pass |

Conclusion: the first storage-layout cut is correct and materially reduces file
count and disk footprint. Clean `Db::close()` now flushes keyspaces and solves
raw reopen. The 4-keyspace adjacency split did not restore traversal and
regressed file count. Do not publish this as a completed storage-layout release
under the original success line.

Current decision point: either re-scope 0.0.7 as a clean-reopen/footprint
release with explicit acceptance changes, or stop this storage-layout branch
and investigate traversal/commit with a separate benchmark-first plan. More
keyspace reshuffling is not justified by the current evidence.

## Non-Goals

- New graph features.
- Full Cypher.
- EdgeId or parallel edges.
- Vector/HNSW.
- Multi-writer concurrency.
- Unsafe durability modes.
- Public storage-format compatibility promises before 0.1.
