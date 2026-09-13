# Roadmap

NervusDB is an embedded, single-file property graph database: the SQLite
model applied to graphs. This document tracks what is done, what is next, and
what is explicitly out of scope. It is a living plan, not a promise.

Guiding constraint for every item: **report accurately and never trade
correctness for benchmark numbers.** Where a figure is quoted, its measurement
conditions are quoted with it.

---

## Current state (v0.1.0)

Working and covered by tests:

- **Zero runtime dependencies.** The core library pulls in no third-party crates;
  `tests/zero_dependency_tests.rs` enforces it, including a negative test. The
  on-disk format's bytes are defined and implemented in this repository
  (`src/codec.rs`, `src/json.rs`, `src/crc32.rs`) and specified byte by byte in
  `FORMAT.md`.
- **Frozen format (version 5).** The stability promise and the two permanent
  limits (64 GiB file, 24-bit property pointers) are documented with the reasoning;
  both limits are enforced rather than assumed. Older versions are refused before
  any write, including WAL replay. Version 5 changed only the Page 0 magic
  (`GLDB` → `NVDB`) when the project was renamed — no page layout moved.
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
  workbench were removed before 0.1.0.
- 237 test cases across 20 suites (236 run, 1 intentionally `#[ignore]`d for a
  child-process lock probe); `cargo fmt`, `cargo clippy -D warnings` and
  `rustdoc -D warnings` all clean.

---

## Next (planned)

### 1. Non-blocking readers (versioned page visibility)

**Partially done.** `NervusDb::read_snapshot()` now gives a caller a
self-consistent view: it holds the shared read lock for its lifetime, so a
multi-step traversal cannot stitch two states together. That closed the correctness
gap (see `tests/concurrency_isolation_tests.rs`).

What remains is the _performance_ gap: a snapshot blocks writers while it lives,
because a reader and a writer still exclude each other. Removing that needs
versioned page visibility — readers pin a snapshot (typically by reading from the WAL
up to a known commit point) while the writer appends. That is a substantial change to
recovery and page visibility.

**This is a different problem from item 5, and an earlier version of this file said
otherwise.** That version claimed the two were "the same redesign", because both touch
the buffer pool's concurrency. The benchmark's own control run refutes it:

| Variant (16 threads)                 | Speedup | Efficiency |
| ------------------------------------ | ------- | ---------- |
| Loop taking only the outer read lock | 6.40×   | 40.0%      |
| Point reads (touch the buffer pool)  | 0.10×   | 0.6%       |

The outer lock scales to the limit of this machine. The collapse is entirely inside
`BufferPoolManager`, so item 5's fix (stop serializing readers against _each other_)
needs sharding, not versioned visibility. This item is about the _other_ pair —
a reader and a **writer** excluding each other through the outer `RwLock`, which
sharding does nothing for. Different cause, different fix; they merely live in the
same module. See [`docs/benchmarks.md`](docs/benchmarks.md#concurrency-scaling) for the
measurements.

### 2. Planner memory: spilling instead of capping

**Done.** Two mechanisms, and they are separate on purpose.

**Capping is still the default.** The action queue is bounded by
`DEFAULT_MAX_TRANSACTION_ACTIONS`; overflow is an error rather than an automatic flush,
because flushing mid-transaction commits part of it and destroys the atomicity that is
the reason to use a transaction at all.

**Spilling is available when a transaction genuinely needs to be larger than memory.**
`NervusDbOptions::spill_transaction_actions` (default `false`) makes overflow write the
queued actions to the WAL as `ActionWrite` frames (tag 6) and keep only a location
index — the same shape as STEAL spilling for pages. Resident memory becomes "one window
plus 8 bytes per action" instead of "≈502 bytes per node action".

**Why it is off by default** — the costs are real, and they are the whole story:

- While a transaction has spilled actions, **`Checkpoint` is refused**, because a
  checkpoint truncates the WAL and would discard those frames. The refusal is an error,
  not a no-op, so "deferred" cannot be mistaken for "done".
- An **automatic** checkpoint deferred this way does not fail the commit that triggered
  it. That commit is already durable by then, and reporting it as failed invites a
  retry that duplicates data. This was a real defect during implementation, caught by a
  test that asserted the surviving data rather than the returned `Result`.
- WAL size grows with the transaction until it commits or rolls back.

Also unified: `add_nodes` / `add_edges` now enqueue through `Transaction::push_op`
instead of checking the cap inline. Those inline copies were a second implementation of
the cap and could not see the spill logic at all — one option with two behaviours. The
batch methods keep the reason they exist (one lock acquisition to reserve N ids); only
the enqueue moved.

[`FORMAT.md`](FORMAT.md) documents the frame and its recovery rule, since this adds a
byte to the WAL format at version 5.

### 3. Cost-based query planning

**Done, with a stated limit.** `src/cypher/planner.rs` holds the cost model and the
join order; `find_matches` now runs a planned index-nested loop instead of expanding
every pattern independently and multiplying.

What it does:

- **Orders patterns by estimated cardinality**, preferring a pattern that shares a
  variable with the ones already solved. Connectivity comes first because a cheap
  pattern with no shared variable only produces rows for a later Cartesian product.
- **Drives the inner pattern from the bound node.** When a pattern's start variable is
  already bound, it expands from that one node rather than computing the pattern's full
  match set and discarding almost all of it.
- **Reports the plan in `EXPLAIN`**, including each pattern's estimate and whether that
  estimate was _measured_ (counted from a real index) or _approximated_.

What it does **not** do — the limit is the point, so it is stated rather than implied:

- **There is no per-relationship-type edge count and no degree histogram.** Fan-out is
  approximated as the average degree `2E/N`, scaled by the target label's share. The
  model therefore separates "indexed equality lookup" from "full scan" (orders of
  magnitude) but **cannot** rank two patterns whose candidate counts are similar.
- **No index-nested-loop _selection_**: the index is used when one exists rather than
  being costed against a scan. The planner decides order, not access method.
- **The estimate feeds only the ordering**, never a row-count promise. `EXPLAIN` labels
  each number's basis so a reader is not misled into treating an approximation as a
  measurement.

Measured effect (this repository's fixture, 64-frame pool, 20,000 decoy nodes with only
one relevant edge): buffer-pool misses for the driven query are 0–5 with the bound-driven
path against **316** with it disabled — the reorder is a real work reduction, not a
notation change. `tests/planner_tests.rs` pins that comparison and asserts the row sets
are unchanged by reordering.

**Row order in multi-pattern results changed.** Cypher does not promise an order without
`ORDER BY`, so this is not a compatibility break, but it is a visible difference: rows
now come out in the planned pattern order. Documented here and in the changelog rather
than left for a caller to discover.

### 4. SDK publication

**The name is settled: `nervusdb`.** The earlier `graphlite*` names were unusable —
`graphlite` belongs to unrelated projects on every registry (`eugene-eeo/graphlite`
on PyPI, `GraphLite-AI/GraphLite` on crates.io), so publishing under it would have
shipped someone else's name, and a published name cannot be cleanly retracted.
`nervusdb` is already owned by this project on crates.io, and the old `0.0.x`
releases there have been yanked, so the name now resolves only to the new line.

**Packaging is done.** The release workflow builds four native targets
(macOS arm64/x64, Linux arm64/x64) — the previous configuration published a single
Linux wheel and an npm tarball with no loadable binary, so neither registry could have
served a working install. What remains before a first publish is not code: the three
registry tokens must be set, and required reviewers added to the `release` environment
(see [docs/releasing.md](docs/releasing.md)).

**The previously recorded "9x slower than native" figure is retracted** — it came
from debug builds of the bindings compared against a release core. Measured with
both sides in release the bindings run at 0.85-0.93x of the native path. See
`docs/benchmarks.md` for the corrected table and the retraction.

### 5. Latch contention: measured, only the fix remains

**Measured, and the answer is worse than "not characterised".**
`concurrency_stress_tests.rs` covers the correctness guarantees (no deadlock, no lost
writes, self-consistent structure, readers make progress). A separate profiling run on
the real com-DBLP database found that read throughput **degrades** as threads are
added: 16 threads reached 0.6-1.4% of single-thread throughput, i.e. reads got slower.
See [benchmarks.md](docs/benchmarks.md#concurrency-scaling) for the method and the
control runs that rule out the machine.

The cause is structural, not a tuning problem: every page touch goes through the one
`Arc<Mutex<BufferPoolManager>>`, and a single `get_node` of a degree-343 hub used to
cost ≈345 acquisitions.

**Half done.** The per-edge acquisitions are now collapsed into one per chain
(`collect_edge_chain_batched`), which under 8-thread contention on a shared hub
measured 1.4-2.2× (17.4-27.8k → 37.8-38.3k ops/s, and far more stable run to run).

**Further reduced, not fixed.** `get_node` now takes the global mutex **once** instead
of four times (record, payload, outgoing chain, incoming chain — the chain walks had
already gone from one acquisition per edge to one per chain). Measured on a
reproducible synthetic instrument
(`benches/real_data/concurrency_scaling_bench.rs`, 200k nodes / 600k edges, 4096-frame
pool, 98.7% cache hit so the cause is not disk): **8 threads went from 191–208k to
396k ops/s (2.0×)**, while the 1-thread row is unchanged — which is the control that
proves this is contention reduction rather than a faster path.

**Still open:** readers remain serialized, because the mutex is still global. The curve
still falls (894k → 396k from 1 to 8 threads), so **there is no read parallelism yet** —
only fewer, shorter visits to one lock. Closing it needs per-frame latching, i.e. a
redesign of the buffer pool's concurrency model, not a patch.

**The target, measured on the real dataset.** The synthetic fixture above shows a curve
shape; it does not say how much is left. com-DBLP (317,080 nodes / 1,049,866 edges) with
a pool actually large enough to hold it (256 MB against an 81 MB file, 99.6% cache hits,
so the mutex is the only variable left), all threads reading the **same** 50
highest-degree hubs — the worst case for one global lock:

| Threads | ops/s  | Efficiency |
| ------- | ------ | ---------- |
| 1       | 69,962 | 1.00×      |
| 2       | 57,880 | 0.41×      |
| 4       | 57,514 | 0.21×      |
| 8       | 56,876 | 0.10×      |
| 16      | 57,132 | **0.051×** |

**5.1% efficiency against the ≈40% this machine can deliver**, and throughput flattens
rather than falls once the mutex is the only constraint left. That is the number
per-frame latching has to move. Reproduce with:

```bash
DB_PATH=<a dblp_bench database> POOL_FRAMES=65536 HUB_READ=1 \
  cargo bench --bench concurrency_scaling_bench
```

**A trap in measuring this.** The same read path with the **default 4 MB pool** reports
0.78× at 16 threads instead of 0.42× — disk I/O masks the contention and makes the
problem look smaller than it is. Any comparison of a latching change must state its pool
size. And `HUB_READ` (every thread on the same hubs) versus disjoint ranges (a different
node per thread) are not interchangeable: the first is the worst case for one global
lock, the second the best. Mixing them yields a before/after that reads like a comparison
but measures two different workloads.

**Not the same fix as item 1** — that one is reader-versus-writer through the outer
`RwLock`; this one is reader-versus-reader inside the pool. See the note under item 1.

Also found while profiling: `Frame::latch` was dead code — declared and initialized
since the initial commit, never read or written. **Deleted**, so the struct no longer
implies a page-level latching that does not exist. Making it real is this item's work,
not a field to restore.

## Explicitly out of scope

- **Distributed or multi-node operation.** The design premise is a single
  embedded file, like SQLite.
- **In-memory graph mode.** A resident full-graph representation would violate
  the bounded-memory invariant that the whole architecture is built around.
- **Cypher `WITH`, subqueries, stored procedures.** `MERGE` and `UNWIND` landed in
  0.1.0; the rest each need design work rather than a patch.
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
