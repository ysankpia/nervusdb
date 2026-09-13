# Benchmarks

Every number in this document comes from a benchmark **in this repository**, and
every scenario prints its own configuration. That is deliberate: a throughput
number without its measurement conditions is not comparable to anything, and this
project has already had to correct figures that could not be reproduced.

## How to run

```bash
# Full scale (tens of millions of nodes; minutes, and several GB of temp files)
cargo bench --bench throughput

# Smoke scale, to check the harness itself works
GL_SCALE=small cargo bench --bench throughput

# Pool size vs throughput, optionally chunking the workload
cargo bench --bench pool_probe
GL_NODES=1000000 GL_EDGES=4000000 GL_CHUNKS=40 cargo bench --bench pool_probe

# Peak memory of a single large transaction
cargo bench --bench mem_probe

# SDK benchmarks (from the binding directories)
cd bindings/python && PYTHONPATH=. python3 benches/throughput.py
cd bindings/nodejs && node benches/throughput.mjs
```

## The rule for citing a number

State the command, the hardware (CPU, storage type, OS), the buffer-pool size, and
— for node writes — the property payload. Removing properties roughly doubles node
throughput, so a number quoted without its payload is not interpretable.

See [ROADMAP.md](../ROADMAP.md) for the contribution rule on this.

---

## Measured results

**Host:** Apple M4 (10 cores) / 16GB / macOS 15.7.5 / internal SSD.
Single-transaction batched writes, release build.

### Node writes

Nodes carry an `Entity` label plus two properties (integer `idx`, 8-digit string
`name`) unless marked otherwise.

| Scenario                      | Scale | Pool | Throughput          |
| ----------------------------- | ----- | ---- | ------------------- |
| File-backed                   | 10M   | 64MB | **219,000 ops/s**   |
| File-backed                   | 1M    | 64MB | **547,000 ops/s**   |
| `:memory:`                    | 1M    | 64MB | **577,000 ops/s**   |
| `:memory:`, **no properties** | 1M    | 64MB | **1,430,000 ops/s** |

The last row is the ceiling of the node path. Property payload costs about 2.2×
at this size — `PropCodec` and the slotted pages are what that 2.2× buys.

### Edge writes

`1M nodes + 4M discrete random edges` (edge:node = 4:1):

| Pool                 | Throughput        |
| -------------------- | ----------------- |
| 64MB (16,384 frames) | 322,000 ops/s     |
| 1MB (256 frames)     | **906,000 ops/s** |

**The small pool is faster here, and that is not a bug.** The discriminator is
chunking: split the same 4M-edge workload into multiple transactions and the
trend inverts into a monotonic increase (256 frames 165k → 16,384 frames 420k,
with zero spill at the largest pool).

The cause is that **a single transaction materialises its whole action list in
memory**. At 4M edges, process RSS grows from 897MB to 1104MB. A larger pool then
competes with that footprint for memory bandwidth. Chunking removes the
competition entirely.

**Therefore: chunk very large writes into multiple transactions.** The kernel
should cap and spill planner state instead of relying on the caller; that is
tracked in the roadmap as "bounded-memory batch planning". Edge batches are now
additionally chunked internally at `MAX_BATCH_EDGES_IN_MEMORY` (100,000), so the
weaving step's own working set is bounded regardless of transaction size.

### Real-world graphs (SNAP)

`benches/real_data/snap_dblp_bench.rs` and `benches/real_data/snap_livejournal_bench.rs`
(they moved out of `examples/` before 0.1.0; run them with `cargo bench --bench snap_dblp_bench`).
Both take
`DATASET_PATH`/`DATASET_DIR`, `DB_DIR`, `POOL_MB` and `AUTO_CHECKPOINT_MB`, and
print the configuration they ran with.

com-DBLP (317,080 nodes / 1,049,866 edges, 256MB pool, auto-checkpoint off;
measured 2026-09-13, release build, pre-rename):

| Metric             | Value                             |
| ------------------ | --------------------------------- |
| Node ingestion     | 579,533 ops/s                     |
| Edge ingestion     | **761,526 ops/s**                 |
| Hub 1-hop (top 50) | 42.9 µs avg, **10,080** neighbors |
| Hub 2-hop (top 50) | 1.86 ms avg, **161,877** reached  |
| On-disk size       | 81.26 MB                          |

LiveJournal (4,847,571 nodes / 68,993,773 edges, 1GiB pool, auto-checkpoint off;
measured 2026-09-13, release build, pre-rename):

| Metric             | Value                             |
| ------------------ | --------------------------------- |
| Node ingestion     | 560,455 ops/s                     |
| Edge ingestion     | **155,363 ops/s**                 |
| Hub 1-hop (top 50) | 0.35 s avg, **335,194** neighbors |
| Hub 2-hop (top 50) | 8.9 s avg, **10,027,730** reached |
| On-disk size       | 4.34 GB                           |

Throughput on a single machine varies run to run: repeated com-DBLP edge runs
across this session measured 723,458 / 580,139 / 761,526 ops/s, and LiveJournal
edge ingest measured 155,363 ops/s pre-rename against 201,823 on the 1.0.0 line — neither
figure is a regression, since the same binary re-measured at 280,426 earlier in the
release cycle. The **red lines are the totals, not the rates**: hub 1-hop and 2-hop
must match exactly, and LiveJournal edge ingest must stay above 150,000 ops/s.
Rate claims elsewhere in this document carry the run they came from.

Both 1-hop and 2-hop totals are **exact** against an independent recomputation
over the raw dataset. Getting there required fixing a real determinism bug: the
top-50 hub set is selected by degree, and in com-DBLP three nodes tie at degree
164 _exactly at rank 50_, so the fiftieth hub — and therefore the 2-hop total —
depended on sort internals. Runs produced 161,789 / 161,877 / 162,158, all
"correct" for their own hub set. The sort is now a total order (degree
descending, then raw id ascending).

**Note on the CRC32 cost.** The zero-dependency build was briefly 1.8x slower at
bulk ingest than the release before it, because the hand-written byte-at-a-time
CRC32 cost 7,028 ns per 4 KiB page against `crc32fast`'s 323 ns. Every WAL frame
computes two checksums, so this was the bottleneck. Slicing-by-8 brought the page
down to 1,750 ns and the throughput back to the figures above. The residual ~6% is
the measured price of having no runtime dependencies, and it is recorded here
rather than left implicit.

**Auto-checkpoint roughly halves bulk edge throughput** and the benchmarks
therefore turn it off:

| `wal_auto_checkpoint_bytes` | 10M-edge ingest   |
| --------------------------- | ----------------- |
| `0` (off)                   | **447,122 ops/s** |
| 64MB (engine default)       | 232,350 ops/s     |

Each automatic checkpoint flushes and fsyncs the entire dirty set on top of the
caller's own rhythm. The default stays on because the alternative is an unbounded
WAL; a bulk loader that checkpoints on its own schedule should set it to `0`.

### Edge-write locality work

Discrete edge writes used to degrade as the graph grew. The cause was not cache
hit rate but three structural defects (details in
[architecture.md §6](architecture.md#6-two-phase-batch-edge-weaving)):

| Pool               | Before       | After               | spill/edge       |
| ------------------ | ------------ | ------------------- | ---------------- |
| 1024 frames (4MB)  | 58,000 ops/s | **~200,000 ops/s**  | 1.20 → **0.045** |
| 256 frames (1MB)   | 84,000 ops/s | **~310,000 ops/s**  | 1.66 → **0.02**  |
| 8192 frames (32MB) | —            | **~260,000** (flat) | **0.031**        |

`spill/edge` falling from 1.20 to 0.031 is the false spill being eliminated. The
residual misses at 1024 frames are **genuine capacity pressure**, not a structural
defect: 4M edge-record pages (15,625) plus 1M node pages (7,813) need roughly
91MB against a 4MB pool. At 8192 frames the miss rate is flat, which is what
distinguishes capacity from defect.

### Concurrency scaling

**Result: read throughput degrades as threads are added.** This is the one measured
number in this document that is bad, and it is recorded here rather than left out
because a reader deciding whether to use this database needs it.

Method: com-DBLP database (317,080 nodes / 1,049,866 edges, 81.26 MB) opened
read-only, 10-core machine, release build. Each configuration ran for a fixed 1.2 s
wall-clock budget so that slow configurations are not flattered by finishing first;
the reported figure is completed operations ÷ elapsed.

| Threads | Hub point reads (high lock count) | Random point reads | `MATCH (n) RETURN count(*)` |
| ------- | --------------------------------- | ------------------ | --------------------------- |
| 1       | 14,581 ops/s (1.00×)              | 55,972 ops/s (1.00×) | 5 ops/s (1.00×)           |
| 2       | 11,275 ops/s (0.77×)              | 49,001 ops/s (0.88×) | 5 ops/s (0.90×)           |
| 4       | 7,991 ops/s (0.55×)               | 40,521 ops/s (0.72×) | 4 ops/s (0.80×)           |
| 8       | 9,865 ops/s (0.68×)               | 37,184 ops/s (0.66×) | 2 ops/s (0.46×)           |
| 16      | 10,420 ops/s (0.71×)              | 38,790 ops/s (0.69×) | 2 ops/s (0.47×)           |

At 16 threads the scaling efficiency (`speedup ÷ threads`) is **0.6%–4.5%** — adding
threads makes reads slower, not faster.

**Control runs, because a bad number must be shown to be real.** The collapse could
plausibly be the measuring machine rather than the database, so three variants ran in
the same process:

| Variant                                  | 16-thread speedup | Efficiency |
| ---------------------------------------- | ----------------- | ---------- |
| Pure CPU spin (no database calls)        | 6.34×             | 39.6%      |
| Loop taking only the outer read lock     | 6.40×             | 40.0%      |
| Point reads (any, i.e. touching the pool)| 0.10×–0.23×       | 0.6%–1.4%  |

The first two establish what this 10-core machine can deliver (≈6.4×, ≈40%
efficiency) and show that neither the environment nor the outer `RwLock` is at fault.
The two point-read rows use disjoint node ranges and a shared contiguous range
respectively, so cache-line sharing is not the explanation either. The common factor
is `BufferPoolManager`.

**Cause**: `DiskGraph` reaches the pool through one `Arc<Mutex<BufferPoolManager>>`,
so every page touch takes a global mutex. A `get_node` acquires it at least three
times (record, properties, and once **per incident edge**): a degree-343 hub costs
≈345 acquisitions. Serializing 345 critical sections per read is what turns extra
threads into contention.

**Mitigation applied: collapse the per-edge acquisitions.** `get_node` now walks a
whole adjacency chain inside **one** buffer-pool acquisition instead of one per edge
(`DiskGraph::collect_edge_chain_batched`), so a degree-343 hub costs a handful of
acquisitions rather than ≈345.

*(The two paragraphs above describe the state at the time of the com-DBLP measurement.
A later change collapsed those remaining four acquisitions into **one**, making a point
read a single critical section whatever the degree — see the `get_node` rows further
down. The acquisition counts here are history, not the current behaviour.)* Measured under contention — 8 threads all reading the
same hub, which is the worst case for a global mutex, alternating the old and new
builds:

| Build                          | Run 1 | Run 2 | Run 3 |
| ------------------------------ | ----- | ----- | ----- |
| before (per-edge acquisition)  | 17,431 ops/s | 27,605 ops/s | 27,813 ops/s |
| after (per-chain acquisition)  | **38,283** | **37,842** | **38,071** |

Roughly 1.4–2.2×, and the "after" column varies by under 2% while "before" varies by
60% — less lock traffic means less sensitivity to scheduling.

**What this does not fix.** Readers are still serialized: the mutex is still global
and still taken once per call, so scaling efficiency at 16 threads remains ≈4%
rather than ≈40%. Reducing acquisitions shortens each critical section; it does not
let two readers proceed at once. Genuine read parallelism needs per-frame latching,
which is a redesign of the buffer pool's concurrency model rather than a patch.

**Writes were profiled separately, and the bottleneck is different.** Continuing the
same com-DBLP setup, write workloads were measured at three granularities:

| Workload                                          | 1 thread | 2 | 4 | 8 |
| ------------------------------------------------- | -------- | --- | --- | --- |
| one commit per node (`add_node`)                  | 249 ops/s | — | — | — |
| one transaction per node (`with_transaction`)     | 251 ops/s | 249 | 253 | 249 |
| one transaction per 20,000 nodes (`add_nodes`)    | 451,576 ops/s | 823,948 | 1,053,810 | — |

The middle row does not scale **at all** — 8 threads equal 1 thread — but the cause is
not the mutex. Holding the per-transaction cost fixed while varying the number of
nodes per transaction isolates it:

| Nodes per transaction | Transactions | ops/s | Time per transaction |
| --------------------- | ------------ | ----- | -------------------- |
| 1                     | 2,000        | 255   | 3.92 ms |
| 10                    | 200          | 2,747 | 3.64 ms |
| 200                   | 10           | 55,971 | 3.57 ms |
| 2,000                 | 1            | 397,927 | 5.03 ms |

Time per transaction is ≈3.5 ms **regardless of how many nodes it writes**. That is
the `fsync`, and it is serialized by definition. So the 250 ops/s ceiling for
one-node transactions is the durability guarantee working as designed, not a defect:
the fix is batching, which is exactly what `with_transaction` / `add_nodes` are for,
and the bottom row shows that batched writes do scale (1.83× at 2 threads, 2.33× at 4).

The read-side collapse above is therefore not a general "everything is slow" story —
reads are serialized by the buffer-pool mutex at any transaction size, while writes
are limited by one fsync per commit and scale once batching removes that.

**A second mitigation: one lock acquisition per `get_node`, not four.** `get_node` was
taking the global `bpm` mutex four times (record, payload, outgoing chain, incoming
chain). The chain walks had already been collapsed from one acquisition *per edge* to
one per chain; this collapses the remaining four into one, so a point read is a single
critical section instead of four.

Measured with a **reproducible** instrument — `benches/real_data/concurrency_scaling_bench.rs`,
which builds a synthetic 200k-node/600k-edge graph so no external dataset is needed.
512 frames (2MB) would not reproduce the contention; 4096 frames (16MB) against ~45MB
of data does, and every row below reports a 98.7% cache hit rate, which is what rules
out disk I/O as the cause:

| Threads | Before (4 acquisitions) | After (1 acquisition) | Change |
| ------- | ----------------------- | --------------------- | ------ |
| 1       | 872k / 892k ops/s       | 894k ops/s            | ~1.00× (no change) |
| 2       | 510k / 521k ops/s       | 594k ops/s            | 1.15×  |
| 4       | 326k / 322k ops/s       | 437k ops/s            | 1.35×  |
| 8       | 191k / 208k ops/s       | 396k ops/s            | **2.0×** |

Two rows are quoted for the "before" column because it was measured twice; the "after"
value is stable across three runs (425–440k at 4 threads, 396k at 8).

**The single-thread row is the control.** It is unchanged, which is what distinguishes
"less contention" from "a faster code path": reducing critical-section count cannot
help a single thread, and it does not. The gain appears only where threads compete, and
grows with thread count.

**This is not the fix for read parallelism.** Readers are still serialized by one global
mutex; the curve still falls from 894k to 396k. Collapsing four acquisitions into one
shortens each visit to the critical section, it does not admit two readers at once.
That needs per-frame latching — a buffer-pool concurrency redesign, not a patch.
`ROADMAP.md` item 5 tracks it; item 1 (reader-versus-writer through the outer lock) is a
separate problem with a separate fix.

**Scope of the claim.** This is about *read parallelism*, not correctness or
single-threaded speed: the full suite passes, and the batched walk is
indistinguishable from the per-edge walk in every existing test (the chain semantics,
the `in_use` filter and the `seen` cycle guard are all reproduced). See [architecture.md §10](architecture.md#10-concurrency-model) and
`ROADMAP.md`.

### SDK throughput

50,000 nodes with properties (and 100,000 edges), 4096-frame pool, file-backed.
**Bindings must be built with `--release`** — a debug build of the binding against
a release core produces numbers that are wrong by 6x, which is exactly the mistake
this section previously contained (see below).

| Path           | Node writes   | Edge writes    |
| -------------- | ------------- | -------------- |
| Rust (native)  | 382,000 ops/s | ~450,000 ops/s |
| Python (PyO3)  | 355,000 ops/s | 422,000 ops/s  |
| Node.js (NAPI) | 326,000 ops/s | 395,000 ops/s  |

**The bindings are at 0.85–0.93x of the native path.** There is no 9x gap and the
FFI boundary is not the bottleneck — measured directly, a 50,000-node batch spends
0.037 s crossing the boundary and parsing, against 0.095 s actually committing to
disk. The commit is the cost; the boundary is noise.

`Transaction::add_nodes` / `add_edges` exist in both SDKs. They are kept for
ergonomics and to avoid taking the global write lock once per record, **not** as a
throughput claim: on the standard benchmark their effect is within run-to-run noise.

Correction history — the previous version of this table read:

| SDK            | Node writes  | Edge writes   |
| -------------- | ------------ | ------------- |
| Python (PyO3)  | 63,000 ops/s | 112,000 ops/s |
| Node.js (NAPI) | 64,000 ops/s | 117,000 ops/s |

Those were produced with **debug** bindings measured against a **release** core.
The same script measured 499,599 ops/s (release) versus 78,603 ops/s (debug) — a
6.4x delta that was being attributed to the FFI boundary rather than to the build
profile. The figure is retracted; the measurement conditions are now stated above
so the same mistake is harder to repeat.

---

## Correction to previously published figures

Earlier drafts of this project advertised:

| Claimed                              | Measured here |
| ------------------------------------ | ------------- |
| 708,561 ops/s — 10M nodes            | 219,000 ops/s |
| 1,019,526 ops/s — `:memory:`         | 577,000 ops/s |
| 1,439,681 ops/s — 50k edge burst     | 33,189 ops/s  |
| ~624,000 ops/s — 1M nodes + 4M edges | 322,000 ops/s |

Those configurations do not reproduce those numbers on this hardware. The
`:memory:` figure **is** reproducible once the property payload is removed
(~1.43M ops/s), which is most likely how it was obtained — the numbers were
plausible, they were just quoted without their conditions.

This is why the rule above exists.
