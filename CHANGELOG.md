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

## [Unreleased]

### Fixed

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
  GraphLiteOptions::max_transaction_actions.
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

- **`GraphLite::read_snapshot()` — a self-consistent read view.** It holds the shared
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
  visibility, which remains ROADMAP item 1. The snapshot gives callers a *correct*
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
    be equal, but a node carrying *extra* properties still matches. (Cypher's own
    `MERGE (person:Person) ON MATCH SET ...` example matches all six `Person`
    nodes.)
  - **The pattern is matched or created as a whole.** If any part is missing, the
    *entire* pattern is created — including a second `:A {k: 1}` in
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

  `GraphLite::execute` and `run_cypher` now snapshot the allocator metadata, open a
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

- **The CLI (`graphlite-cli`) and the browser workbench (`graphlite-studio`), plus
  the demo binary (`graphlite`).** All three were separate binaries that
  duplicated capability the library and the SDKs already provide: dump, checkpoint,
  schema inspection and query execution are all available through `GraphLite`, and
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

## [1.0.0] — 2026-09-12

### Added

- **`graphlite-studio` — a local browser workbench.** `graphlite-studio novel.db`
  opens the database read-only, serves a force-directed graph on
  `127.0.0.1:<random port>`, and opens your browser.

  The page is embedded in the binary (`include_str!`) and references **no external
  resources**, so it works offline — a local database tool that cannot render its
  own UI on a plane would be absurd. Layout, pan/zoom, drag, label filtering and a
  read-only Cypher console are all hand-written vanilla JS. The repulsion step uses
  a spatial grid: the naive O(n²) version is 4 million distance computations per
  frame at 2,000 nodes, which locks the browser.

  The HTTP server is hand-written over `std::net::TcpListener` — three GET routes
  did not justify letting a dependency into the tree that `zero_dependency_tests`
  is guarding. It binds **127.0.0.1 only**: the server has no authentication, so
  binding elsewhere would publish the database.

  Export is capped at 5,000 nodes and requests above that are **refused with an
  explanation**, not silently clamped. A silently truncated graph reads as "this is
  the whole picture", which is the wrong impression to leave.

- **The file lock is now taken per request, not held for the process lifetime.**
  This is the difference between the studio being usable and not: shared read locks
  and the write lock are mutually exclusive, so holding a read lock for as long as
  the UI is open would block the agent that is writing — the exact scenario the
  tool exists for. Found by end-to-end testing: while the studio ran, a writer in
  another process was refused.

  A writer can now write between requests, and the studio observes the change on
  its next request. **This is still not concurrent read-write**: if the writer
  happens to hold the lock during a request, that request gets a 503 and can be
  retried. True concurrency needs snapshot isolation, which is a different order of
  change and is recorded in `ROADMAP.md` rather than pretended here.

- **`Transaction::add_nodes` / `Transaction::add_edges`** in the core, exposed as
  `tx.add_nodes(...)` / `tx.add_edges(...)` in both SDKs. One boundary crossing and
  one lock acquisition per batch instead of per record.

- **Correction: the SDKs were never 9x slower than the native path.** The recorded
  "Python 63k, Node 64k vs Rust 550k ops/s" figures came from **debug** builds of
  the bindings compared against a **release** core. Measured with both sides in
  release, on 50,000 nodes with properties:

  | Path              | Throughput    |
  | ----------------- | ------------- |
  | Rust, file-backed | 382,000 ops/s |
  | Python            | 355,000 ops/s |
  | Node.js           | 326,000 ops/s |

  Same script, debug versus release binding: 78,603 vs 499,599 ops/s — a 6.4x
  difference that the old figure was attributing to the FFI boundary. Direct
  measurement of the boundary itself: 0.037 s crossing and parsing a 50,000-node
  batch versus 0.095 s committing it to disk. The commit is the cost.

  This means the batch API is **not** the large win it was planned as. It is kept
  for ergonomics and to avoid one lock acquisition per record, and the docs now say
  exactly that rather than implying a throughput claim.

  The SDK benchmarks now require `BUILD_PROFILE=release` and print the profile,
  and both warm up before measuring — the first run is 3-4x slower than steady
  state (Node measured 68k on a cold run and 241k-276k across three warm runs),
  which is enough to mistake warmup for a performance difference.

- **`db.backup(path)` — a consistent online copy.** The sequence is what makes it
  consistent: checkpoint first (so the data file becomes the single authoritative
  snapshot and the WAL is empty), then copy while holding the write lock (so no
  writer can interleave), then fsync. Copying a WAL that still held half a
  transaction is the failure mode this ordering avoids.

  The copy is a complete database, not a read-only snapshot: it opens
  independently, retains multi-page overflow properties, and accepts writes. An
  empty WAL is created beside it so the two-file invariant holds for the copy too.

  It **refuses to overwrite an existing file** — the value of a backup is having a
  second copy, so silently replacing a previous one could destroy the only good
  one. Backing up onto the source path is refused for the same reason.

- **`db.vacuum()` — reports reclaimable space.** Record slots were already
  reclaimed on delete (new nodes immediately reuse deleted slots), so the honest
  answer is a measurement rather than a compaction claim. `vacuum` checkpoints to
  converge state and returns a `VacuumReport`: live counts, file size, and how many
  whole property and overflow pages are on the free chains.

  **It does not truncate the file, and says so.** Page numbers are a
  logical-to-physical map, so truncating would require rewriting that map — the one
  operation that could corrupt addressing. That trade is stated in the report's
  documentation rather than left as a surprise for someone expecting `VACUUM` to
  shrink their file.

- **Unique constraints**: `db.create_unique_constraint("Character", "name")` makes a
  `(label, property)` pair's values unique across every node carrying that label.
  Violations raise `GraphError::UniqueConstraintViolation` — a distinct variant so
  callers can tell an expected data conflict from a general failure.

  This is the last line of defence for data cleanliness. Without it, a buggy writer
  or a retrying agent can create two nodes for one entity while queries return only
  half the data, and nothing surfaces the problem until much later.

  Three details that matter:

  - **Declaring a constraint over existing duplicates fails**, naming the nodes that
    conflict. Discovering the conflict at the next write instead would point the
    error at the wrong place — the writer rather than the historical data.
  - **Updating a node to its own current value is allowed**; the check excludes the
    node being modified, so a no-op update is not mistaken for a self-conflict.
  - **Constraints persist** in the Page 0 index catalog and still apply after a
    restart. `db.unique_constraints()` lists them.

  Implementation reuses the existing `(label, property)` index rather than adding a
  parallel structure, and when that index is not yet built the write is **refused**
  rather than allowed through — optimistically permitting a write would make the
  constraint silently meaningless.

- **Multiple readers can now share a database while a single writer holds it.**
  Previously exactly one handle could open a file, so a background process writing
  and a foreground process observing were mutually exclusive — the most common
  shape of an agent-plus-UI workload.

  Read-only handles take a **shared** lock and coexist; a write handle takes the
  exclusive lock and excludes everyone. The kernel enforces it (`try_lock_shared`),
  so there is no spinning or retry loop. `GraphLite::open_read_only(path)` is the
  entry point, and `GraphLiteOptions::read_only` covers the options-based path.

  The subtlety is that **a reader must not trigger WAL replay**, since replay
  writes the data file. So a read-only open first checks
  `StorageEngine::pending_replay_pages()`: if the WAL still holds committed pages
  that have not reached the data file, the open fails and says to open once with a
  read-write handle. Silently skipping those pages would return stale data — the
  failure a reader is least equipped to notice.

  Write entry points on a read-only handle return a clear error naming the cause
  rather than attempting the write. Covered by two tests: reader/writer mutual
  exclusion in both directions including lock release on drop, and the pending-WAL
  refusal followed by a successful read-only open after one replay.

- **`LIMIT` is pushed down into matching when it is safe to do so.** The engine
  used to expand every match and truncate at the very end, so `LIMIT 1` cost as
  much as the full query. It now stops as soon as enough rows are collected.

  Measured on a 4-hub graph:

  | Query                    | Before    | After    |
  | ------------------------ | --------- | -------- |
  | `LIMIT 1`, 32,000 edges  | 60,577 ms | **6 ms** |
  | `LIMIT 1`, 100,000 edges | —         | 53 ms    |

  Push-down is gated on three conditions, and the gate is conservative on purpose:

  - **No `ORDER BY`** — sorting needs every row, so truncating early would return
    the wrong prefix.
  - **No `SKIP`** — the cut-off point would be computed against the wrong offset.
  - **No aggregate, and no `RETURN *`** — both need the full row set (the latter
    because the column list is derived from the contexts).

  Equivalence was verified by running each query twice — once with `LIMIT`, once
  without — and asserting the limited result equals the first N rows of the full
  result, including the cases where push-down is refused.

- **`EXPLAIN <query>`** prints the plan without running the query: the start-node
  selection (label index, property index, or full scan), the expansion steps with
  direction, type and hop range, whether `LIMIT` was pushed down and why not if it
  was not, and whether aggregation or sorting is involved.

  It exists because a slow query used to offer no way to see which path the engine
  took — the O(N²) expansion in the previous commit could only be diagnosed by
  reading the source. Every line reflects a branch the executor actually takes, so
  the plan cannot drift from the behaviour without the code moving.

  `EXPLAIN` is side-effect free and therefore works on a read-only handle, and
  `EXPLAIN CREATE ...` / `EXPLAIN ... SET ...` are permitted — they describe the
  write path without performing it. Pinned by a test asserting `node_count` is
  unchanged after `EXPLAIN CREATE`, and that no property is modified by
  `EXPLAIN ... SET`.

- Scalar functions `id(x)`, `labels(n)`, and `type(r)` in `WHERE` and `RETURN`.
  `id` returns the internal node or edge id as an integer, which is what start-node
  anchoring will key on.

- `test_cli_rejects_unparsable_trailing_input` asserts both halves of the fix: the
  error is reported, **and** the malformed statement produced no side effects —
  a prefix must not be executed while the tail is dropped.

- **Format version 4 — the frozen format, and the core library now has zero
  runtime dependencies. This is a breaking change: databases written by
  `v1.0.0-rc.2` (version 3) or any earlier version will not open.**

  WAL frames and Page 0 metadata were encoded by `bincode`. That crate **ceased
  maintenance in December 2025** — its final release contains only a compiler
  error and a notice. The bytes on disk must not be owned by someone else's
  release schedule, so the format is now defined and implemented in this
  repository (`src/codec.rs`, `src/json.rs`, `src/crc32.rs`), with every byte
  specified in the new `FORMAT.md`. `serde`, `serde_json`, `crc32fast`, and
  `thiserror` are gone as well.

  Timing was the whole point: the format was not yet frozen, so the swap cost
  nothing. Once frozen there is no second free opportunity, and the database's
  lifetime would have been tied to an abandoned crate with no security updates —
  the same shape of risk that left Kuzu's users stranded when that project was
  archived. `tests/zero_dependency_tests.rs` now enforces the invariant, including
  a negative test confirming that adding a dependency makes the guard fail.

  **To migrate**: export with the older build via `.dump`, then re-import.

- **`FORMAT.md` is new, and it is a commitment.** It specifies every offset, the
  record layouts, the WAL frame and payload encoding, and the limits — so a future
  version or a third-party reader can be written without reverse-engineering the
  code. The stability promise is stated there in the same terms SQLite uses: the
  format does not change in incompatible ways, and a future change that genuinely
  cannot be expressed will be opt-in (a DuckDB-style storage-version selector),
  never a silent reinterpretation.

- **Format version 3 — page-level CRC32 coverage for the whole data file.**

  Previously only WAL frames were checksummed; a bit flip or half-written page in
  `{path}` was silently returned as "not found" and the graph quietly came back
  wrong. Every data page now carries a CRC32: pages `1..255` inline in Page 0,
  pages `>= 256` through a two-level `CrcDirPage` radix directory. A directory
  page that covers ~4 GiB of address space costs 4 KiB, so the smallest database
  still fits in 16 KiB (4 pages).

  Checksums are computed lazily, only when a page is written to `{path}` or read
  back, so the write hot path pays nothing. A stored checksum of `0` means "not
  recorded" and the page is skipped rather than reported — after a crash the
  server must not refuse to start over a page it simply never got to.

- **The 24-bit property-pointer overflow is now a hard error instead of a silent
  corruption.** `pack_prop_ptr` used `debug_assert!` plus a `& 0x00FFFFFF` mask;
  `debug_assert!` is compiled out in release builds, so a page number beyond the
  24-bit limit (a file over 64 GiB) would have been **truncated to a wrong page,
  returning another entity's data with no error at all** — the worst failure mode
  a database can have. The packer now returns `Result` and `open` refuses an
  oversized file up front.

  This is documented as a permanent limit in `FORMAT.md` §6. It is deliberate:
  widening the pointer would grow `NodeRecord` from 32 to 40 bytes, dropping each
  page from 128 records to 102 — a 20% capacity loss on the hot path, paid by
  every deployment, to buy address space the target workload does not use.

- **The format and size gates now run before any write, including WAL replay.**
  Both live at the top of `open_with_options`. Replay writes the main data file,
  so a check placed after it would already have reinterpreted an old file under
  current-version semantics. `test_version_guard_rejects_v1_v2` now asserts the
  rejected file comes out byte-identical to how it went in.

- Directory pages are self-checksummed (`self_crc`, sealed on write, verified on
  load). Without this, one silently corrupted L2 page would report every data
  page it covers as a mismatch — thousands of false alarms naming innocent pages.

- **`integrity_check()` now names the corrupt pages.** It sweeps every page's
  checksum first and reports `PageChecksumMismatch` with the page number, before
  any content-level check runs. Previously a bad page produced a vague "node
  count mismatch" plus a wall of dangling-edge errors that pointed at healthy
  edges. A new `PageUnreadable` kind distinguishes an I/O failure from a
  checksum mismatch rather than asserting the latter.

- **`GraphLite::open_with_options` / `GraphLiteOptions`**: `buffer_pool_frames`
  and `wal_auto_checkpoint_bytes`. Setting the latter (default 64 MB) makes the
  engine checkpoint automatically once the WAL grows past it, so a long-lived
  writer cannot accumulate an unbounded WAL.

  **The default costs throughput on bulk ingest, and that is a deliberate
  trade.** Measured on LiveJournal edge ingestion (1 GiB pool, 10 M edges):
  447k ops/s with auto-checkpoint off, 232k with the 64 MB default — roughly
  half, because each automatic checkpoint flushes and fsyncs the whole dirty set
  on top of whatever rhythm the caller already has. Bulk loaders that checkpoint
  on their own schedule should set `wal_auto_checkpoint_bytes: 0`; that is what
  the shipped benchmarks do, and they print the setting so the number and the
  configuration stay together. The default stays on for the general case, where
  the alternative is an unbounded WAL.

- Page-level WAL replay is now **streaming**. `WalCursor` walks the WAL frame by
  frame and applies committed pages through a callback, so peak memory during
  recovery is O(number of transactions) instead of O(size of the WAL). A 1.5 GB
  WAL no longer has to be materialised in memory to recover.

- `CrcStore` and the `crc` module are public, so an embedder can verify a page
  on disk (`GraphLite::verify_page_on_disk`) without opening the buffer pool.

- `docs/` for depth: `architecture.md` (paging, WAL, STEAL, slotted pages, batch
  weave, Cypher, indexing, algorithms, concurrency, production safety, storage
  versioning), `benchmarks.md` (measured results with their conditions plus the
  correction notice), `testing.md` (the suite and the adversarial style).
  Root keeps only the files a reader expects.
- `CHANGELOG.md` (this file).
- `ROADMAP.md` — planned work, explicit non-goals, and the rule that any
  performance claim must ship with a runnable scenario and its measurement
  conditions.
- `LICENSING.md` — plain-language explanation of the dual-licence model,
  including that AGPL does not prohibit commercial use.
- `CONTRIBUTING.md`, `CLA.md`.
- `SECURITY.md` — how to report a vulnerability privately.
- Reproducible benchmarks: `benches/throughput.rs`, `benches/pool_probe.rs`,
  `benches/mem_probe.rs`, plus `benches/throughput.py` and
  `benches/throughput.mjs` for the SDKs. Each scenario prints its own
  configuration, so a number cannot be quoted without its conditions.
- GitHub Actions CI (format, check, clippy, tests, release-mode throughput
  suites, rustdoc) on Linux and macOS.

---

### Changed

- **Checkpoint ordering is now load-bearing and documented.** `sync_header()`
  runs before `flush_crc()`: `CrcStore` is the last writer of Page 0 (it owns the
  inline CRC array and `crc_dir_root`), so writing the header afterwards would
  silently erase the checksums.

- **Long edge batches are chunked at `MAX_BATCH_EDGES_IN_MEMORY` (100,000).**
  Two-phase batch weaving builds O(batch) hash tables and record vectors; a
  ten-million-edge transaction previously grew those without bound, breaking the
  promise that resident memory is set by the buffer pool rather than by input
  size. Chunk boundaries are transparent — the next chunk's "previous head" is
  the head the previous chunk wrote — and `edge_locality_tests` pins the
  equivalence.

- **`try_get_node` / `try_get_edge` are the error-preserving readers**;
  `get_node` / `get_edge` remain lossy and are now documented as such. Internal
  scans propagate read errors instead of skipping unreadable pages, which is what
  previously let a corrupt page shrink the graph silently.

- **Committed data appeared to be lost after a hard crash (SIGKILL).**
  `StorageEngine::open` replays the WAL into the main data file, and it must run
  before `DiskManager::open` — replay extends the file, and the page high-water
  mark is derived from its length. So replay happens while no `CrcStore` exists,
  and the pages it writes keep their **previous** checksums. Every later read of
  such a page reported a checksum mismatch, and because `get_node` is lossy the
  caller saw a _missing node_ — the data was on disk the whole time.

  `for_each_committed_page` now walks the committed frames once more after the
  CRC store is attached and refreshes their checksums in memory; the existing
  `flush()` persists them. Cost is one sequential pass over the WAL at open.

  Getting a regression test that actually reproduces this required a real
  `SIGKILL`: a normal `drop` flushes the buffer pool and writes the checksums
  along with the pages, hiding the defect. `test_wal_replay_refreshes_page_checksums`
  now forks a child that commits batches and kills it with `kill -9`. With the
  fix reverted it fails with 128 unreadable nodes on a single page; with the fix
  it is clean.

- Benchmark examples and both SDK benchmarks take `DATASET_PATH`, `DATASET_DIR`,
  `DB_DIR`, `DB_PATH` and `POOL_FRAMES` instead of hardcoded absolute paths, and
  print what to set when a dataset is missing rather than panicking.

- **Hub selection in the SNAP benchmarks is now an explicit total order**
  (degree descending, then raw id ascending). It previously sorted by degree
  alone, and in com-DBLP three nodes tie at degree 164 _exactly at rank 50_ —
  so which hub was the fiftieth depended on sort internals, and the same data
  produced 2-hop totals of 161,789 / 161,877 / 162,158. All three are "correct"
  for their own hub set, which made the number impossible to compare across runs.

  An independent reimplementation over the raw dataset (union of the neighbours
  of every node adjacent to a hub) gives 161,877, and the benchmark now reports
  exactly that.

- **README rewritten and slimmed from 759 to ~265 lines.** It had grown into a
  reference manual: 13 architecture subsections inline, an 80-line CLI
  walkthrough, a per-case description of every test, and a 70-line file tree. It
  now follows the shape well-regarded embedded databases use — what it is, a
  runnable example, features, then links — with the depth split out into `docs/`.

- **Contribution policy: this repository no longer accepts external code.**
  Pull requests from anyone other than the owner, a member or a collaborator are
  closed automatically. The reason is licensing (dual AGPL + commercial), not
  code quality: merging an outside patch licensed under AGPL alone would make
  that code unusable for the commercial licence and break the model
  project-wide. Issues remain welcome and are acted on. See CONTRIBUTING.md.

  _If you were planning to send a patch, please open an issue with a
  reproduction instead._

- `CLA.md` now applies **only** to changes the maintainer explicitly invites.
  Opening a pull request on your own is no longer, and is not treated as,
  acceptance of it.

- The README no longer describes the project as "production-grade". The release
  notes and roadmap list known production gaps, so the claim contradicted them.
  It now states plainly that this is a release candidate and links to the
  limitations.

### Fixed

- **A corrupted length prefix could request a multi-gigabyte allocation.**
  `decode_props` and `decode_node_data` called `with_capacity(count)` where `count`
  came straight from a varint on disk. A damaged or crafted 4-byte value would ask
  for gigabytes — an out-of-memory abort or a capacity-overflow panic instead of a
  diagnosable error.

  Both now bound the count against the bytes actually remaining, which is the same
  guard `codec.rs`, `StringDict::decode`, and `IndexCatalog::decode` already used.
  Every entry costs at least a one-byte length prefix, so a count larger than the
  remaining payload cannot be legitimate.

- **Read errors were reported as empty results.** Two paths folded a storage
  failure into a "nothing here" answer:

  - `find_initial_candidates` used `all_node_ids().unwrap_or_default()`, so a read
    error became an **empty candidate set** and the query returned 0 rows.
  - `algo::has_cycle` / `find_cycles` folded the same error into `false` / an empty
    list, so a damaged database would answer "no cycles" without having read
    anything.

  Measured: truncating a database to one third of its size made
  `MATCH (n:T) RETURN count(*)` return **0** where 3,000 nodes had been — with no
  error at all. The caller sees "this graph is empty"; the truth is "a page could
  not be read".

  The query path now propagates the error. For the algorithm path, the existing
  signatures are part of the public API, so instead of changing them,
  `try_has_cycle` and `try_find_cycles` were added alongside — matching the
  `get_node` / `try_get_node` split the project already uses — and the lossy
  variants are documented as such.

- **`has_cycle()` and `find_cycles()` aborted the process on long chains.** Both
  used recursive DFS, so recursion depth equalled path length. A 60,000-node chain
  — legitimate data, and the natural shape of a citation or chapter chain — blew
  the thread stack:

  ```text
  thread 'main' has overflowed its stack
  fatal runtime error: stack overflow, aborting
  ```

  That is an **uncatchable abort**: a host application cannot `catch_unwind` it, and
  the process dies. For an embedded database, letting valid data crash the process
  is not an acceptable failure mode.

  Both are now iterative with an explicit heap stack, so memory scales with the
  data rather than with the stack limit. Verified on chains of 60,000 and 200,000
  nodes, including the cyclic case, and confirmed by restoring the recursive
  implementation and watching the test abort with SIGABRT.

- **Deleting a property-less edge inflated the database file to 64 GiB.** The
  single-edge insert path used `INVALID_PAGE_ID` to mean "no properties", but that
  constant is `u32::MAX` — numerically identical to the `PROP_PTR_OVERFLOW`
  sentinel, which means "properties live in the overflow chain rooted at page
  `0x00FFFFFF`".

  `remove_edge` therefore treated a **nonexistent page 16,777,215** as an overflow
  chain: it pushed it onto the overflow freelist, marked it dirty, wrote it to the
  WAL, and on checkpoint materialised it at file offset 68,719,472,640.

  Measured: after deleting one property-less edge, the file went from 12 KB to
  **68,719,476,736 bytes** — exactly the format limit documented in `FORMAT.md`.
  `backup()` and `vacuum()` would carry that size along.

  The batch weaving path always used the correct `PROP_PTR_NONE` (0), so only
  single-edge insertion was affected. That is also why the extensive edge test
  suites never caught it: they exercise the batch path.

- **`LIMIT` push-down silently discarded `WHERE`, returning wrong rows.** The
  fast path added for `LIMIT` used the predicate only to narrow the _candidate
  start nodes_ via the index; it never applied the row filter, which the normal
  path does with a trailing `retain(eval_expr_truthy)`.

  Measured on five nodes with `age` 10..50:

  ```text
  MATCH (n:P) WHERE n.age < 30 RETURN n.age LIMIT 5  ->  5 rows (expected 2)
  MATCH (n:P) WHERE id(n) = 3 RETURN n.age LIMIT 1   ->  10   (expected 30)
  ```

  The verification written at the time missed it because every query it compared
  had **no** `WHERE` clause — it only checked "with LIMIT" against "without LIMIT",
  and both were wrong in the same way. The regression test now covers seven
  predicate shapes (comparisons, `AND`/`OR`, `id()`), and asserts that the cap
  counts only rows that pass the filter, so `LIMIT 10` cannot return fewer rows
  than exist.

- **Unique constraints were enforced on only two of five write paths.**
  `GraphLite::add_node` and `update_node_property` checked them; `Cypher CREATE`,
  `MATCH ... CREATE`, and `Transaction::commit` all call `DiskGraph::add_node`
  directly and bypassed the check entirely.

  Measured: after declaring `(:C {name})` unique, `CREATE (x:C {name:'林渊'})`
  silently inserted a duplicate, as did a transaction — while the Rust API
  correctly refused. A constraint that only holds for some callers is worse than
  no constraint, because it implies the data is clean.

  The check now lives on `IndexManager` (which owns both the constraint set and
  the index needed to evaluate it) and all five paths route through it.

- **A failed transaction made a constrained label permanently unwritable.**
  `invalidate_all()` downgrades every label index to `Registered`, and the
  constraint guard treated "index not built" as a violation. So after one
  rejected duplicate, _every_ subsequent write to that label failed — including
  perfectly valid values:

  ```text
  CREATE (x:C {name:'林渊'})   -> rejected (correct)
  CREATE (y:C {name:'苏晴'})   -> rejected (wrong: not a duplicate)
  ```

  The guard now rebuilds the index on demand instead of refusing. The original
  reasoning ("if uniqueness cannot be verified, do not write") had the right
  intent but the wrong remedy: refusing is only correct if the index can never be
  rebuilt, and it can.

- **Two places violated the project's own invariant 13: `.lock().unwrap()` in
  library code.** `get_or_allocate_node_page` and `get_or_allocate_edge_page` used
  it for a page-number cache. `AGENTS.md` forbids that pattern outright and
  `src/sync_ext.rs` exists to replace it, so the documented guarantee was false as
  shipped. Both now use `lock_recover`: these caches hold rebuildable hints, so
  recovering from a poisoned lock costs at most one cache miss, while panicking
  ends the process.

- **Two pointer-traversal loops had no cycle guard**, against invariant 10's rule
  that every `while curr != 0` walk carry a `seen` set. The incoming-chain walk in
  `remove_edge` and the node page-directory walk could both spin forever on a
  corrupted chain. The other eight walks in the same file already had guards, and
  `walk_free_chain` right below the second one has both a guard and a step cap —
  the omission was an inconsistency rather than a design choice.

- **Documentation described the old format in three places**, and one of them
  contradicted itself: `docs/architecture.md` said "the data file has no page
  checksums" (section 11) while section 13 of the same file described the page
  checksums, and both the version number and the section heading still said
  version 3. `SECURITY.md` repeated the no-checksums claim and also stated that
  only one handle may open a database, which stopped being true when shared read
  locks landed. A reader could have concluded the release's central safety property
  did not exist.

- **A doc comment in the public API quoted retracted benchmark figures.**
  `Transaction::add_nodes` said "Python ~42k ops/s vs native ~550k". Those were the
  debug-vs-release artifacts retracted in this same release; the corrected
  release-vs-release measurement is 355k vs 382k. The comment now states the
  corrected numbers and what the method is actually for (lock traffic, not
  throughput).

- **Test counts were stale in four documents**, and `docs/testing.md` omitted three
  suites entirely (studio, zero-dependency, equivalence) while understating others
  (production safety said 12, it has 22). All now match the tree: 146 cases across
  13 suites.

- Two claims were made stronger than the evidence supported: a "100GB graph in a
  4MB pool" (the largest dataset ever exercised is 4.34 GB) and the LiveJournal
  figure appearing as two different numbers in `AGENTS.md` and
  `docs/benchmarks.md` without noting they came from different releases. The first
  now cites what was measured; the second carries its version.

- **A graph with roughly 80 or more distinct labels lost its entire schema on
  reopen.** `sync_header` wrote the label/edge-type dictionary into a **single**
  page, and `PropertyPage::encode` silently truncates payloads beyond
  `MAX_PAYLOAD` (4088 bytes, about 140 short labels). The read side then failed to
  decode the truncated dictionary — and swallowed the error with
  `if let Ok(..)`, leaving the dictionary empty.

  Measured: 70 labels survived, **80 did not**. On reopen `db.labels()` returned
  nothing and edge types degraded to the fallback, so the damage presented as "this
  database simply has no schema" rather than as corruption.

  Two changes, because either alone would have been a partial fix:

  - Metadata (dictionary and index catalog) is now written as a **chain** of
    overflow pages, chunked at `MAX_PAYLOAD`, removing the one-page ceiling
    entirely. Verified from 70 to 2000 labels.
  - All four metadata decode sites now **return an error** instead of ignoring it.
    A corrupt dictionary must be reported, not silently reinterpreted as an empty
    schema — the same rule as `AGENTS.md` §12 for reads.

  Found during the v1.0.0 release audit by testing a dimension the suite had never
  touched: every existing test used a handful of labels. The regression test covers
  both sides of the old threshold (70 and 200) plus a size far beyond one page
  (2000), and reverting the fix makes it fail.

- **Opening a non-database file silently destroyed it.** `check_format_version`
  passed through any file whose magic did not match — the comment said "let the
  later path handle it", and the later path initialised it as a **new database**,
  writing Page 0 over whatever was there.

  Measured before the fix: an 8 KB file of arbitrary bytes opened successfully, and
  after a single write its first 8 KB were overwritten. `open` is never expected to
  be destructive, so this was release-blocking.

  The criterion is now deliberately narrow: only a **nonexistent** path or a
  **zero-length** file is treated as a new database. Everything else with a
  non-GraphLite header is refused with an error that says why and what to do.

  A 4 KiB all-zero file is refused too. It is the shape most likely to be mistaken
  for "an empty database", but it is equally likely to be truncated data from
  something else, and guessing wrong here destroys a file.

  `test_open_refuses_non_database_files_without_modifying_them` asserts both halves:
  the open fails, **and** the file's bytes are unchanged — the second part is what
  actually pins the defect. It also checks the two legitimate new-database shapes
  still work, so the check is not merely over-tightened.

- **`graphlite-studio` died when its stdout reader went away.** `println!` panics
  if the write fails, and that panic happened on the main thread — so piping the
  output anywhere that stops reading (a test harness, `head`, a log collector)
  killed the whole server, and clients saw `ConnectionReset`.

  Reproduced locally with `graphlite-studio db 300 | head -3`: the server exited.
  Startup output now goes through a helper that ignores write errors, and the same
  applies to the stderr paths (a closed stderr pipe panics identically).

  The test harness had the matching defect: it read stdout only until it found the
  port, then dropped the reader, closing the pipe. It now keeps draining both
  streams for the life of the child. This was latent on macOS, where startup output
  usually fit in the pipe buffer before the reader closed, and failed reliably on
  Linux CI.

- **A 1.8x write-throughput regression introduced by the zero-dependency
  conversion.** Replacing `crc32fast` with a hand-written byte-at-a-time CRC32
  made checksumming the bottleneck: every WAL frame computes two CRCs (the page
  and the payload), and the naive table lookup cost **7,028 ns per 4 KiB page**
  against `crc32fast`'s **323 ns** — 21.8x slower, using hardware CRC32
  instructions (`SSE4.2` + `PCLMULQDQ`) that process 8 bytes per operation.

  Measured on LiveJournal, 20 million edges, interleaved runs on the same machine:

  |                             | Edge ingestion    |
  | --------------------------- | ----------------- |
  | before (byte-at-a-time CRC) | 136,865 ops/s     |
  | after (slicing-by-8 CRC)    | **280,426 ops/s** |
  | rc.2 baseline (`crc32fast`) | 297,634 ops/s     |

  The fix is slicing-by-8: a second table lets the loop consume 8 bytes per
  iteration and merge their contributions in one pass, taking the page from
  7,028 ns to 1,750 ns. It stays pure software and dependency-free; hardware
  instructions would need `std::arch` intrinsics plus runtime CPU feature
  detection, which is a larger change than this regression warrants.

  The first measurement of the replacement was wrong in a way worth recording:
  timing `hash()` in a loop over the _same_ buffer let the optimiser collapse the
  repeated work, reporting 545 MB/s for an implementation that actually ran at
  142 MB/s. The trustworthy number came from timing the real call pattern
  (`Hasher::new` + `update` + `finalize` per page).

  `slicing_matches_bytewise_for_all_lengths` pins the new path against a
  byte-at-a-time reference across 0..=24 bytes plus large sizes. An off-by-one in
  the slicing table would still produce a plausible-looking checksum — one that
  would then declare every existing page corrupt — so this equivalence is the
  property that matters, not any single known value.

- **A read-only handle could still write, via `run_cypher`.** Every write entry
  point except this one had the read-only guard: `add_node`, `add_edge`, `execute`
  and `checkpoint` were covered, but `run_cypher`'s write branch was missed —
  because `mutating` is only known after parsing, so the guard cannot sit at the
  top of the function, and it was overlooked.

  Consequence: `GraphLite::open_read_only(...)` would happily execute
  `run_cypher("CREATE ...")`, `SET`, `DELETE` and `DETACH DELETE`. Anything built
  on it (the studio's Cypher console, for instance) could modify a database it was
  supposed to only read.

  Found by the studio's end-to-end test, not by the unit tests, which had no case
  for "read-only handle runs a write statement through `run_cypher`".

  `test_read_only_handle_rejects_every_write_path` now enumerates **every** write
  entry point rather than sampling one, because the omission happened while adding
  guards one by one — so the verification has to be one by one too. Reverting the
  fix makes it report five leaking paths, which is how the test was confirmed to
  catch what it claims to.

- **`WHERE` conditions containing a function call were silently discarded, so
  the filter became "always true" and the query returned every row.** This is the
  worst class of bug a database can have — wrong answers with no error.

  `WHERE id(a) = 1` parsed as the bare variable `id`: the parser recognised `id`
  as an identifier, saw that the next token was neither `.` nor `:`, returned
  `Variable("id")`, and never consumed `(a) = 1`. Evaluation then fell through to
  the catch-all and treated it as true.

  Measured before the fix, on a 5-node graph:

  | Query                                  | Expected | Returned |
  | -------------------------------------- | -------- | -------- |
  | `WHERE id(a) = 1`                      | 1 row    | 5 rows   |
  | `WHERE id(a) = 99` (id does not exist) | 0 rows   | 5 rows   |
  | `WHERE notafunction(a) = 1`            | 0 rows   | 5 rows   |

  Two changes close it, because either alone would be a patch rather than a fix:

  1. **The parser now rejects trailing input.** After a complete statement only an
     optional semicolon is allowed. SQLite and Postgres behave the same way, for
     the same reason: a query that returns wrong data is far worse than one that
     fails.
  2. **`id()`, `labels()`, and `type()` are actually implemented**, in both `WHERE`
     and `RETURN`. Unknown function names are a parse error naming the supported
     set, rather than a silent no-op.

  A side effect worth noting: `tests/cli_tests.rs` had a multi-line script whose
  first line lacked a semicolon, so it concatenated two statements into
  `CREATE (...) RETURN a`. The old parser dropped the `RETURN a` and the test
  passed; the strict parser failed it immediately. **The test had encoded the
  bug**, and its script was corrected rather than the check being relaxed.

- **A 1-hop expansion over a high-degree node was O(D²) instead of O(D).**
  `node_matches_pattern` called `graph.get_node`, which returns a full `Node`
  _including its adjacency lists_ — so checking one label walked the node's entire
  outgoing and incoming chains. Since `match_path_step` calls it for every
  candidate at every step, a hub of degree D cost D² per expansion.

  It now reads only the record and the property payload. Measured on a 4-hub
  graph, doubling the edge count:

  |                                 | Before    | After     |
  | ------------------------------- | --------- | --------- |
  | 1,000 edges                     | 55 ms     | 1 ms      |
  | 8,000 edges                     | 2,385 ms  | 6 ms      |
  | 32,000 edges                    | 48,249 ms | **27 ms** |
  | 32,000 edges, `WHERE id(a) = 1` | 73,576 ms | **21 ms** |

  Growth is now linear in edge count rather than quadratic — 32× the edges costs
  27× the time. The 32k-edge case improved by roughly 1,800×, and the filtered case
  by roughly 3,500×.

  A regression test pins the complexity, not just the result: a test that only
  checks correctness passes at any speed, which is how this survived.

- **`crc_dir_root` was never persisted, so checksums beyond page 256 were never
  actually verified.** Three faults overlapped: `CrcStore` read-modify-wrote
  Page 0's disk bytes for the inline CRCs while `sync_header()` wrote the same
  page through a buffer-pool frame (two writers, last one wins); `flush_crc()`
  never wrote the root at all; and `CrcStore::flush()` re-read Page 0 from disk,
  so a stale read silently erased a root that a buffer-pool frame was still
  holding. `CrcStore` now keeps the inline array in memory and writes Page 0 once,
  last, containing both the inline CRCs and the root.

  The register that masked this was the coverage test: it corrupted the last page
  of the file, which is a _directory_ page (self-protected, not a data page), and
  a 40,000-node fixture only reached 475 pages, so the mid-file page it fell back
  to was still inside the inline range. The fixture is now large enough that a
  mid-file page is provably beyond 256.

- **Two further CRC-directory defects, both only reachable once the directory
  outgrows its 64-page cache** (≈65,000 data pages). The unit suites could not
  reach them; the LiveJournal run did.

  1. **The root page number was still not written to Page 0.** After the fix
     above, `flush()` gated the Page 0 update on "something changed this round".
     Once the directory existed and a round merely rewrote pages that already had
     entries, neither flag was set, Page 0 was skipped, and `crc_dir_root` stayed
     `0` on disk — the entire chain unreadable on the next open. It now re-pins
     its two Page 0 fields whenever a directory exists, which is idempotent and
     costs one 4 KiB read-modify-write per checkpoint.
  2. **Directory pages were written back unsealed during cache eviction.**
     Eviction wrote the page without recomputing `self_crc`, so a page sealed in
     an earlier round went to disk holding new contents and an old checksum. The
     next load reported it corrupt, which is exactly how the LiveJournal ingest
     aborted: `CRC directory page 65255 is corrupt (self-checksum mismatch)`.

  `test_crc_directory_survives_churn_beyond_cache` drives `CrcStore` across
  300,000 pages with repeated rewrites. Each fix was confirmed by reverting it
  alone and watching that test fail (`root on disk = 0`, and the corrupt-page
  error respectively) — the first attempt at a reproducer passed with the fix
  removed, so it proved nothing until the interleaving was corrected.

- `all_node_ids()` used `if let Ok(fid) = bpm.fetch_page(pid)`, so a page that
  failed its CRC was skipped and the caller saw a _smaller graph_ instead of an
  error. Now propagated.

- The degree-conservation test exercised the wrong failure class. It flipped a
  pointer byte and expected the degree oracle to catch it, but the page CRC now
  catches that case first — which is correct behaviour, and it left the oracle
  untested. The test now recomputes the page's checksum after corrupting it,
  modelling a _write-path bug_ (self-consistent page, valid CRC) rather than
  media corruption, and asserts as a precondition that the corruption really did
  pass the CRC.

- **`pip install graphlite` and `npm install graphlite-node` were documented but
  neither package is published.** Both binding READMEs now say so explicitly and
  give the build-from-source steps instead.

- Stale content removed from the docs: they still narrated "1.0 added four test
  suites, 1.1 added three more" when there is a single version, and the test
  description still claimed the batch speedup is ">20x" long after that
  assertion was replaced with a machine-independent one. `ROADMAP.md` also
  referred to the release as `v1.0.0` rather than `v1.0.0-rc.1`, and quoted "92
  tests" without noting one is intentionally ignored.

- **CI was red on the `v1.0.0-rc.1` tag.** Two checks failed that local
  verification had missed, both because the local toolchain was older than CI's:
  `rustdoc -D warnings` rejected unescaped angle brackets in doc comments
  (`<expr>`, `<NodeId>`), and a newer clippy flagged
  `self.ops.drain(..).collect()` in the commit path (a needless allocation).
  Fixed the code rather than pinning an older compiler.

  _If you build the `v1.0.0-rc.1` tag itself, expect those two lint failures;
  they are doc-comment and lint-level only and do not affect the library. Use
  `main` or a later tag._

## [1.0.0-rc.1] — 2026-09-11

First public pre-release. Everything below is new; the engine was built and
verified in this cycle.

### Added — storage kernel

- Single-file storage: `{path}` plus a page-level WAL `{path}.wal`. Two files
  only, no sidecar.
- 4KB physical paging with an LRU buffer pool, page pin/unpin counting, dirty
  tracking and page-level latches.
- Fixed-size records with O(1) physical addressing: `NodeRecord` 32 bytes,
  `EdgeRecord` 64 bytes.
- Disk-native index-free adjacency: double-cyclic edge pointer chains traversed
  through the buffer pool. Neighbour lookup never scans.
- Slotted property pages: several variable-length payloads packed per page, 1KB
  inline threshold, overflow chains above it, and a compact varint codec
  (`PropCodec`) replacing bincode-framed maps.
- Property pointers packed as 24-bit page + 8-bit slot; slot reclamation with
  in-page compaction and a whole-page freelist.
- Freelist slot reuse for both node and edge records.
- Crash recovery by WAL replay, with CRC verification per frame and safe
  truncation of a torn trailing frame.

### Added — ACID and transactions

- Explicit transactions with single-fsync group commit.
- STEAL spilling: when the pool fills mid-transaction, uncommitted pages spill to
  the WAL tracked by a page-location index, so one transaction can mutate far
  more pages than the pool holds.
- Exact rollback: per-page baselines restore content and index positions, leaving
  the main file byte-identical.
- `with_transaction` for single-fsync bulk ingestion.

### Added — Cypher 1.0

- `CREATE`, `MATCH` (including multi-pattern joins), `WHERE` with label
  predicates, `SET` (property and label), `DELETE` / `DETACH DELETE`,
  `ORDER BY`, `SKIP`, `LIMIT`.
- Aggregates: `count`, `sum`, `avg`, `min`, `max`, with grouping.
- Variable-length paths (`*1..3`) and undirected edges.
- Multi-label node patterns; `RETURN *` expansion from actual bindings.

### Added — analytics

- Breadth-first search, Dijkstra shortest path, directed cycle detection.
- PageRank with configurable damping, iteration cap and tolerance, redistributing
  dangling mass and normalising scores to 1.0.
- Weakly connected components (union-find, undirected semantics).
- K-hop subgraph extraction with direction and edge-type filtering.

### Added — production safety

- Exclusive single-writer open lock, taken on the data file itself via
  `std::fs::File::try_lock` (no new dependency, no sidecar file). A contended
  open returns `DatabaseLocked` instead of silently losing a writer's data. The
  lock is acquired **before** WAL replay, which itself writes the main file.
- `integrity_check()` and `verify()`, backed by a degree-conservation oracle:
  chain degree walked from on-disk pointers must equal expected degree from an
  independent scan of the edge id space. Catches chain damage that leaves counts
  self-consistent.
- `try_get_node` / `try_get_edge` preserve storage errors; `get_node` /
  `get_edge` are documented as lossy.
- Poison-recovering lock accessors replace 71 panic sites, so a poisoned lock
  cannot abort the process.

### Added — tooling and bindings

- Interactive CLI: multi-line input, ASCII tables, and
  `.schema` / `.stats` / `.checkpoint` / `.dump` / `.history` / `.help`.
- Logical dump (`dump_cypher`) that replays into a fresh database, and doubles as
  the migration path across storage format changes.
- Python SDK (PyO3) and Node.js SDK (NAPI-RS) with transaction support.

### Added — indexing

- Label inverted index and `(label, property)` index, used for start-node
  selection. Indexes self-heal after property updates, label changes and failed
  transactions, and rebuild on demand after a cold restart.

### Fixed — correctness

Found by the adversarial test suites written in this cycle:

- **Self-loop insertion dropped the outgoing chain head.** The single-edge path
  wrote the source update and then clobbered it with a stale target copy when
  `src == dst`, losing the chain. Both updates are now merged for self-loops.
- **`MATCH (a)-[:R]->(a)` overwrote the first binding of `a`** instead of
  treating the repeated variable as a join constraint. Repeated variables are
  now a constraint, not a silent reassignment.
- **Quadratic incoming-chain planning.** The batch weave looked up positions with
  a linear scan per edge, which went quadratic when many edges shared one target.
  Replaced with position indexes.
- **`SQLITE`-style silent data loss on concurrent open** — two handles each
  reported success and the second write vanished (measured: two writers, one
  surviving node, no error).
- **Silent corruption went unreported** — corrupting one 4KB page produced a
  successful open, no error, and 156 of 200 nodes silently wrong or missing.

### Fixed — performance

Discrete edge writes degraded as the graph grew. Root cause was not cache hit
rate but three structural defects:

- **Per-edge weaving** touched the source page, target page and old head page for
  every edge; with pages far exceeding the pool, the same page was evicted and
  re-read repeatedly per batch, and each miss spilled a full 4KB page to the WAL
  ("false spill"). Replaced by two-phase batch weaving: distinct touched nodes
  are read once in page order, chain pointers are derived in memory, edge records
  are written in id order, and node head pointers are written once per node.
- **`LRUReplacer` was O(pool size) per operation** — a `VecDeque` with linear
  scans in `pin`, `unpin` and victim selection. This was the real throughput
  ceiling: at 16,384 frames (misses ≈ 0) throughput was 47k ops/s, an order of
  magnitude below the 1024-frame case. Replaced with an intrusive doubly-linked
  list, making every operation O(1).
- **Page directory pages were evictable**, so each node/edge address resolution
  could be forced to re-walk the whole directory chain. Page 0 and all directory
  pages are now substitution-exempt.

### Changed — behavioural

- **Storage format `DB_PAGE_VERSION` 1 → 2** (slotted property pages). Files
  written by the earlier one-page-per-entity layout are **not readable**; `open`
  returns an explicit error directing you to `.dump` and re-import. It never
  silently reinterprets an old file.
- `DELETE` on a node that still has relationships is now a hard error advising
  `DETACH DELETE`, rather than an implicit cascade.
- Edge weight validation moved from buffering time to commit-apply time, so an
  invalid edge fails the whole transaction atomically.
- Batched edge writes now order chains per source node rather than in strict
  client order. Outgoing chain order is preserved; **incoming** chain order for a
  target with edges from several sources may differ from per-edge insertion.
  Membership, edge contents and all analytics results are identical.

### Removed

- The pre-`DiskGraph` in-memory `Graph` type (271 lines, zero callers). It
  contradicted the pure-disk invariant.
- `CHALLENGE.md` and `MASTER_ROADMAP.md`, which described the pre-development
  problem statement and were superseded by the code, README and ROADMAP.

---

## Notes on performance figures

Earlier drafts advertised 708,561 ops/s (10M nodes), 1,019,526 ops/s
(`:memory:`) and a 1,439,681 ops/s edge burst. **Those configurations do not
reproduce those numbers** on the hardware used here; the measured values are
roughly 3–4× lower. The `:memory:` node figure _is_ reproducible once the
property payload is removed (~1.43M ops/s), which is likely how it was obtained.

Every figure now ships with its scenario and conditions — see the README
benchmark section and `ROADMAP.md`. A number without its conditions is not
accepted.

[Unreleased]: https://github.com/ysankpia/graphlite/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/ysankpia/graphlite/releases/tag/v1.0.0
[1.0.0-rc.1]: https://github.com/ysankpia/graphlite/releases/tag/v1.0.0-rc.1
