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

5. **Explicit Batched Transactions**
   - Autocommitted single writes each cost one WAL append plus one fsync. Bulk ingestion must use `GraphLite::with_transaction` (Rust), `db.begin_transaction()` (Python/Node), and commit once so the whole batch shares **exactly one fsync**.
   - `BufferStats::wal_fsync_count` / `wal_frames_written` are the observable contract for this: a test asserting bulk-commit semantics asserts the fsync delta is exactly 1.

6. **Storage Format Versioning**
   - `DB_PAGE_VERSION` is `2` (slotted property pages). Version 1 databases (one 4KB property page per entity) are **not readable**: `GraphLite::open` returns an explicit error directing the user to export with GraphLite 1.0 via `.dump` and re-import. Never silently reinterpret an old file.

7. **Freelist Slot Reclamation**
   - Page 0 (Header Page) stores `first_free_node_id` and `first_free_edge_id`.
   - Deleted records must be chained into the respective Freelist.
   - New allocations must prioritize popping from the Freelist before advancing `next_id`, completely eliminating disk space fragmentation.

8. **Lock De-escalation & Concurrency Safety**
   - Avoid holding global write locks during long traversals. Read queries and graph algorithms (`dijkstra`, `bfs`, `has_cycle`, `query()`) must clone the lightweight `DiskGraph` handle, release the global read lock immediately, and execute concurrently under page-level latches.
   - All pointer traversal loops (`while curr != 0`) must enforce cycle-detection guards (`seen: HashSet<u64>`) to guarantee zero infinite loops under concurrent pointer updates.

---

## 2. Codebase Map & Module Responsibilities

```text
/Users/luhui/Desktop/graphlite-rs/
├── Cargo.toml                  # Workspace root manifest (core, python, nodejs)
├── README.md                   # User documentation & architecture guide
├── AGENTS.md                   # This developer & agent standard specification
├── src/
│   ├── lib.rs                  # Public facade (GraphLite, Transaction), ACID coordinator
│   ├── main.rs                 # Standalone demo binary
│   ├── bin/cli.rs              # Interactive REPL tool (graphlite-cli) with ASCII table
│   ├── page.rs                 # 4KB page layout, NodeRecord (32B), EdgeRecord (64B), SlottedPropPage, PropCodec
│   ├── buffer.rs               # DiskManager (paged I/O) and LRU BufferPoolManager (page latches, STEAL spill)
│   ├── disk_graph.rs           # O(1) direct addressing, disk adjacency, Freelist, page iterators
│   ├── storage.rs              # Page-level WAL engine (WalWriter), CRC32 verification, checkpointing, recovery
│   ├── index.rs                # Secondary indexing (Label inverted index & Property BTreeMap index)
│   ├── query.rs                # Chainable strongly-typed QueryBuilder & GraphQuery DSL
│   ├── algo.rs                 # Pure-disk graph algorithms (BFS, Dijkstra, Cycle, PageRank, WCC, K-Hop)
│   ├── cypher/
│   │   ├── ast.rs              # AST statements, expressions, and pattern nodes
│   │   ├── lexer.rs            # Cypher tokenizer (comments, aggregate keywords)
│   │   ├── parser.rs           # Recursive-descent Cypher parser (SET/ORDER BY/SKIP/aggregates)
│   │   ├── executor.rs         # Execution engine leveraging disk cursors and secondary indexes
│   │   └── mod.rs              # Cypher module exports
│   └── graph.rs                # Domain models (Node, Edge, Value, Direction, GraphError)
├── bindings/
│   ├── python/                 # Official Python SDK (PyO3 0.22 + Maturin, abi3)
│   └── nodejs/                 # Official Node.js / TypeScript SDK (NAPI-RS 2.16)
└── tests/
    ├── integration_tests.rs    # Core CRUD/ACID/concurrency/index/stress regression suites
    ├── cypher_advanced_tests.rs# Cypher 1.0 syntax closure (SET/DETACH DELETE/ORDER BY/aggregates/paging)
    ├── analytics_tests.rs      # PageRank / WCC / K-Hop subgraph analytics
    ├── steal_spill_tests.rs    # STEAL spilling, rollback zero-pollution, checkpoint semantics
    ├── slotted_property_tests.rs # Slotted page packing, slot reuse, compaction, density target
    ├── batch_tx_tests.rs       # Batched transactions, single-fsync contract, bulk throughput
    └── cli_tests.rs            # Interactive REPL end-to-end (multi-line, dot commands, dump round trip)
```

---

## 3. Engineering Workflows & Verification Commands

All agents modifying this codebase must execute the relevant checks before concluding any task:

### 3.1 Workspace Compilation Check

Must compile with **zero warnings and zero errors**:

```bash
cargo check --workspace
```

### 3.2 Full Test Suite Regression

Must pass every test suite (including the 1MB out-of-core memory stress tests):

```bash
cargo test --workspace
```

### 3.3 Target-Specific Verification

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
- **Verify Interactive CLI**:
  ```bash
  cargo run --bin graphlite-cli -- /tmp/test.db
  ```
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
MATCH pat [, pat ...] [WHERE expr]
      [SET item [, item ...]]
      [DELETE var... | DETACH DELETE var...]
      [CREATE pat]
      [RETURN item [, item ...]]
      [ORDER BY expr [ASC|DESC], ...] [SKIP n] [LIMIT n]

item   := * | var | var.key [AS alias] | FUNC(*) | FUNC(var[.key]) [AS alias]
FUNC   := count | sum | avg | min | max
SET    := var.key = <literal|var.key|var> | var:Label
pat    := (var:Label1:Label2 {k: v}) -[r:TYPE*min..max {k: v}]-> (var)
```

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

## 5. Cleanliness & Commit Standards

- Never commit test database artifacts (`*.db`, `*.db.wal`, `*.paged`).
- Keep `.gitignore` updated for target builds, Node binaries (`*.node`), and Python dynamic libraries (`*.so`, `*.dylib`).
- No placeholder code: Strictly forbidden to introduce `todo!()` or `unimplemented!()`.
