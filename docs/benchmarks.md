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

`examples/snap_dblp_bench.rs` and `examples/snap_livejournal_bench.rs`. Both take
`DATASET_PATH`/`DATASET_DIR`, `DB_DIR`, `POOL_MB` and `AUTO_CHECKPOINT_MB`, and
print the configuration they ran with.

com-DBLP (317,080 nodes / 1,049,866 edges, 256MB pool):

| Metric             | Value                             |
| ------------------ | --------------------------------- |
| Node ingestion     | 892,000 ops/s                     |
| Edge ingestion     | **598,902 ops/s**                 |
| Hub 1-hop (top 50) | 40.7 µs avg, **10,080** neighbors |
| Hub 2-hop (top 50) | 1.63 ms avg, **161,877** reached  |

LiveJournal (4,847,571 nodes / 68,993,773 edges, 1GiB pool, auto-checkpoint off):

| Metric             | Value                             |
| ------------------ | --------------------------------- |
| Node ingestion     | 713,000 ops/s                     |
| Edge ingestion     | **200,618 ops/s**                 |
| Hub 1-hop (top 50) | 0.35 s avg, **335,194** neighbors |
| Hub 2-hop (top 50) | 8.1 s avg, **10,027,730** reached |
| On-disk size       | 4.34 GB                           |

Both 1-hop and 2-hop totals are **exact** against an independent recomputation
over the raw dataset. Getting there required fixing a real determinism bug: the
top-50 hub set is selected by degree, and in com-DBLP three nodes tie at degree
164 _exactly at rank 50_, so the fiftieth hub — and therefore the 2-hop total —
depended on sort internals. Runs produced 161,789 / 161,877 / 162,158, all
"correct" for their own hub set. The sort is now a total order (degree
descending, then raw id ascending).

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

Same host, smoke scale (100k nodes + 400k edges):

| SDK            | Node writes  | Edge writes   |
| -------------- | ------------ | ------------- |
| Python (PyO3)  | 63,000 ops/s | 112,000 ops/s |
| Node.js (NAPI) | 64,000 ops/s | 117,000 ops/s |

The native Rust path is ~550,000 ops/s at the same scale, so the gap is the
**one-call-per-write FFI boundary**, not engine speed. A batch API accepting an
array of entities per call would close most of it; see the roadmap.

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
