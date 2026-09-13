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
line; this is the same engine and the same author. It is renamed because the
`graphlite` name was taken on all three registries by unrelated projects —
`GraphLite-AI/GraphLite` on crates.io, `eugene-eeo/graphlite` on PyPI, and a
monitoring package on npm — so publishing under it would have delivered someone
else's package, and a published name cannot be cleanly retracted.

`nervusdb` is owned by this project on crates.io (the older, unrelated `0.0.x`
releases there are yanked) and was free on PyPI and npm. It is the only candidate
that needed no per-ecosystem compromise, which is why all three now carry it.

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

### Removed

- **Five public functions that no caller could reach.** An audit of the public surface
  found 20+ functions with no call anywhere in the repository; these are the ones that
  were removed rather than kept and tested:

  - `Node::set_prop`, `Node::add_label`, `Node::remove_label`, `Edge::set_prop`. These
    mutated the in-memory `Node`/`Edge` struct — a value returned by `get_node` /
    `get_edge` — and **nothing ever wrote it back**. `node.set_prop("k", v)` therefore
    changed a local copy and persisted nothing, silently. That contradicts the engine's
    central invariant (`DiskGraph` is the single source of truth; no in-memory value is
    primary state), so removing them is a fix, not only a cleanup. Use
    `NervusDb::update_node_property` / `update_edge_property`.
  - `json::write_raw_json_string` — zero callers, including none internally.
  - `IndexManager::new` — byte-for-byte equivalent to the derived `Default`, which
    `IndexManager::default()` still provides.
  - `IndexManager::{edge_types, label_index_count, property_index_count}` — all three had
    no caller at all; `NervusDb::edge_types()` reads the persisted catalog directly and
    never went through the first.

  **If you called any of these, the compiler will point at the line.** None were exposed
  by the Python or Node.js bindings, and none were documented.

### Changed

- **`NervusDb::wal_path()` is now used by the test suite.** It was documented and
  correct but untested; three tests built `{path}.wal` by hand instead. They now call the
  accessor, so the path rule has one implementation.

- **A point read (`get_node`) now takes the buffer-pool mutex once instead of four
  times** — the node record, its property payload, and its outgoing and incoming edge
  chains each used to be a separate acquisition. No API or format change; this only
  shortens how long a reader holds the one global lock.

  Measured on a reproducible synthetic instrument
  (`cargo bench --bench concurrency_scaling_bench`, 200k nodes / 600k edges, 4096-frame
  pool, 98.7% cache hit so the cause is not disk I/O): **8 threads went from 191–208k
  to 396k ops/s (2.0×)**, with the single-thread number unchanged. The unchanged
  single-thread row is the control: reducing critical-section count cannot help one
  thread, and it does not.

  **Reads are still serialized** — one global mutex remains, and throughput still
  falls as threads are added (894k → 396k from 1 to 8). See `ROADMAP.md` item 5.

### Added

- **`NervusDbOptions::spill_transaction_actions` — transactions larger than memory.**
  With it enabled, a transaction whose action queue exceeds
  `max_transaction_actions` no longer fails: the queued actions are written to the WAL
  as `ActionWrite` frames (WAL tag 6) and only an 8-byte-per-action location index stays
  resident, so a single transaction can exceed the in-memory cap without giving up
  rollback. The shape is the one STEAL spilling already uses for pages.

  **Off by default, and the reasons are the point:**

  - While a transaction has spilled actions, **`Checkpoint` is refused** (`GraphError`)
    rather than silently truncated. A checkpoint truncates the WAL, and those frames
    _are_ the transaction's unapplied work.
  - An automatic checkpoint that is deferred for this reason does **not** fail the
    commit that triggered it. The commit is already durable; reporting failure would
    invite a retry that duplicates data.
  - WAL size grows with the transaction until it commits or rolls back.

  **Action-queue errors are unchanged when the option is off** (the default), so this
  is additive. One behavioural note for anyone who was relying on the exact wording of
  the batch-overflow error: `add_nodes` / `add_edges` now enqueue through the same
  `push_op` gate as the single-record methods, so they report the same
  "queue is full" message. They previously had a separate inline check — a second
  implementation of the cap that could not see this new behaviour at all.

  `FORMAT.md` documents the frame, its recovery rule (an `ActionWrite` is ignored
  unless its transaction has a matching `TxCommit`, exactly like `PageWrite`), and why
  `seq` may not be reordered.

- **Cost-based join ordering for multi-pattern `MATCH`.** A query with several patterns
  in one `MATCH` is no longer solved by expanding each pattern independently and
  multiplying. Patterns are ordered by estimated cardinality, preferring ones that share
  a variable with the patterns already solved, and a pattern whose start variable is
  already bound expands **from that node** instead of computing its full match set:

  ```text
  MATCH (h:Hub {key: 'target'}), (h)-[:R]->(c) RETURN count(*)
  ```

  The second pattern used to enumerate every `(h)-[:R]->(c)` pair in the graph before
  filtering on `h`. It now expands from the one bound node. Measured on a fixture with
  20,000 nodes and a 64-frame pool, buffer-pool misses for that query are 0–5 against
  **316** when the bound-driven path is disabled.

  **Behavioural, and worth knowing:** the **row order** of a multi-pattern result can
  differ from previous versions, because rows now come out in the planned order. Cypher
  does not guarantee an order without `ORDER BY` and the row _set_ is unchanged, but if
  you were relying on the incidental order, add `ORDER BY`.

  **The cost model's limit, stated rather than implied:** there is no per-relationship
  edge count and no degree histogram in the storage engine, so fan-out is approximated
  as the average degree `2E/N` scaled by the target label's share. It reliably separates
  an indexed lookup from a full scan and cannot reliably rank two patterns of similar
  size. It chooses **order**, not access method — the index is used when one exists
  rather than being costed against a scan. `EXPLAIN` prints each pattern's estimate
  together with whether it was measured from an index or approximated, so the numbers
  can be checked rather than trusted.

- **`UNWIND <list> AS <var>` — batch ingestion in one statement.** It expands a list
  into rows and binds each element, which is the only way to express bulk data inside
  a single query:

  ```text
  UNWIND [10, 20, 30] AS v CREATE (n:Num {v: v})   -- one statement, three nodes
  UNWIND [1, 2, 3, 4] AS x RETURN sum(x)           -- 10
  UNWIND [5,1,4] AS x RETURN x ORDER BY x SKIP 1   -- 4, 5
  ```

  Non-list input yields a single row (`UNWIND 42 AS x` binds 42), an empty list yields
  zero rows, and row order follows list order. Read-only handles accept
  `UNWIND ... RETURN` and reject `UNWIND ... CREATE`; `EXPLAIN` reports which of the
  two a statement is.

  This required pattern property values to become expressions rather than literals, so
  `CREATE (n {name: x})` can read the `UNWIND` variable. Two consequences follow, both
  deliberate: `MATCH` pattern properties are validated as literals at parse time (a
  pattern is matched before any variable is bound, so `MATCH (n {k: someVar})` can
  never be evaluated — rejecting it beats silently matching nothing), and `SET` /
  `DELETE` on a scalar binding is an error rather than a silent no-op.

- **`MERGE <pattern>` — the idempotent write.** It matches the pattern and reuses what
  it finds; only when nothing matches does it create:

  ```text
  MERGE (u:User {name: 'alice'})                            -- creates
  MERGE (u:User {name: 'alice'})                            -- reuses, still 1 node
  MERGE (a:Acct {id: 7}) ON CREATE SET a.created = 1
                        ON MATCH  SET a.seen = 9
  MERGE (x:A {k: 1})-[:R]->(y:B {k: 2})                     -- whole pattern, either way
  ```

  The point is idempotence: "check, then create" written by hand has a gap between the
  check and the write, and the duplicates that gap produces are silent — they surface
  later as a count that is too high. `is_mutating()` returns true for `MERGE`
  unconditionally, even for a run that writes nothing, because whether it writes is
  only known after matching and a shared read lock would let two concurrent `MERGE`s
  both find nothing and both create.

  Two semantics are worth stating because they are easy to get subtly wrong:

  - **Matching uses MATCH filter semantics.** Properties named in the pattern must be
    equal, but a node carrying _extra_ properties still matches. (Cypher's own
    `MERGE (person:Person) ON MATCH SET ...` example matches all six `Person` nodes.)
  - **The pattern is matched or created as a whole.** If any part is missing, the
    _entire_ pattern is created — including a second `:A {k: 1}` in
    `MERGE (x:A {k: 1})-[:R]->(y:B {k: 3})`. Cypher's `HAS_CHAUFFEUR` example makes the
    same point: it creates `Chauffeur` nodes even though `Person` nodes with those
    names already exist. Partial reuse would make the result depend on which parts
    happened to exist, which is not a rule anyone can predict from the query.

- **`NervusDb::read_snapshot()` — a self-consistent read view.** It holds the shared
  read lock for its lifetime, so a multi-step traversal (read a node's adjacency list,
  then read each of those edges) sees one state instead of stitching two together.
  Without it the second read can miss an edge the first read named:

  ```text
  node 5's adjacency list references edge 1 -> that edge reads back as absent
  ```

  This is not an internal tear — the engine's own `integrity_check`, which runs
  entirely inside one lock, stayed healthy across 22,540 rounds of concurrent writes.
  The gap was that callers had no way to get the same window. Verified by
  `tests/concurrency_isolation_tests.rs`, whose first run reproduced the dangling
  reference and whose deterministic case now fails 100% of the time if the snapshot
  stops holding the lock.

  The snapshot blocks writers while it lives, because the buffer pool and page table
  are shared mutable state — true reader/writer parallelism needs versioned page
  visibility, which remains ROADMAP item 1. The snapshot gives callers a _correct_
  option; it does not claim to be non-blocking MVCC.

- **A bound on the transaction action queue.** A transaction holds every action in
  memory until it commits, and that queue was unbounded — which contradicted the
  project's own rule that memory is bounded by the buffer pool. Measured with a real
  RSS probe: **502 bytes per node action, 128 bytes per edge action**. A one-million-
  node transaction therefore cost 479 MB, and ten million would have cost 4.8 GB.

  The queue now defaults to `DEFAULT_MAX_TRANSACTION_ACTIONS` (4,000,000 actions,
  ≈812 MB measured for an all-node worst case) and reports an error on overflow
  instead of splitting itself:

  ```text
  Transaction action queue is full (4000000 actions). ...
  Commit in batches instead, or raise the limit deliberately with
  NervusDbOptions::max_transaction_actions.
  ```

  Splitting automatically would be the easy answer and the wrong one: it means
  committing part of a transaction, so a later failure could no longer roll the whole
  thing back — and all-or-nothing is the reason to use a transaction at all. The limit
  covers every enqueue path through a single `push_op` gate.

- **`Value::List` and `Value::Null` as evaluation-time types.** Neither is written to
  disk: in Cypher, setting a property to null _removes_ it, and no current syntax can
  produce a list-valued property. `FORMAT.md` is therefore unchanged and the format
  version is unaffected by these two variants.

  `PropCodec::push_value` and `encode_props` now return `Result` instead of `()` so a
  non-storable value is reported rather than silently dropped — a `SET` that appeared
  to succeed while writing nothing is the failure mode this project keeps removing.

  Both SDKs map the new variants to their native types: Python gets `None` and `list`,
  Node.js gets JSON `null` and `Array`.

- **Two invariants that were documentation-only are now enforced by tests.**

  - **§13 slice-conversion comments.** The rule says fixed-offset slice conversions
    must state why they cannot fail. An audit found **none** of `page.rs`'s 18
    conversions had such a comment, nor did `disk_graph.rs`'s. All 46 conversions
    across the format layer now carry one, and
    `zero_dependency_tests::fixed_offset_slice_conversions_are_documented` fails when a
    new one appears without it.
  - **Test counts.** README said 187, ROADMAP and `docs/testing.md` said 190, and the
    real number was 191 — three different figures for one fact.
    `documented_suite_table_matches_the_files` now compares the `docs/testing.md` table
    against the files row by row, so a case added without updating the table fails the
    build instead of drifting.

  Both guards needed a second attempt to be worth keeping, which is recorded in their
  comments: the first slice guard matched only single-line conversions (missing 23 of 46) and then attributed doc comments to the wrong function (18 false positives); the
  first count guard derived a total that could not be made to agree with the runner. A
  guard that is blind or noisy is worse than none, because it trains people to ignore
  it.

- **A high-parallelism concurrency stress suite.** The existing stress test pinned 20
  threads, which over-subscribes an 8-core machine and under-loads a 64-core one.
  `concurrency_stress_tests.rs` scales its thread count to `available_parallelism()` and
  asserts only hardware-independent properties: every thread joins (no deadlock), the
  node count equals the sum of per-thread writes (no lost writes), the structure stays
  self-consistent, and readers make progress. Throughput is deliberately not asserted —
  AGENTS.md §3.2 records a previous test that passed locally and failed on CI because a
  cloud disk's fsync behaviour differs.

- **A version-consistency guard.** The version is written in five manifests (root and
  both binding crates, `pyproject.toml`, `package.json`). Missing one at release time
  produces an artifact that claims to be a version it was not built from, and that
  cannot be corrected after publication. A test now fails if any two disagree, with a
  negative control for the parser itself.

### Changed

- **The Python distribution name.** It was `graphlite`, which is taken on PyPI by an
  unrelated embedded graph database, so that configuration would have shipped a
  package resolving to someone else's project. It is now `nervusdb`, matching the crate
  and the npm package. The import name is unchanged (`import nervusdb`), because
  distribution name and import name are separate concepts — the standard arrangement,
  as with `beautifulsoup4` → `import bs4`.

  A test rejects any manifest that would publish under a known-taken name, naming the
  owner in the message.

- **`Transaction::remove_node` / `remove_edge` / `update_node_property` /
  `update_edge_property` now return `Result<(), GraphError>`.** Previously `()`. The
  queue bound above has to be reportable, and a silent truncation would be the exact
  failure this project keeps eliminating. Callers add `?`.

- **The real-dataset benchmarks and end-to-end verification tools moved** from
  `examples/` to `benches/real_data/`, which is what they are: acceptance instruments
  rather than user-facing examples. They now run through
  `cargo bench --bench snap_dblp_bench` and friends, and remain separately declared
  because `cargo bench` does not discover subdirectories.

- **`PLAN-1.0.md` moved to `docs/history/`** — it records the decisions taken while
  building 1.0 and no longer describes pending work.

- **The CLI, the browser workbench, and the demo binary were removed.** All three were
  separate binaries that duplicated capability the library and the SDKs already
  provide: dump, checkpoint, schema inspection and query execution are available
  through `NervusDb`. The trigger was concrete — a tool that must be kept in sync with
  the engine is a place for the two to drift, and the engine is the product.
  Inspection and scripting now go through the library, which cannot drift from itself.

  Net effect: ~1,900 lines and 11 tests removed. The engine is unchanged, verified by
  re-running the real-data acceptance afterwards (DBLP red lines bit-identical, file
  size identical at 81.26 MB).

### Fixed

- **`dump_cypher` produced a script the parser could not read back, for any negative or
  whole-number float property.** Two defects in the same round trip, which the docs
  designate as the format-migration path:

  - **Negative literals did not parse at all.** The lexer emitted `Token::Dash` for every
    `-`, and both `SET n.k = -7` and pattern properties (`{v: -7.5}`) go through
    `parse_primary_expr`, which accepts a single primary token and has no
    unary-operator concept. So `MATCH (n:N) SET n.v = -7` failed with
    `Unexpected expression token: Some(Dash)` — and `dump_cypher` writes exactly that form
    for a negative property. Measured on `v1.0.0` too: the dump succeeds, the re-import
    fails. Fixed in the lexer by folding `-` directly into a numeric literal when it is
    followed by a digit, which covers `SET`, pattern properties and comparisons at once.

  - **Float properties came back as integers.** `format_literal` rendered `Value::Float`
    with `f64::to_string()`, which gives `"3"` for `3.0` — no decimal point, so the
    re-import parsed `Int(3)`. Measured: of 60 nodes with `f = i * 1.5`, 30 (exactly the
    whole-number results) changed type. A whole-number float now keeps its `.0`.

  Arithmetic operators are a separate matter: `BinaryOperator` has only comparisons and
  boolean logic, so `n.v - 1` does not work — a scope limit, not this defect. Verified it
  behaves identically before and after this change.

- **`Transaction::add_edges` silently ignored the `edge_id` the caller supplied.**
  `EdgeInsert` is public and so is its `edge_id` field, so writing
  `EdgeInsert { edge_id: 999_000, .. }` compiles — and `add_edges` discarded it, assigned
  its own ids, and returned `[1, 2, 3, 4]`. Measured: `get_edge(999_000)` is `None`. The
  same category as the interfaces removed in 0.1.0 (they claimed a capability that had no
  effect), except this field cannot be deleted: the commit path uses it to carry already
  allocated ids. Added `EdgeInsert::new`, which does not expose the field, and documented
  the rule at both ends. Pinned by
  `tests/memory_mode_tests.rs::add_edges_assigns_ids_and_ignores_the_supplied_edge_id`,
  which asserts both halves — returned ids work, supplied ids do not exist — because
  either half alone misses a way this can go wrong.

- **`backup()` on a `:memory:` database failed with a message that pointed at the wrong
  thing.** It reported `Storage I/O error: No such file or directory`, because the copy
  step opens `db_path`, which in memory mode is the literal string `":memory:"`. A caller
  reads that as "my path is wrong" and goes looking for a typo, when the real answer is
  "this database has no file to copy". It now refuses up front, names the cause, and
  points at `dump_cypher` as the alternative.

  Worth recording why this survived: before the `:memory:` checkpoint fix above, this
  path did not fail at all — the checkpoint wrote a stray file to `":memory:"`, so
  `File::open` succeeded, `backup` **reported success**, and it copied that garbage into
  a file the caller would reasonably believe was a backup.

- **A read-only handle could write, and did — silently.** `reject_write` guarded the 12
  direct write entry points (CRUD, Cypher, checkpoint, vacuum, constraints) but no
  transaction entry point. `with_transaction(|tx| tx.add_node(..))` on a handle from
  `open_read_only` therefore returned `Ok` and persisted: measured, the WAL went from 0
  to 12429 bytes and the node was present after reopening. `begin_transaction` too.

  The consequence is worse than "a read-only handle wrote". Read-only handles take a
  **shared** lock, and shared locks do not exclude each other — that is the
  many-readers design. Once a reader could write, N read-only handles became N writers
  with **no mutex between them**. Measured: two handles committing 500 transactions
  each returned `Ok` **1000 times** and left **zero** of those writes behind, with
  `integrity_check` reporting no problem at all — it verifies graph structure, not
  whether writes landed. This is exactly the silent-multi-writer case that
  `AGENTS.md` §11 exists to forbid, reached through the read-only door.

  `DbLock::acquire_shared`'s own doc comment stated the premise ("the caller must still
  guarantee it does not write"); nothing enforced it. The guard is now in
  `begin_transaction`, the single entry point that `with_transaction` also goes
  through — not in `commit`, because by then the caller has already been told nothing is
  wrong.

- **`checkpoint()` on a `:memory:` database wrote a real file to the working
  directory, and lost the data that was supposed to be in memory.** Checkpoint
  replayed the WAL's committed pages through `StorageEngine::db_path()`, which in
  memory mode holds the literal string `":memory:"` — so the pages went into a new
  file of that name in the current directory, while the in-memory page store
  received nothing. The WAL was then truncated, permanently discarding those pages.

  Two symptoms, both silent. The directory gained a file named `:memory:` (72 MB in
  the run that found this), and after a checkpoint **every node became unreadable** —
  measured 3000/3000. `node_count()` still reported the right number, because that is
  metadata; what was lost was page content, and `get_node` is lossy, so it returned
  `None` rather than an error. Replay now goes through `DiskManager`, which already
  selects file or memory. Pinned by `tests/memory_mode_tests.rs`.

- **A buffer pool could deadlock with every frame free.** STEAL eviction only
  accepted _dirty_ uncommitted frames, so once a page had been spilled to the WAL and
  subsequently read back — clean, but still uncommitted — it matched neither eviction
  round: round one rejects it as uncommitted, round two as not dirty. With enough such
  frames the pool reported `NO-STEAL enforced` while holding 246 of 256 idle frames.
  Reproduced with 512 frames / 100k nodes / 200k edges, where a documented benchmark
  scenario (1M nodes + 4M edges in a 1 MB pool) failed outright. Clean uncommitted
  frames are now evictable too, which is safe because their authoritative image is in
  the WAL and `wal_pages` records where.

- **`cargo bench --bench throughput` no longer ran.** The 10M-node scenario queues 10M
  actions in one transaction, which exceeds the 4M `DEFAULT_MAX_TRANSACTION_ACTIONS`
  cap; the benchmark aborted before measuring anything. It now lifts the cap for
  itself (chunking would silently change the workload the published figures describe)
  and honours `GL_MAX_ACTIONS` so a constrained machine can still see the rejection.

All six predate this release — the `:memory:`, NO-STEAL, read-only, `add_edges` and
both literal-round-trip defects reproduce on `v1.0.0` — and none changes the storage
format.

- **Read concurrency was _negative_: more threads made reads slower.** Measured on
  com-DBLP, 16 threads doing plain point reads reached **0.6%–1.4% of single-thread
  throughput**. The cause was lock traffic, not the machine: every page touch goes
  through one `Arc<Mutex<BufferPoolManager>>`, and a `get_node` took it once per
  incident edge, so a degree-343 hub cost ≈345 acquisitions per read. A whole
  adjacency chain is now walked inside **one** acquisition. Under 8-thread contention
  on a shared hub this measured 1.4–2.2× (17.4–27.8k → 37.8–38.3k ops/s), and the
  "after" runs varied by under 2% against 60% before.

  Control runs ruled out the measuring environment before the cause was accepted:
  duplicate variants doing a pure CPU spin and taking only the _outer_ read lock both
  scaled to ≈40% at 16 threads in the same process, so the collapse is attributable to
  the buffer-pool mutex. Method and both tables: `docs/benchmarks.md`.

  **Readers are still serialized**, because the mutex remains global and is still taken
  once per call — 16-thread efficiency is ≈4% against the ≈40% the machine can deliver.
  Real parallelism needs per-frame latching, which is a buffer-pool redesign (ROADMAP
  item 1). The documentation no longer claims otherwise.

- **A failed multi-record statement left part of its work behind.** A write statement
  that failed partway through kept its already-written pages in the buffer pool, and
  the _next_ successful commit flushed them to disk — including the records the failure
  was supposed to prevent:

  ```text
  CREATE (:Num {v: 1}); declare UNIQUE (:Num.v)
  UNWIND [2, 1, 3] AS v CREATE (n:Num {v: v})   -- fails: 1 already exists
  CREATE (:Other {x: 99})                        -- an unrelated successful write
  -- reopen: MATCH (n:Num) returns {v: 1} AND {v: 2}
  ```

  `NervusDb::execute` and `run_cypher` now snapshot the allocator metadata, open a
  transaction context, and undo a failed statement the same way `Transaction::commit`
  undoes a failed transaction: restore the uncommitted pages from their baselines, roll
  back the in-memory metadata, and invalidate the secondary indexes.

  This mattered little while a statement could only write a handful of records; with
  `UNWIND`, a single statement routinely writes thousands, and a failure partway
  through is the realistic failure mode.

- **Unique constraints did not survive closing and reopening the database.**
  `create_unique_constraint` mutated the in-memory `index_catalog` but never called
  `sync_header()`, so the change never reached Page 0 or the WAL —
  `commit_dirty_pages_to_wal` only commits pages marked modified, and none were:

  ```text
  db.create_unique_constraint("P", "k")
  [reopen]
  db.unique_constraints()      -> []       (the constraint is gone)
  CREATE (:P {k: 1})           -> succeeds (the duplicate is silently accepted)
  ```

  Labels in the same catalog always persisted, because `DiskGraph::add_node` ends with
  `sync_header()`; the constraint path simply omitted it.

  This is worse than a constraint that errors, because the caller stops doing its own
  duplicate checking on the belief that the engine is doing it. Found while checking a
  different claim — that a 1.0.0 database opens unchanged in 1.1.0 — and it turned out
  the format was fine and the constraint handling was not. Three tests now cover
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
  produced `count(c.name) = 1` where 2 is correct. `Value` now has a real `Null`
  variant, so the two are distinct, and the test suite asserts both halves.

- **`sum()` / `avg()` / `min()` / `max()` over an `UNWIND` variable returned the row
  count.** Aggregate arguments were accumulated as the literal `Int(1)` whenever the
  argument was a variable, because before `UNWIND` a variable could only bind a node or
  an edge. So `UNWIND [1,2,3,4] AS x RETURN sum(x)` returned `4` rather than `10`. A
  variable binding now aggregates its own value, and aggregating an entity (a `sum(n)`
  where `n` is a node) is an explicit error instead of a plausible-looking number.

- **`sum()` lost precision on integers above 2^53.** The aggregate computed its total
  through `f64` and cast back to `i64`, and an `f64` mantissa holds only 53 bits:

  ```text
  write 9007199254740993 (2^53 + 1)  ->  sum() reads back 9007199254740992
  two such values, expected 18014398509481986  ->  reads 18014398509481984
  ```

  Off by one, with no indication. `i64::MAX + 1` also saturated silently. All-integer
  sets now accumulate with `checked_add` and report an overflow error rather than
  saturating or wrapping. Mixed int/float sets still return `Float`, and an empty set
  still returns `Int(0)` — both unchanged.

- **Nodes created by a statement were invisible to its own `RETURN`.**
  `UNWIND ['a'] AS s CREATE (m:Str {s: s}) RETURN m.s` returned `null` for a value that
  had just been written to disk. `CREATE` now binds the nodes it creates back into the
  row, so `RETURN` reads the data it just stored. (`MATCH ... CREATE ... RETURN b.x`
  has the same gap from before this release; it is not addressed here.)

- **Two documentation claims did not match the implementation.** `AGENTS.md` §10 and
  `docs/architecture.md` §10 described reads running "concurrently under page-level
  latches", and the module maps advertised "page latches". There is no page-level
  latching: `BufferPoolManager::latch` is declared and initialized but never read or
  written anywhere — dead code since the initial commit. The claims now state what the
  code does, with the measured numbers, rather than what it was meant to do. (That
  field has since been deleted outright.)

---

Earlier releases under the previous name (`v1.0.0` and its release candidates) are
archived in [docs/history/CHANGELOG-legacy.md](docs/history/CHANGELOG-legacy.md).
They describe the GraphLite line, which wrote the `GLDB` magic; this build refuses to
reinterpret those files.

[0.1.0]: https://github.com/ysankpia/nervusdb/releases/tag/v0.1.0
