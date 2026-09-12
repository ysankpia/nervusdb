# Roadmap

GraphLite-RS is an embedded, single-file property graph database: the SQLite
model applied to graphs. This document tracks what is done, what is next, and
what is explicitly out of scope. It is a living plan, not a promise.

Guiding constraint for every item: **report accurately and never trade
correctness for benchmark numbers.** Where a figure is quoted, its measurement
conditions are quoted with it.

---

## Current state (v1.0.0, stable)

Working and covered by tests:

- **Zero runtime dependencies.** The core library pulls in no third-party crates;
  `tests/zero_dependency_tests.rs` enforces it, including a negative test. The
  on-disk format's bytes are defined and implemented in this repository
  (`src/codec.rs`, `src/json.rs`, `src/crc32.rs`) and specified byte by byte in
  `FORMAT.md`.
- **Frozen format (version 4).** The stability promise and the two permanent
  limits (64 GiB file, 24-bit property pointers) are documented with the reasoning;
  both limits are enforced rather than assumed. Older versions are refused before
  any write, including WAL replay.
- Pure disk-backed storage: one data file plus one page-level WAL, fixed-size
  records (32B nodes, 64B edges), O(1) physical addressing, disk-native
  index-free adjacency.
- Slotted property pages: variable-length payloads packed several per 4KB page,
  with 1KB inline/overflow routing and a compact varint codec.
- ACID: explicit transactions, single-fsync group commit, STEAL spilling to WAL
  under constrained memory, crash recovery, exact rollback with zero main-file
  pollution.
- **Concurrency: any number of readers with one writer.** Read-only handles take a
  shared lock; a write handle excludes everyone. Read-only open refuses when the
  WAL still holds unreplayed pages rather than returning stale data.
- Cypher: `CREATE`, `MATCH` (multi-pattern), `MERGE`, `UNWIND`, `WHERE`, `SET`,
  `DELETE` / `DETACH DELETE`, `ORDER BY`, `SKIP`, `LIMIT`,
  `count/sum/avg/min/max`, variable-length and undirected paths, label
  predicates, scalar functions (`id`, `labels`, `type`), and `EXPLAIN`.
- **Query performance**: start-node selection walks adjacency chains instead of
  expanding the whole graph; `LIMIT` is pushed into matching when it cannot change
  the result. A 32,000-edge expansion went from 48 s to 27 ms.
- **Unique constraints** on `(label, property)`, persisted, enforced on insert and
  update, and refused over pre-existing duplicates.
- Analytics: BFS, Dijkstra, cycle detection, PageRank, weakly connected
  components, K-hop subgraph extraction.
- Production safety: page-level CRC32 over the whole data file with self-checked
  directory pages, structural integrity check with a degree-conservation oracle
  that names corrupt pages, error-preserving read accessors, poison-recovering
  locks, WAL auto-checkpoint.
- Operations: `backup()` for a consistent online copy, `vacuum()` for a space
  report. Inspection is through the library API — there is no separate CLI or GUI.
- Tooling: Python and Node.js SDKs with transaction and batch-write support.
  Inspection and dump go through the library API — the CLI and the browser
  workbench were removed in 1.1.0.
- 177 test cases across 15 suites (176 run, 1 intentionally `#[ignore]`d for a
  child-process lock probe); `cargo fmt`, `cargo clippy -D warnings` and
  `rustdoc -D warnings` all clean.

---

## Next (planned)

### 1. Non-blocking readers (versioned page visibility)

**Partially done in 1.1.0.** `GraphLite::read_snapshot()` now gives a caller a
self-consistent view: it holds the shared read lock for its lifetime, so a
multi-step traversal cannot stitch two states together. That closed the correctness
gap (see `tests/concurrency_isolation_tests.rs`).

What remains is the *performance* gap: a snapshot blocks writers while it lives,
because a reader and a writer still exclude each other. Removing that needs
versioned page visibility — readers pin a snapshot (typically by reading from the WAL
up to a known commit point) while the writer appends. That is a substantial change to
recovery and page visibility, and it is the largest remaining gap against the
"agent writes while you watch" workload.

### 2. Planner memory beyond edges

A transaction still queues all its actions in memory before commit. Edge batches
are chunked at `MAX_BATCH_EDGES_IN_MEMORY`, but a multi-million-**node** transaction
still holds the whole action list. Capping and spilling the planner queue itself is
the remaining step.

### 3. Cost-based query planning

Start-node selection is rule-based (index when available, otherwise a scan) and
`LIMIT` push-down is decided by a fixed safety check. There is no cost model, no
join reordering and no index-nested-loop selection. Adequate at the current scale;
a limitation for complex analytical queries.

### 4. SDK publication

The Python and Node.js bindings build and pass their tests but are not published to
PyPI or npm. Publishing needs packaging polish, versioning policy, and platform
wheel/prebuild matrices.

**The previously recorded "9x slower than native" figure is retracted** — it came
from debug builds of the bindings compared against a release core. Measured with
both sides in release the bindings run at 0.85-0.93x of the native path. See
`docs/benchmarks.md` for the corrected table and the retraction.

### 5. Concurrency stress at high core counts

The current suite exercises 20 threads. Behaviour under sustained load on
many-core machines, and the contention profile of the page latches, are not yet
characterised.

## Explicitly out of scope

- **Distributed or multi-node operation.** The design premise is a single
  embedded file, like SQLite.
- **In-memory graph mode.** A resident full-graph representation would violate
  the bounded-memory invariant that the whole architecture is built around.
- **Cypher `WITH`, subqueries, stored procedures.** `MERGE` and `UNWIND` landed in
  1.1.0; the rest each need design work rather than a patch.
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
