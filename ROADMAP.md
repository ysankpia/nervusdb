# Roadmap

GraphLite-RS is an embedded, single-file property graph database: the SQLite
model applied to graphs. This document tracks what is done, what is next, and
what is explicitly out of scope. It is a living plan, not a promise.

Guiding constraint for every item: **report accurately and never trade
correctness for benchmark numbers.** Where a figure is quoted, its measurement
conditions are quoted with it.

---

## Current state (v1.0.0-rc.2)

Working and covered by tests:

- Pure disk-backed storage: one data file plus one page-level WAL, fixed-size
  records (32B nodes, 64B edges), O(1) physical addressing, disk-native
  index-free adjacency.
- Slotted property pages: variable-length payloads packed several per 4KB page,
  with 1KB inline/overflow routing and a compact varint codec.
- ACID: explicit transactions, single-fsync group commit, STEAL spilling to WAL
  under constrained memory, crash recovery, exact rollback with zero main-file
  pollution.
- Cypher 1.0: `CREATE`, `MATCH` (multi-pattern), `WHERE`, `SET`, `DELETE` /
  `DETACH DELETE`, `ORDER BY`, `SKIP`, `LIMIT`, `count/sum/avg/min/max`,
  variable-length and undirected paths, label predicates.
- Analytics: BFS, Dijkstra, cycle detection, PageRank, weakly connected
  components, K-hop subgraph extraction.
- Production safety: exclusive single-writer open lock, page-level CRC32 over
  the whole data file with self-checked directory pages, structural integrity
  check with a degree-conservation oracle that names corrupt pages,
  error-preserving read accessors, poison-recovering locks, WAL auto-checkpoint.
- Tooling: interactive CLI with dot commands and logical dump; Python and
  Node.js SDKs with transaction support.
- 100 test cases across 10 suites (99 run, 1 intentionally `#[ignore]`d for a
  child-process lock probe); `cargo fmt`, `cargo clippy -D warnings` and
  `rustdoc -D warnings` all clean.

---

## Next (planned)

### 1. Multi-reader concurrency

Today exactly one handle may open a database. A shared-lock read-only mode would
let multiple readers coexist with one writer, matching SQLite's model. Blocked
on: WAL replay writes the main file, so read-only open must first establish that
the WAL has nothing committed to apply.

### 2. Quadratic edge expansion in `MATCH`

**Measured, not theoretical.** Expanding a pattern whose source is constrained by
`WHERE id(a) = N` costs O(edges²):

```text
edges      1k      2k      8k     32k
elapsed   117ms   369ms  7.1s   127s
```

The same query without the `WHERE` is also quadratic (2k → 119ms, 32k → 42.6s),
and `LIMIT 1` does not help — it expands everything before applying the limit.
Both effects were reproduced identically on the `v1.0.0-rc.1` tag, so this is
pre-existing, not a regression.

The fix is start-node selection: when a pattern's node is pinned by an id or a
label/property predicate, the expansion should begin at that node and walk its
adjacency chain, instead of expanding every edge and then filtering. `LIMIT`
should also short-circuit rather than materialise the full result set. This is
the highest-value query-planner item and is a prerequisite for usable
interactive queries on large graphs.

### 3. Maintenance operations

`vacuum` (reclaim space after mass deletion) and `backup` (consistent copy while
open). Both are currently absent; the logical dump path covers the migration use
case but not operational hygiene.

### 4. Planner memory beyond edges

A single transaction still queues **all** its actions in memory before commit; a
multi-million-_node_ transaction holds the whole action list even though edge
weaving is now chunked at `MAX_BATCH_EDGES_IN_MEMORY`. Capping and spilling the
planner queue itself is the remaining step.

### 5. Cost-based query planning

Secondary indexes are used for start-node selection only. There is no cost-based
planning, no join reordering, no index-nested-loop selection. Adequate for the
current scope; a limitation for complex analytical queries at scale.

### 6. SDK publication

The Python and Node.js bindings build and pass their tests but are not published
to PyPI or npm. Publishing needs packaging polish, versioning policy and
platform wheel/prebuild matrices.

**Correction (this revision): the previously recorded "9x slower than native"
figure was a measurement artifact, not a real gap.** Those numbers came from a
*debug* build of the bindings compared against a *release* build of the core.
Measured with both sides in release, on the same 50,000-node workload:

| Path | Throughput |
| --- | --- |
| Rust, file-backed | 382,000 ops/s |
| Python, file-backed | 355,000 ops/s |
| Node, file-backed | 326,000 ops/s |

The bindings are at 0.85–0.93x of the native path, not 0.11x. The debug/release
delta is 6.4x on an identical script, which is what the old figure was actually
measuring.

This also means the batch API (`Transaction::add_nodes` / `add_edges`, exposed as
`tx.add_nodes(...)` in both SDKs) is **not** a large performance win — the FFI
boundary was never the bottleneck. It is kept because one call per batch is a
better shape than N calls, but the honest framing is ergonomics and lock traffic,
not throughput. Where it does help is flattening per-record fixed cost when a
batch is built up in a loop; measured effect on the standard benchmark is within
run-to-run noise.

Remaining for publication: packaging, versioning policy, and platform
wheel/prebuild matrices — unchanged.

### 7. Concurrency stress at high core counts

The current suite exercises 20 threads. Behaviour under sustained load on
many-core machines, and the contention profile of the page latches, are not yet
characterised.

---

## Explicitly out of scope

- **Distributed or multi-node operation.** The design premise is a single
  embedded file, like SQLite.
- **In-memory graph mode.** A resident full-graph representation would violate
  the bounded-memory invariant that the whole architecture is built around.
- **Cypher `MERGE`, `UNWIND`, `WITH`, subqueries, stored procedures.**
  Interesting, but each needs design work rather than a patch.
- **Full-text or vector indexes.** Separate problem domain.

---

## How to contribute a performance claim

If you want to add or change a performance number in the documentation:

1. Add or extend a scenario in `benches/throughput.rs` (or the SDK benchmarks).
   The scenario must print its own configuration alongside the result.
2. Run it on a machine you can describe (CPU, storage, OS).
3. Quote the number **together with** that configuration, and state whether the
   run was file-backed or in memory.

A number without its measurement conditions is not accepted, because we have
already had to correct figures that could not be reproduced.
