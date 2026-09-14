# Testing

## Running

```bash
# The full gate — this is what CI runs
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Two of those are easy to skip and both have already failed CI once:
`rustdoc -D warnings` rejects unescaped angle brackets in doc comments
(`<expr>`, `<NodeId>` are parsed as HTML), and clippy's lint set moves with the
compiler, so a toolchain update can introduce a new lint that turns a green tree
red. When that happens, fix the code rather than pinning an older compiler.

The throughput-sensitive suites only mean anything when optimised:

```bash
cargo test --release --test batch_tx_tests
cargo test --release --test edge_locality_tests
```

Target-specific suites:

```bash
cargo test --test unwind_tests            # UNWIND and batch ingestion
cargo test --test merge_tests             # MERGE idempotence
cargo test --test concurrency_isolation_tests  # snapshot consistency under load
cargo test --test production_safety_tests # exclusive lock, read-only writes, rollback
cargo test --test robustness_tests        # page CRC at scale, WAL replay, chunking
```

## Current state

**261 test cases — 260 pass, 1 intentionally `#[ignore]`d** (a child-process lock
probe launched by its parent test).

Run as 21 integration suites (231 cases, of which 1 is `#[ignore]`d) plus 28 inline
unit tests in the hand-written codecs, the action codec, and the Page-0 layout
(`src/codec.rs`, `src/json.rs`, `src/crc32.rs`, `src/action_codec.rs`, `src/page.rs`), which are what the on-disk
format is made of, plus 2 doc-tests: 231 + 28 + 2 = 261.

**The one `#[ignore]`d case is not a skipped test.** It is
`production_safety_tests::cross_process_child_probe`, an eight-line probe that must be
launched by `test_cross_process_lock_excludes` **as a real child process** while the parent
holds the database lock, so the lock's exclusivity is observed _across processes_ rather
than within one. It takes the target path from `GL_CHILD_DB` (set by the parent) and has no
assertions of its own — it prints `CHILD_RESULT=…` and the parent decides. Both facts mean
it must not run in a default `cargo test`: it would panic on the missing variable, and on
its own it verifies nothing. `#[ignore]` plus the parent's `--ignored` is libtest's
mechanism for exactly this. Removing it breaks **two** tests — measured, not reasoned: the
probe panics, and the parent then cannot find `CHILD_RESULT=locked`.

So `1 ignored` is the expected count, and it is the only one. If that number ever rises, a
case was silenced rather than fixed.

The table below is checked against the files by
`zero_dependency_tests::documented_suite_table_matches_the_files`, so a case added or
removed without updating this table fails the build rather than drifting:

| Suite                            | Cases | Covers                                                                                                                |
| -------------------------------- | ----- | --------------------------------------------------------------------------------------------------------------------- |
| `integration_tests.rs`           | 31    | CRUD, ACID, concurrency, indexing, stress, query chain, public-API helpers                                            |
| `memory_mode_tests.rs`           | 6     | `:memory:` disk-free + readable across checkpoint; clean-uncommitted eviction; backup refusal; uncovered entry points |
| `production_safety_tests.rs`     | 35    | Exclusive lock, integrity, constraints, read-only writes (incl. transaction paths), queue cap                         |
| `cypher_advanced_tests.rs`       | 19    | Cypher 1.0 syntax closure, EXPLAIN, aggregates, negative/float literal round trip                                     |
| `unwind_tests.rs`                | 16    | `UNWIND`, batch ingestion, statement atomicity                                                                        |
| `merge_tests.rs`                 | 14    | `MERGE` idempotence, ON CREATE / ON MATCH                                                                             |
| `concurrency_isolation_tests.rs` | 11    | Snapshot consistency, atomic visibility, no lost writes, merged-lock `get_node`                                       |
| `concurrency_stress_tests.rs`    | 3     | Core-count-adaptive mixed read/write stress                                                                           |
| `edge_locality_tests.rs`         | 13    | Weave equivalence, self-loops, false spill, chain-walk equivalence                                                    |
| `robustness_tests.rs`            | 9     | File lock, auto-checkpoint (both paths), page CRC, WAL replay, chunking                                               |
| `batch_tx_tests.rs`              | 7     | Batch commits, single-fsync contract, throughput                                                                      |
| `slotted_property_tests.rs`      | 7     | Page packing, slot reuse, compaction, density                                                                         |
| `analytics_tests.rs`             | 10    | PageRank, WCC, K-hop completeness, cycle false-positives                        |
| `steal_spill_tests.rs`           | 5     | Spilling, rollback pollution, checkpoint                                                                              |
| `equivalence_tests.rs`           | 4     | v1.0.0 behaviour guardrails: query, transaction, API, format                                                          |
| `zero_dependency_tests.rs`       | 14    | Empty deps; version, name, suite-count, section-ref, package, ignore + example guards                                 |
| `planner_tests.rs`               | 9     | Join reorder equivalence, EXPLAIN plan, bound-driven work reduction, non-driven rescan                                |
| `lock_wait_tests.rs`             | 5     | Lock wait: default no-wait, wait succeeds, timeout, still exclusive                                                   |
| `lock_cross_process_tests.rs`    | 1     | Two real processes: second refused, then waits and writes                                                             |
| `multi_process_write_tests.rs`   | 1     | 4 processes write concurrently: no lost writes, no deadlock                                                           |
| `spill_action_tests.rs`          | 11    | Action spill to WAL, order, rollback, checkpoint refusal, large batch                                                 |
| inline (in `src/`)               | 28    | `codec` / `json` / `crc32` / `action_codec` / Page-0 layout round-trips                                               |

Run one suite:

```bash
cargo test --test production_safety_tests
cargo test --release --test batch_tx_tests -- --nocapture
```

## The real dataset is the only end-to-end check at scale

Unit and integration suites run on fixtures. The SNAP benchmarks run on real graphs, and
they are the only check that exercises the engine at a scale where page eviction, CRC
directory churn, and recovery actually happen. **Run them for any page, checksum, or
replay change.**

```bash
DATASET_PATH=/data/com-dblp.ungraph.txt DB_DIR=/data/bench POOL_MB=256 \
  cargo bench --bench snap_dblp_bench
DATASET_PATH=/data/soc-LiveJournal1.txt DB_DIR=/data/bench POOL_MB=1024 \
  cargo bench --bench snap_livejournal_bench
```

They take `DATASET_PATH`/`DATASET_DIR`, `DB_DIR`, `POOL_MB`, `MAX_EDGES`,
`AUTO_CHECKPOINT_MB` and print the configuration they ran with. Leave
`AUTO_CHECKPOINT_MB=0`: the engine's 64 MB default roughly halves bulk ingest throughput,
and these benchmarks checkpoint explicitly.

The SNAP files are not in the repository, so those two commands only work if you have
them. This one needs **no dataset** and reproduces the read-scaling curve on a synthetic
graph, which is what makes the "reads scale negatively" claim checkable by someone other
than its author:

```bash
cargo bench --bench concurrency_scaling_bench        # NODES / EDGES / POOL_FRAMES
```

It reports the cache hit rate alongside each row on purpose: a high hit rate is the
evidence that the collapse is lock contention rather than disk I/O.

**Red lines — a change here is a correctness regression, not noise:**

| Metric                        | Value                            |
| ----------------------------- | -------------------------------- |
| LiveJournal edge ingestion    | **≥150,000 ops/s**               |
| LiveJournal hub 1-hop / 2-hop | **exactly** 335,194 / 10,027,730 |
| com-DBLP hub 1-hop / 2-hop    | **exactly** 10,080 / 161,877     |

**The exact totals are the check; the rates are not.** Measured rates vary by 20%+ run
to run on one machine ([benchmarks.md](benchmarks.md#concurrency-scaling) shows the
spread), so treating a rate as a regression detector produces false alarms. The totals
are deterministic, which is what makes them useful.

### Recorded verification run (2026-09-13)

Both red lines were confirmed on real data after the join planner, action spilling and
single-acquisition `get_node` changes landed:

| Benchmark                     | Totals observed      | Red line             |
| ----------------------------- | -------------------- | -------------------- |
| com-DBLP hub 1-hop / 2-hop    | 10,080 / 161,877     | 10,080 / 161,877     |
| LiveJournal hub 1-hop / 2-hop | 335,194 / 10,027,730 | 335,194 / 10,027,730 |

The rate row is where the warning above earned its place. Five consecutive LiveJournal
runs on the same machine, alternating two builds that differed only by an inlined bounds
check, produced **136k, 167k, 190k, 195k, 176k ops/s**. Two runs of the _same_ build
differed by 40%, and each build's range overlapped the other's entirely — so the first
136k reading looked exactly like a regression from a change that made no measurable
difference. That change was reverted rather than kept: an optimization with no evidence
of benefit does not belong in the tree.

Taking that reading as a regression would have been both wrong and expensive. The table
above quotes **totals only** for this reason.

Hub selection must stay a **total order** (degree descending, then raw id ascending).
com-DBLP has three nodes tied at degree 164 _exactly at rank 50_, so a partial order
makes the 2-hop total depend on sort internals — and produced three different
"correct" numbers across runs.

## A regression test that passes with the fix removed proves nothing

This is not a slogan; it is the acceptance bar for every fix in this repository. The
fix is reverted in isolation, the new test re-run, and it must fail with a symptom that
matches the diagnosis.

Two traps make a would-be reproducer useless, and both recur:

- **Too small a fixture.** The CRC-directory defects need more directory pages than
  `CRC_CACHE_CAPACITY` (64) holds — roughly 65,000 data pages. 90k- and 800k-node
  fixtures passed while the engine was broken; only 300,000 distinct page numbers reach
  the eviction path.
- **Too clean a shutdown.** The WAL-replay checksum defect needs a real `SIGKILL`. A
  normal `drop` flushes the buffer pool and writes pages and checksums together, hiding
  it completely.

Two earlier reproducers for those defects passed _with the fix removed_ and were
discarded rather than kept. A test that cannot fail is worse than no test: it is a
claim of coverage that nothing backs.

## The house style: adversarial, not happy-path

A test that only exercises the happy path is treated here as incomplete. In
practice that means three things.

### 1. Cover the boundaries, every time

For anything you add or change, exercise:

- `N == 0` and `N == 1`;
- self-collision — source equals target (a self-loop), or an input that is also
  an output;
- duplicates and repeated calls (reentrancy);
- exactly-full capacity;
- smallest and largest valid values.

Never filter a boundary case out of a generator to make a test pass. If a case
you added breaks the code, the test is doing its job.

### 2. Verify failure atomicity, not just success

If step K of a multi-step operation fails, nothing from steps 1..K-1 may survive,
and the main database file must be **byte-identical** to before. The rollback
suites assert the file bytes directly, not just the in-memory counts.

### 3. Verify claims two independent ways

Any claim worth asserting is worth a second, independent route to the same
answer. Two forms are used in this codebase:

- **Conservation oracle** — the same quantity computed two different ways must
  agree. `integrity_check` measures node degree both by walking the on-disk chain
  pointers and by independently scanning the edge id space. Deriving both sides
  from one traversal would be self-confirmation, not a check.
- **Differential oracle** — the same input through the old and the new code path
  must produce identical results. `commit()` (batch weaving) versus
  `commit_unclustered()` (per-edge insertion) is asserted to produce identical
  graph structure and PageRank scores agreeing within 1e-12.
- **Cross-SDK oracle** — the same operation through both language bindings must
  produce the same answer. `bindings/cross_sdk_check.py` drives the Python and Node.js
  SDKs over the same values and diffs the results.

  This one exists because a binding bug is invisible from _inside_ the binding: each
  SDK's own suite can be green while the two disagree. That is exactly how the integer
  defect survived — Python returned `9007199254740993` and Node returned
  `9007199254740992` for the same write, and neither suite compared against the other.
  Node's own suite could not have caught it either, because it only used small
  integers; the value that an f64 cannot represent was never in the fixture.

  Run it with `python3 bindings/cross_sdk_check.py` (needs both bindings built). It
  passes values as **strings** and rebuilds them with `BigInt(...)` on the Node side:
  a JSON number is an f64 there, so passing `9223372036854775807` as a number would
  corrupt it before the SDK ever saw it and the check would be measuring the harness.

  Verified the way every guard here is: reverting the Node binding to `number` makes
  it fail with the original three symptoms, naming `i64_max`, `i64_min` and
  `pow53_plus_1`.

## Assertions must not depend on machine speed

This is the rule that was learned the hard way. A test asserted that batched
writes are ">20× faster than autocommit". It passed locally and failed on
`ubuntu-latest` at ~16×, because a cloud disk's fsync characteristics differ from
a local SSD — the assertion was measuring hardware, not the engine.

Assert the **mechanism** instead:

```
N autocommitted writes  ->  N fsyncs
1 batched transaction    ->  exactly 1 fsync
```

That holds on any hardware. Keep any speed ratio as a loose lower bound, never as
a fixed multiplier. Where a throughput floor is unavoidable, warm up first so
one-off allocation cost is not counted, and leave real headroom.

## What the suites pin down

Concrete examples of the style, so the intent is clear:

- **A real child process** attempts to open a database already held by the parent,
  and must report `DatabaseLocked`. The assertion also requires the child to have
  actually run its test (`running 1 test`), so a silently-not-run child cannot
  pass vacuously.
- **Deliberate corruption.** A node page is overwritten and the integrity check
  must report it — with a healthy-database negative control in the same suite, so
  a check that always fails cannot pass either. A second case corrupts _only_
  chain pointers, leaving counts self-consistent, which the degree-conservation
  oracle must still catch.
- **Cross-process lock probe**, **poisoned-lock recovery** (with a `catch_unwind`
  control proving plain `read().unwrap()` does panic), and **API semantic
  difference**: on a corrupt page `try_get_node` returns `Err` while `get_node`
  returns `None`, pinning the documented lossy behaviour.
- **Density**: 40,000 entities must fit in a fraction of the space the old
  one-page-per-entity layout needed, asserted as a compression ratio.

## Reporting a failure

If you are reporting a test or behaviour problem, include the exact command and
the raw output. See [CONTRIBUTING.md](../CONTRIBUTING.md) — this repository does
not accept external code, but a reproducible report is genuinely useful.
