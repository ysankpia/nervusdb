# Audit baseline — what has already been checked, and what is deliberate

**Purpose.** A full audit of this repository should not re-derive what earlier passes
already established, and must not report a deliberate design decision as a defect. This
document is the starting point: what has been tested (and how), what has been verified
correct, and what is knowingly out of scope.

It is written to be **checked against the code**, not trusted. Every claim names the file,
test, or command that backs it, and
`tests/zero_dependency_tests.rs::audit_baseline_references_resolve` fails if a referenced
artefact stops existing — so this file cannot quietly rot.

**Scope of this document.** It covers the audit passes run during the 0.1.0 cycle. It is
not a test catalogue (`docs/testing.md` is) and not a design rationale
(`docs/architecture.md` and `FORMAT.md` are).

---

## 1. How the defects so far were actually found

Worth knowing before starting, because it says where to look. **None of the defects below
was found by a failing test.** Each was found by a _method_:

| Defect                                                 | Found by                                                      |
| ------------------------------------------------------ | ------------------------------------------------------------- |
| `:memory:` checkpoint wrote a file and lost every node | Trying a **mode × operation** combination no test entered     |
| Buffer-pool NO-STEAL with every frame idle             | Constructing a **state** no test constructed                  |
| Read-only handle could write (silently)                | Trying a **mode × entry point** combination                   |
| `dump_cypher` unreadable for negatives / whole floats  | Testing the **documented migration path** end to end          |
| Node.js integers silently rounded above 2^53           | **Comparing two SDKs** against each other                     |
| `has_cycle` false positives                            | Generating the **shape** a fixture never had (shared parents) |
| `k_hop_subgraph` dropping layer-k edges                | Asserting edge **contents**, not counts                       |
| Read-only `0444` file could not be read                | Asking **external-failure** questions                         |

The recurring theme: correct code on paths nobody walked, with assertions too coarse to
notice. **The search strategy that works here is "which combination has never been
run", not "which line looks wrong".**

---

## 2. What has been exercised

### 2.1 Suites (21 files, 262 cases)

Per-suite inventory and counts: [`docs/testing.md`](testing.md). Highlights relevant to an
auditor:

- `production_safety_tests.rs` — locking (including cross-process), integrity checking,
  read-only enforcement across **every** write entry point including transactions,
  constraints, checksum coverage.
- `steal_spill_tests.rs`, `spill_action_tests.rs` — page spilling under pools smaller than
  the working set, action-queue spill to WAL, rollback pollution.
- `robustness_tests.rs` — file lock, both auto-checkpoint paths, page CRC, WAL replay,
  version guard.
- `analytics_tests.rs` — PageRank normalisation, WCC ordering, k-hop completeness, cycle
  detection **including the sibling/no-sibling distinction**.
- `memory_mode_tests.rs` — `:memory:` must not touch disk, must survive a checkpoint, and
  must refuse a backup with a real reason.

### 2.2 Instruments (run separately; not part of `cargo test`)

| Instrument                                       | Covers                                              |
| ------------------------------------------------ | --------------------------------------------------- |
| `benches/real_data/cypher_fuzz_bench.rs`         | Random Cypher vs an independent in-memory model     |
| `benches/real_data/differential_test.rs`         | 400k random write ops vs an in-memory oracle        |
| `benches/real_data/crash_recovery_test.rs`       | SIGKILL injection, replay correctness               |
| `benches/real_data/fault_injection_bench.rs`     | Permission, truncation, missing WAL, unwritable dir |
| `benches/real_data/page_boundary_bench.rs`       | Both sides of every structural boundary             |
| `benches/real_data/concurrency_scaling_bench.rs` | Reader scaling (reproducible, synthetic)            |
| `benches/real_data/snap_dblp_bench.rs` etc.      | Real-dataset acceptance and red lines               |
| `ci/long_run.sh`                                 | Runs all of the above in stages; `LONG=1` scales it |

### 2.3 The most recent full campaign

`LONG=1 bash ci/long_run.sh` — nine stages, all passed:

- fuzz: 600 seeds × 2 disjoint ranges, 19,799 + 19,795 checks, 0 failures
- differential: 400,000 operations, 56,180 rollbacks, 160 restarts, 100% oracle agreement
- crash: 30 SIGKILL rounds
- fault, boundaries, concurrency, sdk (Node 30 / Python 21 / cross-SDK), realdata
- com-DBLP red lines exact: hub 1-hop **10,080**, 2-hop **161,877**, file 81.26 MB

### 2.4 Claims verified correct (not merely untested)

These were checked while auditing and found sound. An auditor can skip them until the
code changes:

- **PageRank** sums to 1.0 including all-dangling graphs, `max_iterations` of 0, and
  out-of-range damping; results descending; `-0.0` on the empty graph is `0.0`.
- **`weakly_connected_components`** size-descending with ascending ids inside each
  component, over 12 shapes including ties.
- **`k_hop_subgraph`** rejects an unknown start with `NodeNotFound`, deduplicates, sorts,
  and keeps exactly the edges with both endpoints inside.
- **Transaction lifecycle** — uncommitted drop, panic inside `with_transaction`, and a
  closure returning `Err` all leave the database untouched; tx ids are never reused.
- **Concurrent `MERGE`** on one key from 8 threads: exactly one node created.
- **Statement atomicity** — `UNWIND [2,1,3]` hitting a unique constraint mid-statement
  leaves no partial row, before or after a reopen.
- **Boundaries** — 127/128/129 records per page, 4095/4096/4097 nodes across the
  direct-page edge, 2047/2048/2049 edges, 1KB inline/overflow, and the same through a
  64-frame pool.
- **Crash recovery** — no committed data lost across repeated SIGKILL; metadata stays
  consistent with content.

---

## 3. Deliberate scope limits — **not defects**

Each of these has been encountered during an audit and is intentional. Reporting one as a
bug means its rationale below was not read first.

### 3.1 Correct by specification

- **A relationship is used at most once per match** in variable-length patterns.
  This is the standard default: openCypher CIP **CIR-2017-174** — "Cypher pattern matching
  assumes relationship uniqueness… by default only returns relationship-unique matches."
  Consequence: `(a)-[:R*2..2]->(b)` over a self-loop `8→8` plus `8→5` yields **one** path
  (`8→8→5`), not two; `8→8→8` would need the same edge twice. It is also what bounds the
  traversal. See [`docs/cypher.md`](cypher.md#variable-length-patterns-and-relationship-uniqueness).
- **`count(x)` counts bindings, not distinct values.** `DISTINCT` is not in the grammar at
  all, so this is consistent rather than surprising. An audit script computing reachable
  _nodes_ will disagree with the engine, and the engine is right.

### 3.2 Grammar limits (documented)

From [`docs/cypher.md`](cypher.md) — the `item` and statement productions are the full
surface. **No arithmetic operators** (`BinaryOperator` has only comparisons and boolean
logic), no `DISTINCT`, no `WITH`, no subqueries, no `UNWIND … MATCH` (only
`UNWIND … CREATE`/`RETURN`), no stored procedures. `MATCH` and `MERGE` pattern properties
must be literals, checked at parse time.

### 3.3 Architecture limits (documented)

- **Reads are not parallel.** Every page touch goes through one global pool mutex.
  Measured: 16 threads reach 0.6–1.4% of single-thread throughput. Real parallelism needs
  per-frame latching — `ROADMAP.md` item 5. **Do not report this as a regression**: it is
  the known, measured state, with the target recorded.
- **One writer, any number of readers.** A reader and a writer exclude each other
  (`ROADMAP.md` item 1 for the versioned-visibility fix that would change that).
- **A WAL with interior corruption loses everything after the damaged frame, silently.**
  Per `FORMAT.md` §7 this is spec: a CRC mismatch is treated as a torn tail. Measurements
  and what closing the gap would take are in the same section. **Not a bug to fix**; it is
  a format change to propose.
- **The action queue is capped** (`DEFAULT_MAX_TRANSACTION_ACTIONS`, 4M) and overflow is a
  hard error, not an automatic flush — flushing would break atomicity. Opt-in spilling to
  the WAL is available. Rationale: `AGENTS.md` §1.
- **A WAL replay stops at a torn tail**, and a read-only open refuses rather than serving
  stale data when the WAL holds unreplayed pages.

### 3.4 Out of scope by decision

`ROADMAP.md` §"Explicitly out of scope": distributed/multi-node operation, in-memory graph
mode, `WITH`/subqueries/stored procedures, full-text and vector indexes.

### 3.5 Not defects, though they look like one

- **`:memory:` takes no lock** and multiple `:memory:` handles coexist by design.
- **`get_node` / `get_edge` are lossy** (storage errors fold into `None`). Use
  `try_get_node` / `try_get_edge`. This is contractual, not sloppiness.
- **A whole-number float property keeps a `.0` in `dump_cypher`.** That is required for the
  round trip to preserve the type; without it `3.0` reparses as `Int(3)`.
- **Node and edge ids share a numbering space.** Anything keyed on a bare `u64` is a bug —
  in _your_ code, not the engine's.

---

## 4. Not yet covered

Stated so a full audit can aim here rather than re-walk §2.

1. **Long-run memory behaviour.** The differential test runs 400k operations but does not
   watch RSS. No multi-hour soak exists; nothing asserts that resident memory stays bounded
   _over time_.
2. **The 64 GiB file limit.** Enforced before any write (24-bit property pointers), but the
   boundary itself has never been approached — the largest real dataset here is 4.34 GB.
3. **Platforms other than macOS.** Every instrument above was run on macOS only. CI covers
   Ubuntu and macOS for `cargo test`, but not the instruments; Windows is untested and the
   lock code has an explicit Windows concern (see `src/lock.rs`).
4. **WAL format byte-level fuzzing.** `page_boundary_bench` covers page layout; the WAL
   frame layout has targeted tests but no fuzzer that mutates frames systematically
   (only the torn-tail/interior-damage cases in `FORMAT.md` §7).
5. **Binding concurrency semantics.** Both SDK suites are single-threaded; whether the
   releases are sound under an event loop / threads is not established.
6. **`backup` under adversarial timing.** Backup takes a checkpoint then copies under the
   write lock. Covered for consistency; not covered for a concurrent writer racing the
   target path, or a copy that fails partway (disk full mid-copy).
7. **Vacuum/reclamation over time.** Slot and page reclamation is tested for correctness;
   no test asserts that a long create/delete cycle converges to a bounded file size.

---

## 5. For an auditor: where the leverage is

Ranked by the yield of the passes that produced §1:

1. **Combinations, not lines.** Mode × operation × pool size × direction. Build the matrix,
   find the empty cells.
2. **Assert contents, not counts.** Every defect in §1 except two was invisible to a count
   assertion.
3. **Compare independent implementations.** Two SDKs, model vs engine, Cypher vs adjacency
   cursors. Self-confirmation is the failure mode to avoid.
4. **Ask external-failure questions.** Full disk, read-only volume, truncated file, missing
   WAL — and after the fault clears, is the database intact?
5. **Read the specification before calling something a defect.** One pass here produced a
   wrong recommendation (that standard Cypher permits relationship reuse) which, if acted
   on, would have made a conforming engine non-conforming.
