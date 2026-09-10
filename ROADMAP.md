# Roadmap

GraphLite-RS is an embedded, single-file property graph database: the SQLite
model applied to graphs. This document tracks what is done, what is next, and
what is explicitly out of scope. It is a living plan, not a promise.

Guiding constraint for every item: **report accurately and never trade
correctness for benchmark numbers.** Where a figure is quoted, its measurement
conditions are quoted with it.

---

## Current state (v1.0.0)

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
- Production safety: exclusive single-writer open lock, structural integrity
  check with a degree-conservation oracle, error-preserving read accessors,
  poison-recovering locks.
- Tooling: interactive CLI with dot commands and logical dump; Python and
  Node.js SDKs with transaction support.
- 92 tests across 9 suites; `cargo clippy -D warnings` clean.

---

## Next (planned)

### 1. Multi-reader concurrency

Today exactly one handle may open a database. A shared-lock read-only mode would
let multiple readers coexist with one writer, matching SQLite's model. Blocked
on: WAL replay writes the main file, so read-only open must first establish that
the WAL has nothing committed to apply.

### 2. Page checksums on the main data file

`integrity_check` currently validates structure, not page content. A checksum
per data page would catch bit rot that leaves structure self-consistent. This is
a storage format change (version 3) and therefore a breaking change, so it needs
its own release.

### 3. Maintenance operations

`vacuum` (reclaim space after mass deletion) and `backup` (consistent copy while
open). Both are currently absent; the logical dump path covers the migration use
case but not operational hygiene.

### 4. Bounded-memory batch planning

A single transaction queues all its actions in memory before commit; a
multi-million-edge transaction therefore holds hundreds of megabytes and shows a
measurable throughput inversion at very large batch sizes. Chunked transactions
already avoid this, but the kernel should cap planner memory and spill planning
state rather than relying on callers to chunk.

### 5. Query planner work

Secondary indexes are used for start-node selection only. There is no cost-based
planning, no join reordering, no index-nested-loop selection. Adequate for the
current scope; a limitation for complex analytical queries at scale.

### 6. SDK publication

The Python and Node.js bindings build and pass their tests but are not published
to PyPI or npm. Publishing needs packaging polish, versioning policy and
platform wheel/prebuild matrices.

Their throughput is also bounded by the one-call-per-write FFI boundary
(measured ~63k ops/s in Python and ~64k in Node, against ~550k for the native
Rust path at the same scale). A batch API that accepts an array of entities per
call would close most of that gap; that is the higher-value change and should
land before publication.

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
