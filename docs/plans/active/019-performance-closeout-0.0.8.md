# 019 Performance Closeout 0.0.8

## Status

Release-candidate validation.

## Goal

0.0.8 targets the two remaining performance blockers before NervusDB is used as
the default embedded graph store for downstream Agent Memory / local graph
applications:

- durable bulk commit is still about `1.4s` on the 100k node / 500k edge medium
  benchmark,
- traversal regressed in 0.0.7, especially two-hop traversal.

This is not a feature release. The job is to find the owning layer with
evidence, make the smallest correct hot-path fixes, and stop when the benchmark
proves the database is good enough for real downstream use.

## Baseline

0.0.7 medium benchmark:

```text
command: bash scripts/cross_db_bench.sh --system nervusdb --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-150028.ndjson
dataset: 100,000 nodes / 500,000 edges / 10,000 read iterations / 100 mutation iterations
correctness_hash: d4b70801ad0bb15b
```

| Metric | 0.0.7 |
|---|---:|
| load total | 1,575.904 ms |
| durable commit | 1,476.589 ms |
| raw reopen | 3.185 ms |
| count verify after reopen | 118.146 ms |
| property lookup p99 | 7.167 us |
| one-hop hot | 2,370,524.893 edges/s |
| one-hop cold | 1,900,463.238 edges/s |
| incoming cold | 1,888,271.005 edges/s |
| two-hop | 1,810,340.857 paths/s |
| update p99 | 4,053.834 us |
| detach delete p99 | 4,105.291 us |
| disk footprint | 38,315,826 bytes / 24 files |

0.0.6 traversal reference:

```text
two-hop: 3,356,928.783 paths/s
one-hop hot: 5,032,923.878 edges/s
```

## First-Principles Diagnosis

The real goal is not to win a benchmark by changing safety semantics. The real
goal is a boring embedded graph database whose startup, write, and traversal
costs are not surprising in downstream applications.

Primitive constraints:

- normal commits remain `PersistMode::SyncAll`;
- public Rust API remains unchanged;
- storage epoch 4 is allowed before 0.1 because 0.0.7 evidence showed the
  epoch 3 per-edge adjacency layout had the wrong access grain for graph
  traversal;
- correctness hash must remain stable;
- crash recovery and fsck-lite must continue to pass.

Commit can only be slow in one of these layers:

- public API staging and key encoding,
- commit validation / cleanup collection,
- batch operation construction,
- Fjall `batch.commit` / journal persistence / fsync,
- clean-shutdown flush behavior.

Traversal can only be slow in one of these layers:

- Fjall prefix iterator setup over many tiny adjacency scans,
- key decode / allocation,
- iterator boxing or dynamic dispatch through the public facade,
- benchmark loop shape,
- physical locality after epoch 3 layout.

The source-level finding from Fjall 3.1.5 is that every `prefix`/`range`
iterator constructs a merged LSM iterator over overlapping tables, sealed
memtables, and the active memtable. The medium two-hop benchmark performs many
tiny adjacency scans with only about five edges per scan. Paying LSM iterator
setup for each tiny scan is the wrong access grain.

The 0.0.8 fix is therefore not another keyspace-count experiment. It changes
adjacency inside `adj_out` / `adj_in` from one key per edge to one sorted,
packed adjacency-list value per `(node, rel)` pair:

```text
adj_out [src:u32][rel:u32] -> repeated dst:u32 BE
adj_in  [dst:u32][rel:u32] -> repeated src:u32 BE
```

This keeps public API and edge identity unchanged while turning
`neighbors(node, Some(rel))` from a small prefix scan into a point read.

## Implementation Stages

### Stage 1: Evidence

- Run 0.0.7 medium benchmark with `NERVUSDB_PROFILE_STORAGE=1`.
- Preserve the artifact path in `PROGRESS.md`.
- If current profile does not split traversal enough, add env-gated profile
  counters for:
  - outgoing prefix scan count,
  - incoming prefix scan count,
  - decoded edge count,
  - elapsed time per scan direction.
- Do not change storage behavior in this stage.

### Stage 2: Traversal Hot Path

Candidate selected: packed adjacency lists.

Allowed candidates:

- bump `STORAGE_FORMAT_EPOCH` to `4`;
- keep physical keyspaces `meta`, `graph_data`, `adj_out`, and `adj_in`;
- encode outgoing and incoming adjacency as sorted u32 lists;
- keep edge identity `(src, rel, dst)` and duplicate edge idempotency;
- update fsck-lite to expand packed adjacency lists when checking symmetry;
- keep `rel=None` traversal as prefix over adjacency-list records, then expand
  values.

Do not do:

- hide traversal regression by changing benchmark shape;
- remove graph integrity checks from write path;
- reintroduce stale endpoint liveness reads in traversal.

### Stage 3: Durable Commit Hot Path

Candidate selected: reduce adjacency KV item count by writing one adjacency
list per `(node, rel)` instead of one key per edge.

Allowed candidates:

- group created/deleted edges by `(src, rel)` and `(dst, rel)` during commit;
- read each old adjacency list at most once per affected group;
- write/remove packed list records in the same durable Fjall batch;
- preserve `SyncAll` and do not introduce public bulk/durability options.

Do not do:

- switch normal commit to buffered durability;
- add public bulk-loader or durability knobs;
- claim success if the remaining time is unambiguously storage-engine fsync
  floor.

### Stage 4: Clean Reopen Checkpoint

The packed adjacency change fixed traversal, but it exposed a different clean
reopen cost. After bulk load, the packed layout writes far fewer bytes, so
Fjall's active journal can remain below its internal rotation threshold. A clean
reopen then pays active-journal replay even though NervusDB already durably
committed and closed the handle.

Source-level finding from Fjall 3.1.5:

```text
worker_pool.rs rotates the active journal only when journal_writer.pos()? > 64_000_000
rotate_memtable_and_wait() flushes keyspaces but does not force active journal rotation
```

The 0.0.8 fix is deliberately narrow:

- `Db::close(self)` consumes the handle.
- It calls `PersistMode::SyncAll`.
- It rotates and waits for `meta`, `graph_data`, `adj_out`, and `adj_in`.
- It drops the Fjall database handle.
- It truncates only the active `.jnl` file to zero and fsyncs the file and
  directory.

This is a clean-shutdown optimization, not a weakened durability mode. Normal
`WriteTxn::commit()` still uses `SyncAll`. Crash recovery still relies on
Fjall's journal; the active journal is touched only after an explicit clean
close and keyspace flush.

## Result Evidence

Clean 0.0.8 medium benchmark:

```text
command: bash scripts/cross_db_bench.sh --system nervusdb --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-183205.ndjson
correctness_hash: d4b70801ad0bb15b
```

| Metric | 0.0.7 baseline | 0.0.8 RC |
|---|---:|---:|
| load total | 1,575.904 ms | 1,448.574 ms |
| durable commit | 1,476.589 ms | 1,328.054 ms |
| raw reopen | 3.185 ms | 2.059 ms |
| count verify after reopen | 118.146 ms | 82.373 ms |
| property lookup p99 | 7.167 us | 3.334 us |
| one-hop hot | 2,370,524.893 edges/s | 7,761,011.750 edges/s |
| one-hop cold | 1,900,463.238 edges/s | 5,402,095.819 edges/s |
| incoming cold | 1,888,271.005 edges/s | 5,053,950.926 edges/s |
| two-hop | 1,810,340.857 paths/s | 4,905,668.123 paths/s |
| update p99 | 4,053.834 us | 4,967.750 us |
| detach delete p99 | 4,105.291 us | 5,043.875 us |
| disk footprint | 38,315,826 bytes | 29,425,660 bytes |
| file count | 24 | 24 |

Profile evidence:

```text
command: NERVUSDB_PROFILE_STORAGE=1 bash scripts/cross_db_bench.sh --system nervusdb --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-183217.ndjson
```

Key profile lines:

```text
WriteTxn::commit.validation             19.868ms
WriteTxn::commit.created_node_writes    20.404ms
WriteTxn::commit.edge_writes            73.580ms
WriteTxn::commit.property_index_writes  241.319ms
WriteTxn::commit.batch_commit           1,072.110ms
GraphEngine::close.flush_keyspaces      143.162ms
GraphEngine::close.checkpoint_journal   7.850ms
GraphEngine::open.database after close  2.283ms
```

Conclusion: traversal and clean reopen are fixed. Durable commit improved
modestly because packed adjacency reduces adjacency KV writes, but the remaining
large cost is Fjall `SyncAll` batch persistence. 0.0.8 closes that issue with
evidence rather than hiding it behind buffered durability.

## Acceptance

Minimum correctness:

```bash
cargo fmt --all -- --check
cargo check -p nervusdb --examples
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_mini_cypher
cargo test -p nervusdb-cli
cargo test -p nervusdb --test core_0_1_agent_memory
cargo test -p nervusdb --features unstable-admin --test core_0_1_agent_memory
bash scripts/check.sh
bash scripts/core_crash_recovery.sh
```

Performance acceptance:

```bash
bash scripts/cross_db_bench.sh --system nervusdb --medium
NERVUSDB_PROFILE_STORAGE=1 bash scripts/cross_db_bench.sh --system nervusdb --medium
```

Target line:

- correctness hash remains `d4b70801ad0bb15b`;
- raw reopen remains millisecond-class;
- disk footprint does not materially regress from 0.0.7;
- two-hop traversal returns to at least `3.0M paths/s`, or profile proves the
  remaining limit is outside NervusDB's current safe optimization layer;
- durable commit drops below `1.0s`, or profile proves `batch.commit`/fsync is
  the hard floor under `SyncAll`.

0.0.8 is complete only if both remaining issues are either fixed or explicitly
closed with hard evidence. No vague "probably Fjall" conclusion is acceptable.

## Non-Goals

- No new graph features.
- No range index.
- No EdgeId or parallel edges.
- No vector/HNSW work.
- No public storage options.
- No unsafe durability mode.
- No public bulk import API.
- No single-file storage rewrite.
- No C rewrite.
- No full Cypher expansion.
