# ADR 0010: Packed Adjacency Lists

## Status

Accepted for 0.0.8.

## Context

0.0.7 fixed raw reopen and disk footprint by moving to four Fjall keyspaces:

```text
meta
graph_data
adj_out
adj_in
```

It did not fix traversal. Medium cross-database evidence showed two-hop
throughput regressed from the 0.0.6 reference even though `adj_out` and
`adj_in` stayed in dedicated keyspaces.

The important source-level fact is in Fjall / lsm-tree itself: `prefix` and
`range` are LSM merge iterators. Opening a prefix iterator constructs iterators
over overlapping tables, sealed memtables, and the active memtable. That is a
reasonable KV API, but it is the wrong unit for graph workloads where a common
operation is:

```text
get all neighbors for one (node, rel)
```

In the medium benchmark, each adjacency scan usually returns about five edges.
Paying LSM iterator setup for every five-edge scan is too expensive. The
problem is not keyspace count anymore; the problem is edge record granularity.

Durable bulk commit has the same shape. One KV item per edge direction writes
about one million adjacency items for 500k logical edges before counting nodes,
properties, labels, and indexes.

After packed adjacency was implemented, a second source-level fact became
visible in Fjall 3.1.5: clean reopen can still be slow when the active journal
has not rotated. Fjall's worker rotates the journal after the active writer
passes about `64_000_000` bytes. Packed adjacency reduces bytes enough that a
medium bulk load can remain below that threshold, so normal reopen may replay
the active journal unless NervusDB performs an explicit clean-close checkpoint.

## Decision

0.0.8 bumps `STORAGE_FORMAT_EPOCH` from `3` to `4` and changes adjacency
records from per-edge keys to packed, sorted adjacency-list values:

```text
adj_out [src:u32][rel:u32] -> repeated dst:u32 BE
adj_in  [dst:u32][rel:u32] -> repeated src:u32 BE
```

Public graph semantics do not change:

- Edge identity remains `(src, rel, dst)`.
- Parallel edges remain unsupported.
- Recreating the same edge remains idempotent.
- `neighbors(src, Some(rel))` returns the same logical `EdgeKey` values.
- `incoming_neighbors(dst, Some(rel))` returns the same logical `EdgeKey`
  values.
- `tombstone_node` remains detach-clean.
- Fsck-lite still checks `adj_out` / `adj_in` symmetry.

The common rel-qualified traversal path becomes a point read of one adjacency
list. The rel-unqualified path still prefix scans all relationship lists for a
node and expands each list.

Commit groups created and deleted edges by `(src, rel)` and `(dst, rel)`, reads
each existing list at most once, mutates it in memory, then writes/removes the
packed list record in the same durable Fjall batch.

`Db::close(self)` also becomes a real clean-shutdown path: it persists with
`SyncAll`, flushes the four keyspaces, drops the Fjall handle, then truncates
only the active clean-shutdown journal and fsyncs the file and directory. This
does not change normal commit semantics and does not run during crash recovery.

## Consequences

The good part: the storage layout now matches the graph operation. Two-hop
traversal avoids thousands of tiny LSM prefix iterators, and bulk commit writes
far fewer adjacency KV items.

Clean reopen stops paying active-journal replay after explicit `Db::close()`.
This is intentionally documented as NervusDB's close-time behavior over the
current Fjall backend, not as a public byte-level Fjall file-format contract.

The cost: updating or deleting a single edge rewrites one small adjacency list
in each direction instead of removing one tiny KV key in each direction. That is
the correct tradeoff for NervusDB's current target: embedded local graph
memory, Agent Memory, and modest-degree property graphs.

High-degree nodes create larger adjacency-list values. That is acceptable for
0.0.8. If downstream projects expose very high-degree hotspots, the next design
should shard lists by `(node, rel, bucket)` rather than reverting to one key per
edge.

Epoch 3 database directories are not readable by epoch 4. This is an
intentional pre-0.1 break. No migration tool is provided.

## Non-Goals

- No public storage options.
- No unsafe durability mode.
- No public bulk loader.
- No EdgeId or parallel edges.
- No vector/HNSW, range index, multi-writer, or full Cypher work.
- No SQLite-style single-file storage.

## Validation

Minimum correctness:

```bash
cargo check -p nervusdb --examples
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --features unstable-admin admin::tests
cargo test -p nervusdb --test core_0_1_agent_memory --features unstable-admin
```

Minimum performance evidence:

```bash
bash scripts/cross_db_bench.sh --system nervusdb --medium
NERVUSDB_PROFILE_STORAGE=1 bash scripts/cross_db_bench.sh --system nervusdb --medium
```

0.0.8 release-candidate evidence:

```text
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-183205.ndjson
correctness_hash: d4b70801ad0bb15b
raw reopen: 2.059ms
two-hop: 4,905,668.123 paths/s
disk footprint: 29,425,660 bytes
```

Profile evidence shows the remaining durable commit cost is dominated by
Fjall `SyncAll` batch persistence:

```text
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-183217.ndjson
WriteTxn::commit.batch_commit: 1,072.110ms
GraphEngine::close.checkpoint_journal: 7.850ms
```
