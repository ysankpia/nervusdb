# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Because this is a database, two kinds of change are called out explicitly, as a
user has to act on them:

- **Storage format** — the on-disk layout changed. Old files may not open. The
  migration path is a logical dump and re-import (see README).
- **Behavioural** — existing queries or APIs behave differently.

---

## [0.1.0] — 2026-09-13

**First release as `NervusDB`.** The project was called GraphLite through the 1.0.0
line; this is the same engine and the same author, renamed because `graphlite` was
already taken on crates.io (GraphLite-AI), PyPI (eugene-eeo) and npm by unrelated
projects. Shipping under it would have delivered someone else's package, and a
published name cannot be cleanly retracted.

The version restarts at `0.1.0` rather than continuing from `1.1.0`: under this name
nothing has ever been released, so claiming `1.x` would assert a history that does not
exist. `0.x` states plainly that the API can still move.

**Storage format — the one exception to the freeze.** `DB_PAGE_VERSION` is now `5`
and the Page 0 magic is `NVDB` (was `GLDB`). **No page layout changed**; the 4-byte
magic is the only difference. Files written by the 1.0.0/1.1.0 line are still
_recognised_: opening one reports the magic change and the migration path rather than
claiming the file is not a database. Migrate by dumping with the older build
(`dump_cypher`) and re-importing; there is no CLI. That dump/re-import is the only
upgrade action the rename itself requires.

**Upgrade actions required:**

- `Transaction::remove_node`, `remove_edge`, `update_node_property` and
  `update_edge_property` now return `Result<(), GraphError>` instead of `()`. Add `?`
  (Rust) or expect an exception (Python `RuntimeError`, Node `Error`).
- A transaction is capped at 4,000,000 queued actions by default. A transaction
  larger than that now fails instead of consuming unbounded memory; if you genuinely
  need one, either commit in batches or set
  `NervusDbOptions::max_transaction_actions`.

No Cypher statement that worked under the old name behaves differently. Two previously
accepted constructs are now rejected, both because they were silently wrong:
`MATCH`/`MERGE` pattern properties written as expressions (they could never be
evaluated, so they matched nothing) and `SET`/`DELETE` applied to a scalar binding.

- **Two invariants that were documentation-only are now enforced by tests.**
  Neither was hypothetical:

  - **§13 slice-conversion comments.** The rule says fixed-offset slice conversions
    must state why they cannot fail. An audit found **none** of `page.rs`'s 18
    conversions had such a comment, nor did `disk_graph.rs`'s. All 46 conversions
    across the format layer now carry one, and
    `zero_dependency_tests::fixed_offset_slice_conversions_are_documented` fails when
    a new one appears without it.
  - **Test counts.** README said 187, ROADMAP and `docs/testing.md` said 190, and the
    real number was 191 — three different figures for one fact.
    `documented_suite_table_matches_the_files` now compares the `docs/testing.md`
    table against the files row by row, so a case added without updating the table
    fails the build instead of drifting.

  Both guards needed a second attempt to be worth keeping, which is recorded in their
  comments: the first slice guard matched only single-line conversions (missing 23 of 46) and then attributed doc comments to the wrong function (18 false positives);
  the first count guard derived a total that could not be made to agree with the
  runner. A guard that is blind or noisy is worse than none, because it trains people
  to ignore it.

### Added

- **`UNWIND <list> AS <var>`** — expands a list into rows, the only way to express
  bulk data in one statement: `UNWIND [1,2,3] AS x CREATE (n:Num {v: x})`.

- **`MERGE <pattern>`** — the idempotent write, with `ON CREATE SET` / `ON MATCH SET`.

- **`NervusDb::read_snapshot()`** — a self-consistent read view; pin one state across
  a multi-step traversal instead of stitching two together.

- **A bound on the transaction action queue** and the `NervusDbOptions::max_transaction_actions`
  setting that controls it.

- **A high-parallelism concurrency stress suite** that sizes itself to the machine, and
  a **version-consistency guard** over the five manifests that carry the version.

### Changed

- **The Python distribution is renamed to `nervusdb`; `import nervusdb` is
  unchanged.** PyPI's `nervusdb` belongs to an unrelated embedded graph database
  (eugene-eeo/nervusdb), as does npm's; crates.io's `nervusdb` belongs to
  NervusDb-AI. Publishing under it would have shipped a package that resolves to
  someone else's project — and a published name cannot be cleanly retracted.

  `nervusdb` is available on crates.io, PyPI and npm (checked 2026-09-13), so it
  is the one candidate needing no per-ecosystem compromise. Distribution name and
  import name are deliberately allowed to differ, which is the standard arrangement
  (`beautifulsoup4` → `import bs4`).

  A test now rejects any manifest that would publish under a known-taken name, with
  the owner in the message, and requires the three manifests to stay traceable to the
  same stem.

### Fixed

- **Read concurrency was _negative_: more threads made reads slower.** Measured on
  com-DBLP, 16 threads doing plain point reads reached **0.6%–1.4% of single-thread
  throughput**. The cause was lock traffic, not the machine: every page touch goes
  through one `Arc<Mutex<BufferPoolManager>>`, and a `get_node` took it once per
  incident edge, so a degree-343 hub cost ≈345 acquisitions per read. A whole
  adjacency chain is now walked inside **one** acquisition. Under 8-thread
  contention on a shared hub this measured 1.4–2.2× (17.4–27.8k → 37.8–38.3k ops/s),
  and the "after" runs varied by under 2% against 60% before.

  Control runs ruled out the measuring environment before the cause was accepted:
  duplicate variants doing a pure CPU spin and taking only the _outer_ read lock both
  scaled to ≈40% at 16 threads in the same process, so the collapse is attributable
  to the buffer-pool mutex. Method and both tables: `docs/benchmarks.md`.

  **Readers are still serialized**, because the mutex remains global and is still
  taken once per call — 16-thread efficiency is ≈4% against the ≈40% the machine can
  deliver. Real parallelism needs per-frame latching, which is a buffer-pool redesign
  (ROADMAP item 1). The documentation no longer claims otherwise.

- **Two documentation claims did not match the implementation.** `AGENTS.md` §10 and
  `docs/architecture.md` §10 described reads running "concurrently under page-level
  latches", and the module maps advertised "page latches". There is no page-level
  latching: `BufferPoolManager::latch` is declared and initialized but never read or
  written anywhere — dead code since the initial commit. The claims now state what the
  code does, with the measured numbers, rather than what it was meant to do.

### Fixed

- **Unique constraints did not survive closing and reopening the database.**
  `create_unique_constraint` mutated the in-memory `index_catalog` but never called
  `sync_header()`, so the change never reached Page 0 or the WAL —
  `commit_dirty_pages_to_wal` only commits pages marked modified, and none were.
  Measured before the fix:

  ```text
  db.create_unique_constraint("P", "k")
  [reopen]
  db.unique_constraints()      -> []       (the constraint is gone)
  CREATE (:P {k: 1})           -> succeeds (the duplicate is silently accepted)
  ```

  Labels in the same catalog always persisted, because `DiskGraph::add_node` ends
  with `sync_header()`; the constraint path simply omitted it.

  This is worse than a constraint that errors, because the caller stops doing its own
  duplicate checking on the belief that the engine is doing it. Found while checking
  a different claim — that a 1.0.0 database opens unchanged in 1.1.0 — and it turned
  out the format was fine and the constraint handling was not. Three tests now cover
  reopen, explicit checkpoint, and joint persistence with labels; removing the
  `sync_header()` call fails all three.

- **A property whose value is the string `"null"` was treated as an absent value.**
  `null` was represented internally as the _string_ `"null"`, so the two were
  indistinguishable:

  ```text
  CREATE (c:Character {name: 'null'})   -- a real value
  MATCH (c) RETURN count(c.name)        -- returned 0, silently skipping it
  ```

  Every `count`, `sum`, `avg`, `min` and `max` over such a property under-counted.
  Verified before the fix: a node with `name = 'null'` plus one with `name = 'real'`
  produced `count(c.name) = 1` where 2 is correct.

  `Value` now has a real `Null` variant, so the two are distinct, and the test suite
  asserts both halves: `"null"` as a value counts like any other, and `avg` / `min`
  over an empty set return `Null`.

- **A high-parallelism concurrency stress suite.** The existing stress test pinned
  20 threads, which over-subscribes an 8-core machine and under-loads a 64-core one.
  `concurrency_stress_tests.rs` scales its thread count to `available_parallelism()`
  and asserts only hardware-independent properties: every thread joins (no deadlock),
  the node count equals the sum of per-thread writes (no lost writes), the structure
  stays self-consistent, and readers make progress. Throughput is deliberately not
  asserted — AGENTS.md §3.2 records a previous test that passed locally and failed on
  CI because a cloud disk's fsync behaviour differs.

- **A version-consistency guard.** The version is written in five manifests (root and
  both binding crates, `pyproject.toml`, `package.json`). Missing one at release time
  produces an artifact that claims to be a version it was not built from, and that
  cannot be corrected after publication. A test now fails if any two disagree, with a
  negative control for the parser itself.

- **A bound on the transaction action queue.** A transaction holds every action in
  memory until it commits, and that queue was unbounded — which contradicted the
  project's own rule that memory is bounded by the buffer pool. Measured with a real
  RSS probe: **502 bytes per node action, 128 bytes per edge action**. A one-million-
  node transaction therefore cost 479 MB, and ten million would have cost 4.8 GB.

  The queue now defaults to `DEFAULT_MAX_TRANSACTION_ACTIONS` (4,000,000 actions, ≈812 MB
  measured for an all-node worst case) and reports an error on overflow instead of
  splitting itself:

  ```text
  Transaction action queue is full (4000000 actions). ...
  Commit in batches instead, or raise the limit deliberately with
  NervusDbOptions::max_transaction_actions.
  ```

  Splitting automatically would be the easy answer and the wrong one: it means
  committing part of a transaction, so a later failure could no longer roll the whole
  thing back — and all-or-nothing is the reason to use a transaction at all. The limit
  covers every enqueue path (`add_node`, `add_edge`, `add_nodes`, `add_edges`,
  `remove_node`, `remove_edge`, `update_node_property`, `update_edge_property`)
  through a single `push_op` gate.

  **Breaking change:** the four methods that previously returned `()` —
  `Transaction::remove_node`, `remove_edge`, `update_node_property`,
  `update_edge_property` — now return `Result<(), GraphError>` so an overflow can be
  reported. Callers add `?`. Verified the real-data red lines are unchanged (hub 1-hop
  10,080 / 2-hop 161,877, 81.26 MB, 580k edges/s).

- **`NervusDb::read_snapshot()` — a self-consistent read view.** It holds the shared
  read lock for its lifetime, so a multi-step traversal (read a node's adjacency
  list, then read each of those edges) sees one state instead of stitching together
  two. Without it the second read can miss an edge the first read named:

  ```text
  node 5's adjacency list references edge 1 -> that edge reads back as absent
  ```

  This is not an internal tear — the engine's own `integrity_check`, which runs
  entirely inside one lock, stayed healthy across 22,540 rounds of concurrent
  writes. The gap was that callers had no way to get the same window. Verified by
  `tests/concurrency_isolation_tests.rs`, whose first run reproduced the dangling
  reference and whose deterministic case now fails 100% of the time if the snapshot
  stops holding the lock.

  The snapshot blocks writers while it lives, because the buffer pool and page table
  are shared mutable state — true reader/writer parallelism needs versioned page
  visibility, which remains ROADMAP item 1. The snapshot gives callers a _correct_
  option; it does not claim to be non-blocking MVCC.

- **`MERGE <pattern>` — the idempotent write clause.** It matches the pattern and
  reuses what it finds; only when nothing matches does it create:

  ```text
  MERGE (u:User {name: 'alice'})                            -- creates
  MERGE (u:User {name: 'alice'})                            -- reuses, still 1 node
  MERGE (a:Acct {id: 7}) ON CREATE SET a.created = 1
                        ON MATCH  SET a.seen = 9
  MERGE (x:A {k: 1})-[:R]->(y:B {k: 2})                     -- whole pattern, either way
  ```

  The point is idempotence: "check, then create" written by hand has a gap between
  the check and the write, and the duplicates that gap produces are silent — they
  surface later as a count that is too high. `is_mutating()` returns true for
  `MERGE` unconditionally, even for a run that writes nothing, because whether it
  writes is only known after matching and a shared read lock would let two
  concurrent `MERGE`s both find nothing and both create.

  Two semantics are worth stating because they are easy to get subtly wrong:

  - **Matching uses MATCH filter semantics.** Properties named in the pattern must
    be equal, but a node carrying _extra_ properties still matches. (Cypher's own
    `MERGE (person:Person) ON MATCH SET ...` example matches all six `Person`
    nodes.)
  - **The pattern is matched or created as a whole.** If any part is missing, the
    _entire_ pattern is created — including a second `:A {k: 1}` in
    `MERGE (x:A {k: 1})-[:R]->(y:B {k: 3})`. Cypher's `HAS_CHAUFFEUR` example makes
    the same point: it creates `Chauffeur` nodes even though `Person` nodes with
    those names already exist. Partial reuse would make the result depend on which
    parts happened to exist, which is not a rule anyone can predict from the query.

- **A failed multi-record statement left part of its work behind.** A write
  statement that failed partway through kept its already-written pages in the
  buffer pool, and the _next_ successful commit flushed them to disk — including
  the records the failure was supposed to prevent. Measured before the fix:

  ```text
  CREATE (:Num {v: 1}); declare UNIQUE (:Num.v)
  UNWIND [2, 1, 3] AS v CREATE (n:Num {v: v})   -- fails: 1 already exists
  CREATE (:Other {x: 99})                        -- an unrelated successful write
  -- reopen: MATCH (n:Num) returns {v: 1} AND {v: 2}
  ```

  `NervusDb::execute` and `run_cypher` now snapshot the allocator metadata, open a
  transaction context, and undo a failed statement the same way `Transaction::commit`
  undoes a failed transaction: restore the uncommitted pages from their baselines,
  roll back the in-memory metadata, and invalidate the secondary indexes.

  This mattered little while a statement could only write a handful of records; with
  `UNWIND`, a single statement routinely writes thousands, and a failure partway
  through is the realistic failure mode.

- **`sum()` / `avg()` / `min()` / `max()` over an `UNWIND` variable returned the row
  count.** Aggregate arguments were accumulated as the literal `Int(1)` whenever the
  argument was a variable, because before `UNWIND` a variable could only bind a node
  or an edge. So `UNWIND [1,2,3,4] AS x RETURN sum(x)` returned `4` rather than `10`.
  A variable binding now aggregates its own value, and aggregating an entity (a
  `sum(n)` where `n` is a node) is an explicit error instead of a plausible-looking
  number.

- **Nodes created by a statement were invisible to its own `RETURN`.** `UNWIND ['a']
AS s CREATE (m:Str {s: s}) RETURN m.s` returned `null` for a value that had just
  been written to disk. `CREATE` now binds the nodes it creates back into the row,
  so `RETURN` reads the data it just stored. (`MATCH ... CREATE ... RETURN b.x` has
  the same gap from before this release; it is not addressed here.)

- **The CLI (`nervusdb-cli`) and the browser workbench (`nervusdb-studio`), plus
  the demo binary (`nervusdb`).** All three were separate binaries that
  duplicated capability the library and the SDKs already provide: dump, checkpoint,
  schema inspection and query execution are all available through `NervusDb`, and
  the Python and Node.js SDKs expose them too.

  The trigger was concrete: a tool that must be kept in sync with the engine is a
  place for the two to drift, and the engine is the product. Inspection and
  scripting now go through the library, which cannot drift from itself.

  Net effect: ~1,900 lines and 11 tests removed, 13 suites → 11. The engine is
  unchanged — verified by re-running the real-data acceptance afterwards, with the
  DBLP red lines matching bit for bit (hub 1-hop 10,080 / 2-hop 161,877) and the
  file size identical at 81.26 MB.

- The real-dataset benchmarks and end-to-end verification tools moved from
  `examples/` to `benches/real_data/`, which is what they are: acceptance
  instruments rather than user-facing examples. They now run through
  `cargo bench --bench snap_dblp_bench` and friends, and remain separately
  declared because `cargo bench` does not discover subdirectories.

- `PLAN-1.0.md` moved to `docs/history/` — it records the decisions taken while
  building 1.0 and no longer describes pending work.

### Added

- **`Value::List` and `Value::Null` as evaluation-time types.** Neither is written
  to disk: in Cypher, setting a property to null _removes_ it, and no current syntax
  can produce a list-valued property. `FORMAT.md` is therefore unchanged and the
  format version stays 4 — verified by re-running the real-dataset acceptance, whose
  red lines (hub 1-hop 10,080 / 2-hop 161,877) and 81.26 MB file size are identical.

  `PropCodec::push_value` and `encode_props` now return `Result` instead of `()` so a
  non-storable value is reported rather than silently dropped — a `SET` that appeared
  to succeed while writing nothing is the failure mode this project keeps removing.

- Both SDKs map the new variants to their native types: Python gets `None` and
  `list`, Node.js gets JSON `null` and `Array`.

- **`UNWIND <list> AS <var>` — the batch-ingestion clause.** It expands a list into
  rows and binds each element, which is the only way to express bulk data inside a
  single statement:

  ```text
  UNWIND [10, 20, 30] AS v CREATE (n:Num {v: v})   -- one statement, three nodes
  UNWIND [1, 2, 3, 4] AS x RETURN sum(x)           -- 10
  UNWIND [5,1,4] AS x RETURN x ORDER BY x SKIP 1   -- 4, 5
  ```

  Non-list input yields a single row (`UNWIND 42 AS x` binds 42), an empty list
  yields zero rows, and row order follows list order. Read-only handles accept
  `UNWIND ... RETURN` and reject `UNWIND ... CREATE`; `EXPLAIN` reports which of
  the two a statement is.

  This required pattern properties to become expressions rather than literals, so
  `CREATE (n {name: x})` can read the `UNWIND` variable. Two consequences follow
  from that, both deliberate: `MATCH` pattern properties are validated as literals
  at parse time (a pattern is matched before any variable is bound, so
  `MATCH (n {k: someVar})` can never be evaluated — rejecting it beats silently
  matching nothing), and `SET` / `DELETE` on a scalar binding is an error rather
  than a silent no-op.

### Fixed

- **`sum()` lost precision on integers above 2^53.** The aggregate computed its
  total through `f64` and cast back to `i64`, and an `f64` mantissa holds only 53
  bits. Measured:

  ```text
  write 9007199254740993 (2^53 + 1)  ->  sum() reads back 9007199254740992
  two such values, expected 18014398509481986  ->  reads 18014398509481984
  ```

  Off by one, with no indication. `i64::MAX + 1` also saturated silently.

  All-integer sets now accumulate with `checked_add` and report an overflow error
  rather than saturating or wrapping. Mixed int/float sets still return `Float`,
  and an empty set still returns `Int(0)` — both unchanged.

---

Earlier releases under the previous name (`v1.0.0` and its release candidates) are
archived in [docs/history/CHANGELOG-legacy.md](docs/history/CHANGELOG-legacy.md).
They describe the GraphLite line, which wrote the `GLDB` magic; this build refuses to
reinterpret those files.

[0.1.0]: https://github.com/ysankpia/nervusdb/releases/tag/v0.1.0
