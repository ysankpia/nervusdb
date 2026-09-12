# GraphLite-RS Agent Development Guidelines

This document defines the architectural invariants, mental models, code contracts, and engineering workflows for AI agents and human contributors working on GraphLite-RS.

---

## 1. Non-Negotiable Invariants

Any modification that violates these rules must be rejected immediately:

1. **Pure Disk-Backed Architecture (Zero In-Memory Graph)**
   - The entire graph topology and dynamic properties live strictly on disk.
   - `DiskGraph` is the **single source of truth**. Never introduce in-memory graphs, tables, or collections (e.g., `HashMap<u64, Node>`) as primary state in `GraphInner` or query executors.
   - Bounded memory footprint: Resident memory must always remain strictly bounded by the configured `BufferPoolManager` pool size (e.g., 256 frames = 1MB, 1024 frames = 4MB), regardless of dataset size.

2. **Single-File Storage & Page-Level WAL**
   - The database consists strictly of two files:
     - `{path}`: Single binary data file partitioned into 4096-byte (4KB) physical pages.
     - `{path}.wal`: Page-level Write-Ahead Log.
   - Never create auxiliary or split database files (e.g., `.paged`, `.snapshot`, `.tmp`, etc.).
   - WAL frames must record standard physical page mutations: `WalRecord::PageWrite { tx_id, page_id, crc32, data: 4KB }`.
   - Transactions commit only to WAL. Checkpoints flush dirty buffer frames to `{path}` and truncate the WAL. Crash recovery replays valid committed pages from WAL directly into `{path}`.
   - **STEAL spilling**: the WAL additionally serves as the transaction spill area. When the buffer pool exhausts its frames mid-transaction, uncommitted dirty pages are appended to the WAL as redo frames and tracked in an in-memory page-location index (`BufferPoolManager::wal_pages`), so a single transaction may mutate far more pages than the pool holds. Each page location is 24 bytes, the same order as SQLite's wal-index; no graph topology is ever cached in memory.
   - **Rollback safety**: every uncommitted page records a baseline location (`tx_baseline`) at first touch. Rollback restores that location and reloads the page image, so uncommitted redo frames stay in the WAL and can never reach `{path}`. Recovery ignores redo frames without a matching `TxCommit`.
   - `MIN_SPILL_FRAMES = 16` defines the external-sort working-set floor: pools below this size keep strict NO-STEAL enforcement (all-uncommitted-dirty exhaustion is a hard error), matching SQLite's minimum page-cache policy.

3. **Fixed-Size Records & Disk-Native Index-Free Adjacency**
   - `NodeRecord` is strictly 32 bytes (128 records per 4KB page).
   - `EdgeRecord` is strictly 64 bytes (64 records per 4KB page).
   - Direct $O(1)$ physical addressing formula:
     $$\text{PageId} = \text{BasePage} + \frac{N \times \text{sizeof(Record)}}{4096}$$
     $$\text{Offset} = (N \times \text{sizeof(Record)}) \pmod{4096}$$
   - Adjacency traversal follows double-cyclic edge pointer chains across disk pages via `BufferPoolManager`. Never perform full table scans to find neighbors.

4. **Slotted Property Pages & Packed Property Pointers**
   - Variable-length property payloads must be packed into shared **Slotted Property Pages** (`SlottedPropPage`, magic `GLSP`): `Header(24B) | Slot Array (grows down) | free | Payload (grows up)`. Multiple records share one 4KB page; never allocate a whole page per entity.
   - `NodeRecord.prop_page_id` / `EdgeRecord.prop_page_id` are **packed property pointers**: high 24 bits = `PageId`, low 8 bits = `SlotId`. `0` means "no properties"; slot `0xFF` (`SLOT_OVERFLOW`) means "record lives in a `PropertyPage` overflow chain".
   - Records strictly larger than `INLINE_RECORD_MAX` (1KB) go to the overflow chain; records at or below it must be inlined into a slot.
   - Property payloads use the compact `PropCodec`/`PropReader` encoding (varint framing, ZigZag integers). Do **not** reintroduce `bincode(HashMap)` framing (~28 bytes/entity of pure overhead), and never intern property keys into `StringDict` (it would grow the header dictionary and risk overflow-page churn).
   - Deleting a record marks the slot dead; space is reclaimed by in-page `compact` when the page fills, and a fully drained page is chained into `first_free_prop_page`. Slot lookup hints live in a bounded `prop_page_hint` ring (`PROP_PAGE_HINT_CAPACITY`), which holds page numbers only, never graph topology.

5. **Explicit Batched Transactions & Memory Budgets**
   - Autocommitted single writes each cost one WAL append plus one fsync. Bulk ingestion must use `GraphLite::with_transaction` (Rust), `db.begin_transaction()` (Python/Node), and commit once so the whole batch shares **exactly one fsync**.
   - `BufferStats::wal_fsync_count` / `wal_frames_written` are the observable contract for this: a test asserting bulk-commit semantics asserts the fsync delta is exactly 1.
   - Standard buffer pool constants are exposed in public API:
     - `SMALL_POOL_FRAMES` (256 frames = 1MB)
     - `DEFAULT_BUFFER_POOL_FRAMES` (1024 frames = 4MB)
     - `MEDIUM_POOL_FRAMES` (4096 frames = 16MB)
     - `LARGE_POOL_FRAMES` (16384 frames = 64MB)
   - Convenience constructor: `GraphLite::open_with_pool_mb(path, mb)` sets frames to `(mb * 256).max(2)`.
   - Test switch `Transaction::commit_unclustered()`: Forces per-edge sequential insertion to benchmark and assert mathematical/algorithmic equivalence against `commit()` two-phase batch weaving.
   - **The action queue is bounded and this is load-bearing.** A transaction queues every
     action in memory until commit — measured at **502 bytes per node action and 128 bytes
     per edge action** — so an unbounded queue breaks the bounded-memory rule in §1 no
     matter how small the buffer pool is. `DEFAULT_MAX_TRANSACTION_ACTIONS` (4,000,000)
     caps it, configurable through `GraphLiteOptions::max_transaction_actions` (`0` =
     unlimited). **Every** enqueue goes through `Transaction::push_op`; do not add a
     second enqueue path that pushes to `ops` directly, because that path silently
     escapes the cap and the caller has no way to know which paths are covered.
     Overflow is a hard error, never an automatic flush: flushing mid-transaction would
     mean committing part of it, which destroys the rollback guarantee that is the
     reason to use a transaction.

6. **Two-Phase Batch Edge Weaving**
   - Edge batches with `EDGE_BATCH_WEAVE_MIN` or more consecutive `AddEdge` actions in one commit must go through `DiskGraph::insert_edges_batch`, never per-edge head insertion. Per-edge weaving touches the source node page, the target node page and the old head page for every edge; on a graph whose pages far exceed the pool the same page is evicted and re-read many times per batch, and each miss spills a full 4KB page to the WAL ("false spill").
   - `insert_edges_batch` must keep this shape: (0) read the distinct touched nodes **sorted by logical page** so each node page is paged in once; (1) derive `src_prev`/`src_next`/`dst_next` in memory from per-node sequences, using position indexes so a heavily shared target cannot degrade to O(n²); (2) write edge records low-id-first then high-id-first, never interleaved (head fix-ups and new edges can be thousands of pages apart and would thrash each other); (3) write node head pointers once per node, sorted by logical page.
   - Chain semantics must stay identical to per-edge insertion: chains are "reverse insertion order", the first in-batch edge's `src_next` points at the pre-batch head, and the pre-batch head's `src_prev` is rewritten. `tests/edge_locality_tests.rs` pins this equivalence.
   - Admitted consequence: reordering within a batch changes **incoming** chain order for a shared target (outgoing order is preserved because ordering is per source node). Batches and per-edge insertion therefore differ only in `Node.incoming` ordering; membership, edge contents and all analytics results are identical.

7. **O(1) Buffer Pool Replacer & Protected Pages**
   - `LRUReplacer` must stay O(1) per operation (intrusive doubly-linked list indexed by frame id). A `VecDeque` with linear scans is O(pool size) per page operation and becomes the throughput ceiling at 16K frames. `victim_filter` must scan without mutating list structure and then unlink once; implementations that reshuffle during the scan drift out of sync with `len` and raise false NO-STEAL errors.
   - Page 0 (Header) and every multi-level page directory page must be registered via `BufferPoolManager::protect_page` and excluded from **both** eviction rounds in `acquire_frame`. Directory pages are traversed on every node/edge address resolution, so letting data pages evict them forces the whole chain to be re-read.
   - `restore_meta` must call `sync_protected_pages()` so a rolled-back transaction cannot leave stale or missing directory protection.

8. **Storage Format Versioning & Zero Dependencies**
   - `DB_PAGE_VERSION` is `4`, and **this is the frozen format**. `FORMAT.md` is the
     authoritative byte-level specification; any change to the bytes on disk must
     update it in the same commit.
   - Version history: v2 added slotted property pages; v3 added full page-CRC coverage
     (`CrcDirPage` chain plus the inline CRC array in Page 0); v4 replaced the
     `bincode`-encoded WAL frames and Page 0 metadata with this repository's own
     encoders, and turned the 24-bit property-pointer overflow from a silent
     truncation into a hard error. Versions 1–3 are **not readable**: `open` returns an
     explicit error directing the user to export with the matching older build via
     `.dump` and re-import. Never silently reinterpret an old file.
   - **The version and size gates run before any write, including WAL replay.** Both
     live at the top of `open_with_options`, ahead of `StorageEngine::open`. Replay
     writes the main data file, so a check placed after it would already have
     reinterpreted an old file under current-version semantics. A rejected file must
     come out byte-identical to how it went in — `test_version_guard_rejects_v1_v2`
     asserts exactly that.
   - **The core library has zero runtime dependencies, and this is load-bearing.**
     `src/codec.rs`, `src/json.rs`, and `src/crc32.rs` define the format's bytes.
     The reason is concrete: `bincode` used to encode both the WAL frames and the
     Page 0 metadata, and it ceased maintenance in December 2025 — its final release
     contains only a compiler error and a notice. Had the format been frozen first,
     the database's lifetime would have been tied to an abandoned crate with no
     security updates. `tests/zero_dependency_tests.rs` enforces the invariant, and
     adding a dependency must be treated as a format change, not a convenience.
   - Page CRC32 coverage: pages `1..255` are covered by the inline array in Page 0
     (`INLINE_CRC_OFFSET`, 4 bytes per page); pages `>= 256` by the two-level
     `CrcDirPage` radix directory. A stored checksum of `0` means "not recorded" and the
     page is **skipped**, never reported as corrupt ("rather miss than falsely alarm").
     `CrcStore` deliberately bypasses `BufferPoolManager` to avoid the
     `fetch_page -> evict -> record CRC -> fetch_page` recursion, and is the **last
     writer of Page 0** — it owns both the inline CRC array and `crc_dir_root`, so
     `sync_header()` must run _before_ `flush_crc()` in the checkpoint sequence.
   - Directory pages carry a `self_crc` (sealed on every write, verified on every load).
     A silently corrupted L2 page would otherwise misreport every data page it covers as
     a mismatch — thousands of false positives blaming innocent pages.
     **Every** write path must seal first: both `flush()` and cache eviction
     (`evict_if_needed`). An unsealed evict leaves new contents with an old checksum.
   - `CrcStore::flush()` must re-pin Page 0 whenever a directory exists, not only when
     something changed this round. `sync_header()` writes other Page 0 fields, so
     "nothing changed" does not imply "Page 0 on disk is current"; gating on change
     leaves `crc_dir_root` at `0` and makes the whole chain unreadable.
   - **WAL replay must refresh checksums.** `StorageEngine::open` replays before
     `DiskManager::open` (replay extends the file, and the page high-water mark comes
     from its length), so no `CrcStore` exists yet and the pages it writes would keep
     their _previous_ checksums. `open_with_options` therefore calls
     `storage::for_each_committed_page` right after attaching the CRC store and
     re-records those pages. Skipping this does not lose data, but it makes every
     replayed page unreadable — and because `get_node` is lossy, it presents as
     **committed data vanishing after a crash**.
   - These three defects are only reachable at scale: the first two need more directory
     pages than `CRC_CACHE_CAPACITY` (64) holds, i.e. ≈65k data pages, and the third
     needs a real `SIGKILL` (a clean `drop` flushes pages and checksums together and
     hides it). Fixtures that stay small will pass while the engine is broken —
     `test_crc_directory_survives_churn_beyond_cache` and
     `test_wal_replay_refreshes_page_checksums` exist for exactly this reason.

9. **Freelist Slot Reclamation**
   - Page 0 (Header Page) stores `first_free_node_id` and `first_free_edge_id`.
   - Deleted records must be chained into the respective Freelist.
   - New allocations must prioritize popping from the Freelist before advancing `next_id`, completely eliminating disk space fragmentation.

10. **Lock De-escalation & Concurrency Safety**

- Avoid holding global write locks during long traversals. Read queries and graph algorithms (`dijkstra`, `bfs`, `has_cycle`, `query()`) must clone the lightweight `DiskGraph` handle, release the global read lock immediately, and execute concurrently under page-level latches.
- All pointer traversal loops (`while curr != 0`) must enforce cycle-detection guards (`seen: HashSet<u64>`) to guarantee zero infinite loops under concurrent pointer updates.

11. **Exclusive Single-Writer Open (No Silent Multi-Writer Corruption)**

- A database file may have **exactly one open handle** at a time, across processes and within one process. `GraphLite::open` must take an exclusive lock on `{path}` itself (`std::fs::File::try_lock`, stable since Rust 1.89 — never add a third-party dependency or a sidecar `.lock` file, which would break the two-file invariant). A contended open returns `GraphError::DatabaseLocked`.
- The lock MUST be acquired **before** `StorageEngine::open`, because WAL replay writes the main data file; locking afterwards already permits a racing replay.
- `:memory:` mode takes no lock.
- Rationale: without this, two writers each report success and the second write is silently lost.

12. **Integrity Checking & No Silent Read Errors**

- `GraphLite::integrity_check()` must remain read-only and must never auto-repair: repair strategies require separate design and explicit authorization.
- Validation must include a **degree-conservation oracle**: chain degree measured by walking on-disk chain pointers must equal expected degree measured by independently scanning the edge id space. Do not re-derive both sides from the same traversal — that is self-confirmation, not a check. Keep the oracle's result wired into the report; a computed-but-discarded accumulator is a defect.
- `get_node` / `get_edge` are **lossy** (they fold storage errors into `None`) and must stay documented as such. Production code uses `try_get_node` / `try_get_edge`, which return `Result` and reserve `Ok(None)` for genuine absence.
- Any new public read accessor must decide explicitly between lossy and error-preserving semantics.

13. **Lock Poisoning Must Not Abort the Process**

- Never write `.lock().unwrap()` or `.read().expect("Lock poisoned")`. Use the poison-recovering accessors from `src/sync_ext.rs` (`lock_recover` / `read_recover` / `write_recover`).
- Rationale: these locks guard rebuildable derived state (frame tables, allocator metadata, handle wrappers), not business invariants, so recovering beats converting a local failure into an unrecoverable process abort — especially for embedded callers.
- `unwrap`/`expect` remain forbidden in library code per §4.1; fixed-offset slice conversions in `page.rs` must carry a comment stating why they cannot fail by construction.

14. **Statement-Level Atomicity**

- **Any Cypher write entry point must undo a partially applied statement before returning its error.** Use `GraphLite::rollback_failed_statement`; do not reimplement the sequence, because a second implementation will drift from the first.
- Rationale, measured: with no rollback, `UNWIND [2, 1, 3] AS v CREATE (n:Num {v: v})` failed on the duplicate `1` and left `{v: 2}` in the buffer pool. A later, unrelated `CREATE (:Other {x: 99})` committed it, so after reopening, `MATCH (n:Num)` returned the row the constraint had rejected. Partial writes are invisible until something else commits them, which is why this survived as long as statements only wrote a handful of records.
- The sequence is: drain the modified-page set, restore each page from its transaction baseline (`BufferPoolManager::rollback_uncommitted_pages`), restore the allocator metadata snapshot, and `IndexManager::invalidate_all`. It mirrors `Transaction::commit`'s failure path on purpose — one semantics for both.
- This is not a substitute for explicit transactions. A single statement is the unit of atomicity here; `GraphLite::with_transaction` remains the way to group several statements.

---

## 2. Codebase Map & Module Responsibilities

```text
graphlite-rs/
├── Cargo.toml                  # Workspace root manifest (core, python, nodejs)
├── README.md                   # User documentation & architecture guide
├── AGENTS.md                   # This developer & agent standard specification
├── src/
│   ├── lib.rs                  # Public facade (GraphLite, Transaction), ACID coordinator
│   ├── main.rs                 # Standalone demo binary
│   ├── page.rs                 # 4KB page layout, NodeRecord (32B), EdgeRecord (64B), SlottedPropPage, PropCodec
│   ├── buffer.rs               # DiskManager (paged I/O) and LRU BufferPoolManager (page latches, STEAL spill)
│   ├── disk_graph.rs           # O(1) direct addressing, disk adjacency, Freelist, page iterators
│   ├── storage.rs              # Page-level WAL engine (WalWriter), CRC32 verification, checkpointing, recovery
│   ├── index.rs                # Secondary indexing (Label inverted index & Property BTreeMap index)
│   ├── integrity.rs            # Read-only structural integrity check (degree-conservation oracle)
│   ├── lock.rs                 # Process-level exclusive open lock on the main data file
│   ├── sync_ext.rs             # Poison-recovering lock accessors (never panic on a poisoned lock)
│   ├── query.rs                # Chainable strongly-typed QueryBuilder & GraphQuery DSL
│   ├── algo.rs                 # Pure-disk graph algorithms (BFS, Dijkstra, Cycle, PageRank, WCC, K-Hop)
│   ├── cypher/
│   │   ├── ast.rs              # AST statements, expressions, and pattern nodes
│   │   ├── lexer.rs            # Cypher tokenizer (comments, aggregate keywords)
│   │   ├── parser.rs           # Recursive-descent Cypher parser (UNWIND/SET/ORDER BY/SKIP/aggregates)
│   │   ├── executor.rs         # Execution engine leveraging disk cursors and secondary indexes
│   │   └── mod.rs              # Cypher module exports
│   └── graph.rs                # Domain models (Node, Edge, Value, Direction, GraphError)
├── bindings/
│   ├── python/                 # Official Python SDK (PyO3 0.22 + Maturin, abi3)
│   └── nodejs/                 # Official Node.js / TypeScript SDK (NAPI-RS 2.16)
└── tests/
    ├── integration_tests.rs    # Core CRUD/ACID/concurrency/index/stress regression suites
    ├── cypher_advanced_tests.rs# Cypher 1.0 syntax closure (SET/DETACH DELETE/ORDER BY/aggregates/paging)
    ├── unwind_tests.rs         # UNWIND expansion, batch ingestion, statement atomicity
    ├── merge_tests.rs          # MERGE idempotence, ON CREATE / ON MATCH, whole-pattern semantics
    ├── concurrency_isolation_tests.rs # Snapshot consistency, atomic visibility, no lost writes
    ├── concurrency_stress_tests.rs # Core-count-adaptive read/write stress
    ├── analytics_tests.rs      # PageRank / WCC / K-Hop subgraph analytics
    ├── steal_spill_tests.rs    # STEAL spilling, rollback zero-pollution, checkpoint semantics
    ├── slotted_property_tests.rs # Slotted page packing, slot reuse, compaction, density target
    ├── batch_tx_tests.rs       # Batched transactions, single-fsync contract, bulk throughput
    ├── edge_locality_tests.rs  # Batch-weave equivalence, self-loops, false-spill elimination
    ├── production_safety_tests.rs # Exclusive lock, integrity check, no silent errors, poison recovery
    ├── robustness_tests.rs     # File lock, auto-checkpoint, page CRC at scale, WAL replay CRC, chunking
    └── equivalence_tests.rs    # v1.0.0 behaviour guardrails (query, transaction, API, format)
```

---

## 3. Engineering Workflows & Verification Commands

All agents modifying this codebase must execute the relevant checks before concluding any task.

### 3.1 The Full CI Gate (run all of these)

CI runs exactly this set, on Linux and macOS. Anything less is incomplete verification:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Two of these are easy to forget and have already caused a red CI:

- **`rustdoc -D warnings`** rejects unescaped angle brackets in doc comments
  (`<expr>`, `<NodeId>` are parsed as HTML tags).
- **clippy's lint set moves with the compiler.** A toolchain update can introduce
  a new lint that turns a previously green tree red. When that happens, fix the
  code; **never pin an older compiler to silence it**, because the lint is
  usually correct.

### 3.2 Release-Mode Throughput Suites

These carry throughput assertions that only mean anything when optimised:

```bash
cargo test --release --test batch_tx_tests
cargo test --release --test edge_locality_tests
```

**Throughput assertions must not depend on machine speed.** The suite previously
asserted "batch is >20x faster than autocommit", which passed locally and failed
on CI at ~16x because a cloud disk's fsync characteristics differ from a local
SSD. Assert the _mechanism_ instead (N autocommitted writes cost N fsyncs; one
batched transaction of N writes costs exactly 1), which holds on any hardware,
and keep any speed ratio as a loose lower bound.

### 3.3 Validate Fixes by Reverting Them

**A regression test that passes with the fix removed proves nothing.** For each
of the three defects found during the v3 page-CRC work, the fix was reverted in
isolation and the new test re-run to confirm it fails. Two earlier attempts at a
reproducer passed _with the fix removed_ and were discarded rather than kept.

Two traps made those first attempts useless, and both recur:

- **Too small a fixture.** The CRC-directory defects need more directory pages
  than `CRC_CACHE_CAPACITY` (64) holds, i.e. roughly 65,000 data pages. 90k- and
  800k-node fixtures passed while the engine was broken; only 300,000 distinct
  page numbers reach the eviction path.
- **Too clean a shutdown.** The replay defect needs a real `SIGKILL`. A normal
  `drop` flushes the buffer pool and writes pages and checksums together, hiding
  it entirely.

### 3.4 Target-Specific Verification

- **Run Out-of-Core Stress Test Only**:
  ```bash
  cargo test test_10_pure_out_of_core_stress -- --nocapture
  ```
- **Run Concurrency Stress Test Only**:
  ```bash
  cargo test test_06_concurrent_read_write_stress_test -- --nocapture
  ```
- **Run STEAL Spill & Rollback Suite Only**:
  ```bash
  cargo test --test steal_spill_tests
  ```
- **Run Slotted Property & Density Suite Only**:
  ```bash
  cargo test --test slotted_property_tests
  ```
- **Run Batched Transaction Suite Only** (use `--release` for the 20,000+ ops/s target):
  ```bash
  cargo test --release --test batch_tx_tests -- --nocapture
  ```
- **Run Edge Locality Suite Only**:
  ```bash
  cargo test --test edge_locality_tests
  ```
- **Run Production Safety Suite Only** (includes a real child-process lock probe):
  ```bash
  cargo test --test production_safety_tests
  ```
- **Run Robustness Suite Only** (page CRC at scale, WAL replay checksums, chunking):
  ```bash
  cargo test --test robustness_tests
  ```
- **Run the benchmarks**:
  ```bash
  cargo bench --bench throughput      # full; GL_SCALE=small for a smoke run
  cargo bench --bench pool_probe
  cargo bench --bench mem_probe
  ```
- **Run a Real Dataset (do this for page, checksum, or replay changes)**:

  ```bash
  DATASET_PATH=/data/com-dblp.ungraph.txt DB_DIR=/data/bench POOL_MB=256 \
    cargo bench --bench snap_dblp_bench
  DATASET_PATH=/data/soc-LiveJournal1.txt DB_DIR=/data/bench POOL_MB=1024 \
    cargo bench --bench snap_livejournal_bench
  ```

  The SNAP benchmarks are the only end-to-end check at realistic scale. They take
  `DATASET_PATH`/`DATASET_DIR`, `DB_DIR`, `POOL_MB`, `MAX_EDGES` and
  `AUTO_CHECKPOINT_MB`, and print the configuration they ran with.

  `AUTO_CHECKPOINT_MB` defaults to `0` (off) here: the benchmarks checkpoint
  explicitly, and the engine's 64 MB default roughly halves bulk ingest
  throughput (LiveJournal: 447k ops/s off vs 232k on). Leave it off when
  measuring write throughput; turn it on only when testing that path itself.

  Red lines from the last accepted run — a change here is a correctness
  regression, not noise:
  - LiveJournal edge ingestion **≥150,000 ops/s** (measured 200,618 on rc.2, 201,823 on rc.3 — see `docs/benchmarks.md`)
  - LiveJournal hub 1-hop / 2-hop **exactly** 335,194 / 10,027,730
  - com-DBLP hub 1-hop / 2-hop **exactly** 10,080 / 161,877

  Hub selection is an explicit total order (degree descending, then raw id
  ascending). It must stay that way: com-DBLP has three nodes tied at degree 164
  _exactly at rank 50_, so a partial order makes the 2-hop total depend on sort
  internals and produced three different "correct" numbers across runs.

- **Verify Node.js SDK**:
  ```bash
  cargo build -p graphlite-node
  cp target/debug/libgraphlite_node.dylib bindings/nodejs/graphlite.node
  cd bindings/nodejs && node test.mjs
  ```
- **Verify Python SDK**:
  ```bash
  cargo build -p graphlite-python
  cp target/debug/libgraphlite_python.dylib bindings/python/graphlite.so
  cd bindings/python && PYTHONPATH=. python3 tests/test_graphlite.py
  ```

---

## 4. Coding & Implementation Contracts

### 4.1 Error Handling

- Never use `.unwrap()` or `.expect()` in library code (`src/` except tests).
- All failures must map to `GraphError`:
  - Storage I/O errors $\to$ `GraphError::IoError(e)`
  - Missing entities $\to$ `GraphError::NodeNotFound(id)` / `GraphError::EdgeNotFound(id)`
  - Corruption/Format errors $\to$ `GraphError::StorageError(msg)`
  - Syntax/Runtime query errors $\to$ `GraphError::General(msg)`

### 4.2 Property Storage Pattern

- Primitive integers (`age`, `score`, `rank`) may be cached inline in `NodeRecord.inline_prop_val`.
- Multi-label collections (`HashSet<String>`) and full dynamic attribute dictionaries (`HashMap<String, Value>`) must be packaged into `NodeData` and saved in `PropertyPage` overflow chains.
- Edge properties must be saved via `write_edge_properties` and linked through `EdgeRecord.prop_page_id`.

### 4.3 Query & Scan Optimizations

- **Label & Property Index First**: When evaluating `MATCH (n:Label {key: val})`, `CypherExecutor` must query `IndexManager` first. Never perform disk scans if an indexed entry point exists.
- **Fast Path Node Iteration**: When no deletion holes exist (`first_free_node_id == 0 && node_count == next_node_id - 1`), `DiskGraph::all_node_ids()` returns `(1..next_node_id).collect()` in $O(1)$ without reading disk pages.
- **Node Caching in Path Match**: Cache repeated hub node resolutions in a local query hashmap to avoid duplicate overflow page deserializations.

### 4.4 Cypher 1.0 Surface (Supported Grammar)

```text
CREATE pat
MERGE pat [ON CREATE SET item [, item ...]] [ON MATCH SET item [, item ...]]
          [RETURN item [, item ...]]
          [ORDER BY expr [ASC|DESC], ...] [SKIP n] [LIMIT n]
UNWIND expr AS var [CREATE pat] [RETURN item [, item ...]]
                        [ORDER BY expr [ASC|DESC], ...] [SKIP n] [LIMIT n]
MATCH pat [, pat ...] [WHERE expr]
      [SET item [, item ...]]
      [DELETE var... | DETACH DELETE var...]
      [CREATE pat]
      [RETURN item [, item ...]]
      [ORDER BY expr [ASC|DESC], ...] [SKIP n] [LIMIT n]

item   := * | var | var.key [AS alias] | FUNC(*) | FUNC(var[.key]) [AS alias]
FUNC   := count | sum | avg | min | max
SET    := var.key = <literal|var.key|var> | var:Label
pat    := (var:Label1:Label2 {k: expr}) -[r:TYPE*min..max {k: expr}]-> (var)
expr   := literal | var | var.key | [expr, ...] | FUNC(...)
```

`UNWIND` expands a list into rows, one per element, and binds each element to
`var`; a non-list value yields a single row and an empty list yields none. It is
the only way to express bulk data in one statement
(`UNWIND [1,2,3] AS x CREATE (n:Num {v: x})`), and it is why **pattern property
values are expressions rather than literals** — `{v: x}` must read the `UNWIND`
variable. Two rules follow from that and both are enforced, not advisory:

- `MATCH` pattern properties must be literals (checked at parse time). A pattern is
  matched before any variable is bound, so an expression there cannot be evaluated;
  rejecting it is required, because silently matching nothing looks like an empty
  graph.
- `SET` / `DELETE` on a scalar binding is an error, not a silent no-op.
- `is_mutating()` returns true for `UNWIND` only when it carries a `CREATE`; the
  read-only handle accepts `UNWIND ... RETURN` and rejects `UNWIND ... CREATE`.

`MERGE` is the idempotent write: match the pattern, reuse it, create it only when
nothing matches. `is_mutating()` returns true for it unconditionally, even when a
given run writes nothing, because that is only known after matching. Two rules are
load-bearing and both are pinned by tests:

- Matching uses MATCH **filter** semantics: named properties must be equal, but
  extra properties on an existing node do not break the match.
- The pattern is matched or created **as a whole**. If any part is missing, the
  entire pattern is created, including parts that already exist elsewhere in the
  graph. Partial reuse would make the outcome depend on which parts happened to be
  present first, which is not predictable from the query.

`GraphLite::read_snapshot()` returns a `ReadSnapshot` that holds the shared read
lock for its lifetime, giving a multi-step traversal one consistent view. Use it for
anything that reads an entity and then follows what it references: without it,
`get_node` and `get_edge` each take and release the lock separately, so a concurrent
delete between them makes the traversal observe a reference to an edge that is no
longer readable by the time it is fetched. That is not an internal tear — the caller
simply assembled two different states. Snapshots block writers while they live, so
keep them short, and never call a `GraphLite` write entry point while holding one
(it would wait on a lock the snapshot itself holds).

A write statement is **atomic at statement granularity**: `GraphLite::execute` and
`run_cypher` snapshot allocator metadata, open a transaction context, and undo a
partially applied statement through `rollback_failed_statement` before returning the
error. Without it, a statement that failed on record 500 of 1000 kept the first 499
in the buffer pool, and the next successful commit wrote them to disk. Do not add a
write entry point that skips this.

Execution pipeline: `find_matches` (per-pattern resolution + shared-variable join) → `WHERE` →
`SET` → `CREATE` → `DELETE` → projection (grouped aggregation) → `ORDER BY` → `SKIP` → `LIMIT`.

- **Variable binding discipline**: contexts bind `Binding::Node(u64) | Binding::Edge(u64)`. Never key a
  context by a raw `u64` alone; node IDs and edge IDs share a numbering space and must stay distinguishable.
- **Aggregation semantics**: `count(*)` counts rows, `count(x)` counts non-null bindings; `sum` returns
  `Int` when every input is integral; `avg` always returns `Float`; `min`/`max` preserve input type.
  An empty group yields `count = 0`, `sum = 0`, and null for `avg`/`min`/`max`.
- **Delete semantics**: `DELETE` on a node that still has relationships is a hard error advising
  `DETACH DELETE`; `DETACH DELETE` cascades to all incident edges and chains the freed node/edge slots
  into the Freelist.
- **In-place node rewrites** (e.g. `SET n:Label`) must use `DiskGraph::update_node_payload`, never
  `insert_node_with_id_exact`, to avoid inflating `node_count`.
- **Mutation visibility**: any statement that mutates (`SET` / `DELETE` / `CREATE` sub-clauses) must take
  the exclusive write lock and commit through the page-level WAL; `CypherStatement::is_mutating()` is the
  single source of truth for that routing decision.

### 4.5 Graph Analytics Contracts

- `pagerank(graph, damping, max_iterations, tolerance)` must redistribute dangling-node mass uniformly and
  return scores sorted descending; the score vector must sum to 1.0.
- `weakly_connected_components(graph)` treats every edge as undirected (union-find) and returns components
  sorted by size descending.
- `k_hop_subgraph(graph, start, k, direction, edge_type)` must deduplicate visited nodes, keep only edges
  whose both endpoints are inside the subgraph, and reject an unknown start node with `NodeNotFound`.
- All analytics traverse via `DiskGraph` adjacency cursors through the buffer pool; never materialize a full
  in-memory adjacency map as persistent state.

---

## 5. Cleanliness, Development Workflow & Commit Standards

### 5.1 Development workflow: PR-based, `main` is protected

`main` has branch protection enabled: force pushes and deletions are refused, and
all three CI checks (`Test (ubuntu-latest)`, `Test (macos-latest)`, `Docs build`)
must pass. Changes therefore go through a pull request, including the
maintainer's own work:

```bash
git switch -c fix/short-description
# ... make the change, run the full CI gate from §3.1 ...
git push -u origin fix/short-description
gh pr create --fill
# merge once CI is green
gh pr merge --squash --delete-branch
```

Branch naming: `feat/…`, `fix/…`, `perf/…`, `refactor/…`, `test/…`, `docs/…`,
`chore/…`, matching the commit type it will produce.

The maintainer may bypass protection in an emergency, but a bypassed change must
be followed up by a verified green run. Do not make bypassing the normal path.

### 5.2 External contributions are refused by policy

This repository does not accept code from outside contributors; pull requests
raised by anyone other than the owner, a member or a collaborator are closed
automatically by `.github/workflows/close-external-prs.yml`. The reason is
licensing (dual AGPL + commercial), not code quality — see CONTRIBUTING.md.

Issues are welcome and are the correct channel for outside reports. Work that
comes from an issue is implemented by the maintainer on a branch, then merged via
PR.

**Never** relax the automation condition in that workflow to accept an
unsolicited pull request, and never merge one, without first resolving the
licensing question (accepting outside code would require a CLA, which the project
deliberately does not collect).

### 5.3 Commit standards

- Message format: `type(scope): summary`, imperative and concise — e.g.
  `fix(storage): reject a second handle on the same file`.
- The body must explain the **root cause**, not only the symptom that was patched.
  If a hypothesis was tested and disproved, say so.
- State plainly what was verified and what was not. Never claim a check was run
  when it was skipped.
- If a change alters a documented performance number, update the number **and**
  its measurement conditions in the same commit.

### 5.4 File safety — never destroy untracked work

**No `rm` on a file that is not tracked by git.** This is not hypothetical: an
agent working on this repository deleted `src/crc.rs` while testing a baseline.
The file was untracked, so `git stash` had not captured it, and there was no
recovery path — it had to be rebuilt from a session log, and the reconstruction
silently lost two fixes that had to be re-derived afterwards.

Untracked files (new modules, scratch harnesses, `examples/`) are the easiest
thing to lose and the hardest to get back. Therefore:

- **Move, don't delete.** Send anything you want out of the way to
  `.trash/<YYYY-MM-DD>/` and leave it there. `.trash/` is gitignored; a human
  decides when it is truly dead.
- **Track before you touch.** If an untracked file is about to be affected by an
  operation (a baseline comparison, a format refactor, a stash), `git add` it
  first. A file in the index is recoverable; an untracked one is not.
- **Never `git stash` to isolate a baseline.** Stash without `--include-untracked`
  silently ignores exactly the files at risk. Use `git worktree add` for a clean
  baseline checkout instead.
- **`rm -rf` requires explicit human authorization**, every time, with the target
  named. "It looked like scratch" is not authorization.

### 5.5 Housekeeping

- Never commit test database artifacts (`*.db`, `*.db.wal`, `*.paged`).
- Keep `.gitignore` updated for target builds, Node binaries (`*.node`), Python
  dynamic libraries (`*.so`, `*.dylib`), benchmark scratch output (`/bench_db/`),
  tool-generated indexes (`.codegraph/`), and the `.trash/` directory above.
- No placeholder code: strictly forbidden to introduce `todo!()` or
  `unimplemented!()`.
- **Keep `CHANGELOG.md` current.** Every user-visible change adds an entry under
  `## [Unreleased]` in the same commit, grouped as Added / Changed / Fixed /
  Removed / Security. The commit type alone is not enough: the changelog is what
  tells a user whether they need to act (a storage format bump or a behavioural
  change certainly qualifies).
- Update the relevant documentation in the same change: `README.md` for
  user-facing behaviour, `AGENTS.md` for invariants or workflows, `ROADMAP.md`
  when a planned item lands or a new limitation is discovered, and **`FORMAT.md`
  in the same commit as any change to the bytes on disk**. A stale format
  specification is worse than none, because the next reader will trust it.
