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
   - Never create auxiliary or split database files (e.g., `.paged`, `.snapshot`, etc.).
   - WAL frames must record standard physical page mutations: `WalRecord::PageWrite { tx_id, page_id, crc32, data: 4KB }`.
   - Transactions commit only to WAL. Checkpoints flush dirty buffer frames to `{path}` and truncate the WAL. Crash recovery replays valid committed pages from WAL directly into `{path}`.

3. **Fixed-Size Records & Disk-Native Index-Free Adjacency**
   - `NodeRecord` is strictly 32 bytes (128 records per 4KB page).
   - `EdgeRecord` is strictly 64 bytes (64 records per 4KB page).
   - Direct $O(1)$ physical addressing formula:
     $$\text{PageId} = \text{BasePage} + \frac{N \times \text{sizeof(Record)}}{4096}$$
     $$\text{Offset} = (N \times \text{sizeof(Record)}) \pmod{4096}$$
   - Adjacency traversal follows double-cyclic edge pointer chains across disk pages via `BufferPoolManager`. Never perform full table scans to find neighbors.

4. **Freelist Slot Reclamation**
   - Page 0 (Header Page) stores `first_free_node_id` and `first_free_edge_id`.
   - Deleted records must be chained into the respective Freelist.
   - New allocations must prioritize popping from the Freelist before advancing `next_id`, completely eliminating disk space fragmentation.

5. **Lock De-escalation & Concurrency Safety**
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
│   ├── page.rs                 # 4KB page layout, NodeRecord (32B), EdgeRecord (64B), PropertyPage
│   ├── buffer.rs               # DiskManager (paged I/O) and LRU BufferPoolManager (page latches)
│   ├── disk_graph.rs           # O(1) direct addressing, disk adjacency, Freelist, page iterators
│   ├── storage.rs              # Page-level WAL engine, CRC32 verification, checkpointing, recovery
│   ├── index.rs                # Secondary indexing (Label inverted index & Property BTreeMap index)
│   ├── query.rs                # Chainable strongly-typed QueryBuilder & GraphQuery DSL
│   ├── algo.rs                 # Pure-disk graph algorithms (BFS, Dijkstra, Cycle Detection)
│   ├── cypher/
│   │   ├── ast.rs              # AST statements, expressions, and pattern nodes
│   │   ├── lexer.rs            # Cypher tokenizer
│   │   ├── parser.rs           # Recursive-descent Cypher parser
│   │   ├── executor.rs         # Execution engine leveraging disk cursors and secondary indexes
│   │   └── mod.rs              # Cypher module exports
│   └── graph.rs                # Domain models (Node, Edge, Value, Direction, GraphError)
├── bindings/
│   ├── python/                 # Official Python SDK (PyO3 0.22 + Maturin, abi3)
│   └── nodejs/                 # Official Node.js / TypeScript SDK (NAPI-RS 2.16)
└── tests/
    └── integration_tests.rs    # 10 comprehensive integration & stress test suites
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

Must pass all 10 test suites (including 1MB out-of-core memory stress tests):

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
  cp target/debug/libgraphlite.dylib bindings/python/graphlite.so
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

---

## 5. Cleanliness & Commit Standards

- Never commit test database artifacts (`*.db`, `*.db.wal`, `*.paged`).
- Keep `.gitignore` updated for target builds, Node binaries (`*.node`), and Python dynamic libraries (`*.so`, `*.dylib`).
- No placeholder code: Strictly forbidden to introduce `todo!()` or `unimplemented!()`.
