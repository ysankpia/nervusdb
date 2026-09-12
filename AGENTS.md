# NervusDB Agent Development Guidelines

This document defines the architectural invariants, mental models, code contracts, and engineering workflows for AI agents and human contributors working on NervusDB.

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
   - Variable-length property payloads must be packed into shared **Slotted Property Pages** (`SlottedPropPage`, magic `NVSP`): `Header(24B) | Slot Array (grows down) | free | Payload (grows up)`. Multiple records share one 4KB page; never allocate a whole page per entity.
   - `NodeRecord.prop_page_id` / `EdgeRecord.prop_page_id` are **packed property pointers**: high 24 bits = `PageId`, low 8 bits = `SlotId`. `0` means "no properties"; slot `0xFF` (`SLOT_OVERFLOW`) means "record lives in a `PropertyPage` overflow chain".
   - Records strictly larger than `INLINE_RECORD_MAX` (1KB) go to the overflow chain; records at or below it must be inlined into a slot.
   - Property payloads use the compact `PropCodec`/`PropReader` encoding (varint framing, ZigZag integers). Do **not** reintroduce `bincode(HashMap)` framing (~28 bytes/entity of pure overhead), and never intern property keys into `StringDict` (it would grow the header dictionary and risk overflow-page churn).
   - Deleting a record marks the slot dead; space is reclaimed by in-page `compact` when the page fills, and a fully drained page is chained into `first_free_prop_page`. Slot lookup hints live in a bounded `prop_page_hint` ring (`PROP_PAGE_HINT_CAPACITY`), which holds page numbers only, never graph topology.

5. **Explicit Batched Transactions & Memory Budgets**
   - Autocommitted single writes each cost one WAL append plus one fsync. Bulk ingestion must use `NervusDb::with_transaction` (Rust), `db.begin_transaction()` (Python/Node), and commit once so the whole batch shares **exactly one fsync**.
   - `BufferStats::wal_fsync_count` / `wal_frames_written` are the observable contract for this: a test asserting bulk-commit semantics asserts the fsync delta is exactly 1.
   - Standard buffer pool constants are exposed in public API:
     - `SMALL_POOL_FRAMES` (256 frames = 1MB)
     - `DEFAULT_BUFFER_POOL_FRAMES` (1024 frames = 4MB)
     - `MEDIUM_POOL_FRAMES` (4096 frames = 16MB)
     - `LARGE_POOL_FRAMES` (16384 frames = 64MB)
   - Convenience constructor: `NervusDb::open_with_pool_mb(path, mb)` sets frames to `(mb * 256).max(2)`.
   - Test switch `Transaction::commit_unclustered()`: Forces per-edge sequential insertion to benchmark and assert mathematical/algorithmic equivalence against `commit()` two-phase batch weaving.
   - **The action queue is bounded.** `DEFAULT_MAX_TRANSACTION_ACTIONS` caps it
     (configurable via `NervusDbOptions::max_transaction_actions`; `0` = unlimited)
     because a transaction holds every action in memory until commit.
     **Every** enqueue goes through `Transaction::push_op` — a second path that pushes
     to `ops` directly escapes the cap silently. Overflow is a hard error, never an
     automatic flush: flushing mid-transaction commits part of it, which destroys the
     rollback guarantee that is the reason to use a transaction. Sizes and rationale:
     [`docs/architecture.md`](docs/architecture.md).

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
   - `DB_PAGE_VERSION` is `5` and **this is the frozen format** — the number is
     asserted against the code by
     `zero_dependency_tests::documented_format_version_matches_the_code`, because
     this line said `4` for a release after the constant moved to `5`. `FORMAT.md`
     is the authoritative byte-level spec (version history, limits, CRC layout, why
     zero dependencies). Any change to the bytes on disk updates `FORMAT.md` **in
     the same commit**. Never silently reinterpret an older file.
   - **The version and size gates run before any write, including WAL replay** — they
     sit at the top of `open_with_options`, ahead of `StorageEngine::open`, because
     replay writes the main file. A rejected file comes out byte-identical
     (`test_version_guard_rejects_v1_v2`).
   - **The core library has zero runtime dependencies, and this is load-bearing**:
     `codec.rs` / `json.rs` / `crc32.rs` produce the format's bytes. Adding a
     dependency is a format change, not a convenience. Enforced by
     `tests/zero_dependency_tests.rs`.
   - CRC ordering constraints — each one is load-bearing and each has a test:
     - `CrcStore` is the **last writer of Page 0** (it owns the inline CRC array and
       `crc_dir_root`), so checkpoints run `sync_header()` **before** `flush_crc()`.
     - `CrcStore::flush()` re-pins Page 0 **whenever a directory exists**, not only
       when something changed: `sync_header()` writes other Page 0 fields, so
       "unchanged" does not mean "on disk is current".
     - **Every** write path seals a directory page first — both `flush()` and cache
       eviction. An unsealed evict leaves new contents with a stale checksum.
     - WAL replay **refreshes checksums** (`for_each_committed_page` after attaching
       the store); replay runs before `DiskManager::open`, so replayed pages would
       otherwise keep their previous checksums and read as unreadable.
   - A stored checksum of `0` means "not recorded" → the page is **skipped**, never
     reported corrupt. Rather miss than falsely alarm.

9. **Freelist Slot Reclamation**
   - Page 0 (Header Page) stores `first_free_node_id` and `first_free_edge_id`.
   - Deleted records must be chained into the respective Freelist.
   - New allocations must prioritize popping from the Freelist before advancing `next_id`, completely eliminating disk space fragmentation.

10. **Lock De-escalation & Concurrency Safety**

- Avoid holding global write locks during long traversals. Read queries and graph algorithms (`dijkstra`, `bfs`, `has_cycle`, `query()`) must clone the lightweight `DiskGraph` handle and release the global read lock immediately, so they do not block writers for the duration of a traversal.
- **Reads are serialized by the buffer pool, and this is a known, measured limitation — do not describe it as parallel.** `BufferPoolManager` is reached through a single `Arc<Mutex<BufferPoolManager>>`, so every page touch takes one global mutex. A read of one node takes at least three acquisitions, plus one per incident edge: a hub of degree 343 costs ≈345 acquisitions. Measured on the com-DBLP database (16 threads vs 1, same total work): plain point reads ran at **0.6×–1.4% scaling efficiency**, i.e. _slower_ than single-threaded. Control runs in the same process confirmed the environment is not the cause — pure CPU spins and read-lock-only loops both scaled to ≈40% at 16 threads.
- There is **no page-level latching**, and `Frame` no longer pretends otherwise: it used to carry a `latch: Arc<RwLock<()>>` that was declared and initialized but never read or written. Deleted rather than left as unimplemented intent. Real per-frame latching is what would make readers genuinely parallel, and it is a buffer-pool redesign (ROADMAP item 1) — not a field to re-add.
- All pointer traversal loops (`while curr != 0`) must enforce cycle-detection guards (`seen: HashSet<u64>`) to guarantee zero infinite loops under concurrent pointer updates.

11. **Exclusive Single-Writer Open (No Silent Multi-Writer Corruption)**

- A database file may have **exactly one open handle** at a time, across processes and within one process. `NervusDb::open` must take an exclusive lock on `{path}` itself (`std::fs::File::try_lock`, stable since Rust 1.89 — never add a third-party dependency or a sidecar `.lock` file, which would break the two-file invariant). A contended open returns `GraphError::DatabaseLocked`.
- The lock MUST be acquired **before** `StorageEngine::open`, because WAL replay writes the main data file; locking afterwards already permits a racing replay.
- `:memory:` mode takes no lock.
- Rationale: without this, two writers each report success and the second write is silently lost.

12. **Integrity Checking & No Silent Read Errors**

- `NervusDb::integrity_check()` must remain read-only and must never auto-repair: repair strategies require separate design and explicit authorization.
- Validation must include a **degree-conservation oracle**: chain degree measured by walking on-disk chain pointers must equal expected degree measured by independently scanning the edge id space. Do not re-derive both sides from the same traversal — that is self-confirmation, not a check. Keep the oracle's result wired into the report; a computed-but-discarded accumulator is a defect.
- `get_node` / `get_edge` are **lossy** (they fold storage errors into `None`) and must stay documented as such. Production code uses `try_get_node` / `try_get_edge`, which return `Result` and reserve `Ok(None)` for genuine absence.
- Any new public read accessor must decide explicitly between lossy and error-preserving semantics.

13. **Lock Poisoning Must Not Abort the Process**

- Never write `.lock().unwrap()` or `.read().expect("Lock poisoned")`. Use the poison-recovering accessors from `src/sync_ext.rs` (`lock_recover` / `read_recover` / `write_recover`).
- Rationale: these locks guard rebuildable derived state (frame tables, allocator metadata, handle wrappers), not business invariants, so recovering beats converting a local failure into an unrecoverable process abort — especially for embedded callers.
- `unwrap`/`expect` remain forbidden in library code per §4.1; fixed-offset slice conversions in `page.rs` must carry a comment stating why they cannot fail by construction.

14. **Statement-Level Atomicity**

- **Any Cypher write entry point must undo a partially applied statement before returning its error.** Use `NervusDb::rollback_failed_statement`; do not reimplement the sequence, because a second implementation will drift from the first.
- Rationale, measured: with no rollback, `UNWIND [2, 1, 3] AS v CREATE (n:Num {v: v})` failed on the duplicate `1` and left `{v: 2}` in the buffer pool. A later, unrelated `CREATE (:Other {x: 99})` committed it, so after reopening, `MATCH (n:Num)` returned the row the constraint had rejected. Partial writes are invisible until something else commits them, which is why this survived as long as statements only wrote a handful of records.
- The sequence is: drain the modified-page set, restore each page from its transaction baseline (`BufferPoolManager::rollback_uncommitted_pages`), restore the allocator metadata snapshot, and `IndexManager::invalidate_all`. It mirrors `Transaction::commit`'s failure path on purpose — one semantics for both.
- This is not a substitute for explicit transactions. A single statement is the unit of atomicity here; `NervusDb::with_transaction` remains the way to group several statements.

---

## 2. Codebase Map & Module Responsibilities

Workspace: `src/` (core, zero deps), `bindings/python`, `bindings/nodejs`, `tests/`,
`benches/` (includes `real_data/` acceptance instruments). Layout is discoverable with
`ls`; what follows is **which module owns which concern**, which is not.

| Concern                                                                | Owner                                       |
| ---------------------------------------------------------------------- | ------------------------------------------- |
| Public facade, ACID coordination, locking entry points                 | `src/lib.rs` (`NervusDb`, `Transaction`)    |
| Page layout, records, property codec, property pointers                | `src/page.rs`                               |
| Page I/O and the buffer pool (eviction, STEAL spill, CRC mount points) | `src/buffer.rs`                             |
| Addressing, disk adjacency, freelists, batch weave                     | `src/disk_graph.rs`                         |
| WAL framing, checkpoint, recovery                                      | `src/storage.rs`                            |
| Secondary indexes (label inverted, property BTreeMap) and constraints  | `src/index.rs`                              |
| Structural verification (degree-conservation oracle)                   | `src/integrity.rs`                          |
| Cypher: tokenize → parse → execute                                     | `src/cypher/{lexer,parser,executor,ast}.rs` |
| Graph algorithms over disk cursors                                     | `src/algo.rs`                               |
| Domain models and `GraphError`                                         | `src/graph.rs`                              |
| Byte-level format contract (**authoritative**)                         | `FORMAT.md`                                 |

Every document, and when to read it: [`docs/index.md`](docs/index.md). Consult that
rather than this table when you are looking for _where something is written down_.

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

Any single suite: `cargo test --test <name>` (`ls tests/` lists them). The two that
carry throughput assertions need `--release`. Suites whose scope is not obvious from
the name are described in [`docs/testing.md`](docs/testing.md).

- **Run the benchmarks**:
  ```bash
  cargo bench --bench throughput      # full; GL_SCALE=small for a smoke run
  cargo bench --bench pool_probe
  cargo bench --bench mem_probe
  ```
- **Run a real dataset — the only end-to-end check at realistic scale.** Do this for
  any page, checksum, or replay change:

  ```bash
  DATASET_PATH=/data/com-dblp.ungraph.txt DB_DIR=/data/bench POOL_MB=256 \
    cargo bench --bench snap_dblp_bench
  DATASET_PATH=/data/soc-LiveJournal1.txt DB_DIR=/data/bench POOL_MB=1024 \
    cargo bench --bench snap_livejournal_bench
  ```

  They take `DATASET_PATH`/`DATASET_DIR`, `DB_DIR`, `POOL_MB`, `MAX_EDGES`,
  `AUTO_CHECKPOINT_MB` and print the configuration they ran with. Leave
  `AUTO_CHECKPOINT_MB=0`: the engine's 64 MB default roughly halves bulk ingest
  throughput and these benchmarks checkpoint explicitly.

  **Red lines — a change here is a correctness regression, not noise:**
  - LiveJournal edge ingestion **≥150,000 ops/s**
  - LiveJournal hub 1-hop / 2-hop **exactly** 335,194 / 10,027,730
  - com-DBLP hub 1-hop / 2-hop **exactly** 10,080 / 161,877

  Hub selection must stay a **total order** (degree descending, then raw id
  ascending): com-DBLP has three nodes tied at degree 164 _exactly at rank 50_, so a
  partial order makes the 2-hop total depend on sort internals. Rates vary by run;
  the exact totals are what detect a regression. Method and conditions:
  [`docs/benchmarks.md`](docs/benchmarks.md).

- **Verify an SDK.** The copy is mandatory, not a convenience: Node's `require`
  cannot load a `.so`/`.dylib` (it fails with "Invalid or unexpected token"), and the
  Python module must be importable as `nervusdb`, which only the renamed `.so`
  provides. `.node-version` pins the runtime both workflows use.
  ```bash
  cargo build -p nervusdb-node && cp target/debug/libnervusdb_node.dylib bindings/nodejs/nervusdb.node
  cd bindings/nodejs && node test.mjs

  cargo build -p nervusdb-python && cp target/debug/libnervusdb_python.dylib bindings/python/nervusdb.so
  cd bindings/python && PYTHONPATH=. python3 tests/test_nervusdb.py
  ```

### 3.5 Releasing

`.github/workflows/release.yml`, triggered by a `v*` tag **or** a manual dispatch.
Publishing is irreversible — a version cannot be overwritten and a released version
cannot be deleted — so the two jobs are separated:

- `verify` runs on the tag: the whole §3.1 gate re-run on the tagged commit (a tag can
  point at a commit that never passed CI), a check that the tag matches the manifest
  version, and `cargo package --locked`. It publishes nothing.
- `publish` runs **only** on `workflow_dispatch` with `confirm: publish`, behind the
  `release` GitHub Environment. A tag alone never publishes.

Required secrets, all referenced **only** in the `publish` job:

| Secret                 | For                        |
| ---------------------- | -------------------------- |
| `CARGO_REGISTRY_TOKEN` | crates.io                  |
| `PYPI_API_TOKEN`       | PyPI (the wheel build)     |
| `NPM_TOKEN`            | npm (the platform package) |

The `release` environment exists (created 2026-09-13; check with
`gh api repos/ysankpia/nervusdb/environments`). The three secrets do **not** exist yet
and must be added before the first publish — the workflow will fail without them, and
adding required reviewers to the environment is what turns its gate into a human
approval rather than just a label.

The registry name and version become permanent at that point, which is what the tag/
manifest check above exists to protect.

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

### 4.4 Cypher Surface (Supported Grammar)

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

Constraints the executor relies on (mechanism and rationale:
[`docs/architecture.md`](docs/architecture.md) §7):

- **Pattern property values are expressions, not literals** — `UNWIND ... AS x
CREATE (n {v: x})` must read `x`. But `MATCH` / `MERGE` pattern properties are
  **validated as literals at parse time**: a pattern is matched before any variable is
  bound, so an expression there could never be evaluated, and matching nothing
  silently looks like an empty graph.
- **`is_mutating()` is the single source of truth** for read-lock vs write-lock
  routing. `MERGE` is unconditionally mutating (whether it writes is only known after
  matching); `UNWIND` is mutating only when it carries `CREATE`.
- **`SET` / `DELETE` on a scalar binding is an error**, never a silent no-op.
- **Statements are atomic**: a failed write statement is undone through
  `NervusDb::rollback_failed_statement` before the error returns. Do not add a write
  entry point that skips it.
- **Variable binding discipline**: contexts bind `Binding::Node(u64) | Binding::Edge(u64) | Binding::Value(Value)`.
  Never key a context by a raw `u64` alone; node and edge ids share a numbering space.
- **Aggregation**: `count(*)` counts rows, `count(x)` non-null bindings; `sum` returns
  `Int` when every input is integral, `avg` always `Float`, `min`/`max` preserve type.
  An empty group yields `count = 0`, `sum = 0`, null for `avg`/`min`/`max`.
- **Delete**: `DELETE` on a node with relationships is a hard error advising
  `DETACH DELETE`; `DETACH DELETE` cascades and chains freed slots into the Freelist.
- **In-place node rewrites** (`SET n:Label`) use `DiskGraph::update_node_payload`,
  never `insert_node_with_id_exact`, which would inflate `node_count`.

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

### 5.1 Development workflow: two long-lived branches

There are exactly **two** branches, and no topic branches:

- **`main`** — the project. What gets released and what a new reader clones.
- **`develop`** — where work happens. Every change lands here first.

Do the work on `develop`, then merge it into `main`. Merging does not end the branch —
`develop` keeps going, receives the next change, and merges again. There is no branch
to create, name, or delete, and so no naming convention to get wrong.

```bash
git switch develop
# ... make the change, run the full CI gate from §3.1 ...
git commit
git push
# when it should land on main:
git switch main && git merge --no-ff develop && git push
git switch develop
```

`main` is protected: force pushes and deletions are refused, `strict` is on (a PR must
be up to date with `main` before merging), and **all five CI checks** must pass:
`Test (ubuntu-latest)`, `Test (macos-latest)`, `SDK (node)`, `SDK (python)`,
`Docs build`. Because of that protection the merge into `main` goes through a pull
request opened from `develop` — the same two branches, one PR per merge.

A topic branch is warranted only when work must be **parked**: an experiment that may
be abandoned, or a change that must not touch `develop` until it is known good.
Otherwise it is one more thing to name and clean up.

The two `SDK` checks exist because `cargo test --workspace` compiles the binding
crates but has no test target to run there, so their end-to-end suites were invisible
to every check that preceded them. They are **required** rather than advisory: the
first run of `SDK (node)` caught a real defect (Node's `require` cannot load a
`.so`/`.dylib`, so the binding must be copied to `.node` first), and one that a local
green run had masked because the copy already existed on the developer's machine and
is gitignored.

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

---

## Agent skills

Configuration for the engineering skills (`/triage`, `/to-tickets`, `/to-spec`,
`/implement`, `/code-review`, `/wayfinder`). Written by
`/setup-matt-pocock-skills`; edit the files below directly rather than re-running it,
unless the issue tracker itself changes.

### Issue tracker

GitHub Issues on `ysankpia/nervusdb`, via the `gh` CLI. Issues are the only accepted
inbound channel — external PRs are closed automatically by policy (§5.2), so the
"PRs as a request surface" flag is permanently `no`. See
`docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles under their default names (`needs-triage`, `needs-info`,
`ready-for-agent`, `ready-for-human`, `wontfix`); only `wontfix` already exists. See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` at the root plus `docs/adr/`, both created lazily by
`/domain-modeling` when a term or decision is actually resolved — not scaffolded in
advance. See `docs/agents/domain.md` for how they relate to `FORMAT.md` and this file,
which already define much of the domain vocabulary. The same file lists two vocabulary
traps worth knowing before naming anything: node and edge IDs share a space, and this
engine has no page latches.
