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
(they moved out of `examples/` in 1.1.0; run them with `cargo bench --bench snap_dblp_bench`).
Both take
`DATASET_PATH`/`DATASET_DIR`, `DB_DIR`, `POOL_MB` and `AUTO_CHECKPOINT_MB`, and
print the configuration they ran with.

com-DBLP (317,080 nodes / 1,049,866 edges, 256MB pool, auto-checkpoint off;
measured on 1.1.0, 2026-09-13, release build):

| Metric             | Value                             |
| ------------------ | --------------------------------- |
| Node ingestion     | 579,533 ops/s                     |
| Edge ingestion     | **761,526 ops/s**                 |
| Hub 1-hop (top 50) | 42.9 µs avg, **10,080** neighbors |
| Hub 2-hop (top 50) | 1.86 ms avg, **161,877** reached  |
| On-disk size       | 81.26 MB                          |

LiveJournal (4,847,571 nodes / 68,993,773 edges, 1GiB pool, auto-checkpoint off;
measured on 1.1.0, 2026-09-13, release build):

| Metric             | Value                             |
| ------------------ | --------------------------------- |
| Node ingestion     | 560,455 ops/s                     |
| Edge ingestion     | **155,363 ops/s**                 |
| Hub 1-hop (top 50) | 0.35 s avg, **335,194** neighbors |
| Hub 2-hop (top 50) | 8.9 s avg, **10,027,730** reached |
| On-disk size       | 4.34 GB                           |

Throughput on a single machine varies run to run: repeated com-DBLP edge runs
across this session measured 723,458 / 580,139 / 761,526 ops/s, and LiveJournal
edge ingest measured 155,363 ops/s on 1.1.0 against 201,823 on 1.0.0 — neither
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
