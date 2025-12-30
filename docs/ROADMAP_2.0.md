# NervusDB v2.0 Roadmap: The "Graph SQLite" Initiative

> **Vision**: To be the default **Embedded Graph Database** for the AI and Edge Computing era.
>
> **Philosophy**:
> 1.  **Architecture 100%**: Single-file, Crash-safe, Native Indexing.
> 2.  **Functionality 100%**: Full Cypher CRUD, Vector Search, Multi-language Bindings.
> 3.  **Quality 100%**: Fuzz-tested, Production-ready reliability.

---

## 🏆 Phase 1: The Core (Architecture & Cypher Parity)
**Timeline**: Month 1 (v2.0.0-beta)
**Focus**: Completing the database kernel and query engine to support real-world application logic.

### 1.1 Native Indexing (Storage Engine)
*Goal: Queries should be O(log N), not O(N).*
- [ ] **Page-Backed B+Tree**: Implement a B+Tree index that lives inside the `.ndb` pager (no external files).
- [ ] **Snapshot Isolation**: Ensure B-Tree updates utilize Copy-on-Write (CoW) or MVCC so readers always see a consistent state.
- [ ] **WAL Integration**: Ensure index updates (Insert/Delete) are atomic with data updates via WAL.
- [ ] **Query Optimizer**: Update `Planner` to automatically use indexes for `WHERE` clauses (Cost-Based Optimization MVP).

### 1.2 Cypher Completeness (Query Engine)
*Goal: Write once, run anywhere (Cypher compatibility).*
- [ ] **`EXPLAIN` Clause**: Allow users to inspect the execution plan (scan vs index seek).
- [ ] **`MERGE` Clause**: Implement idempotent "Create or Match" logic (Critical for data ingestion).
- [ ] **`OPTIONAL MATCH`**: Support left-outer-join style pattern matching.
- [ ] **Aggregations**: Complete support for `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` with `GROUP BY` (implicit).
- [ ] **Functions**: Add standard string/math functions (e.g., `toUpper`, `substring`, `size`).

### 1.3 Auto-Management
*Goal: Zero-config operation.*
- [ ] **Checkpoint-on-Close**: Automatically merge `.wal` back into `.ndb` and delete the log file on clean shutdown (Portability).
- [ ] **Auto-Compaction**: Trigger background L0->L1 compaction based on write amplification/tombstone ratio.
- [ ] **Vacuum**: API to reclaim free pages and shrink the `.ndb` file size.

---

## 🚀 Phase 2: The Ecosystem (Bindings & AI)
**Timeline**: Month 2 (v2.0.0-rc)
**Focus**: Making NervusDB accessible to Python/JS developers and AI workflows.

### 2.1 Multi-Language Bindings (UniFFI)
*Goal: `pip install nervusdb` / `npm install nervusdb`.*
- [ ] **Bulk Import Tool**: High-performance CLI tool to ingest CSV/JSONL files directly into `.ndb` (bypass WAL for speed).
- [ ] **UniFFI Core**: Create `nervusdb-uniffi` crate to expose a stable C-ABI.
- [ ] **Python Binding**: Full Python support (sync API first) for Data Science/AI integration.
- [ ] **Node.js Binding**: TypeScript definitions and N-API bindings for web backends.

### 2.2 Native Vector Search (AI Ready)
*Goal: The best embedded database for RAG (Retrieval-Augmented Generation).*
- [ ] **HNSW Index**: Implement Hierarchical Navigable Small World graphs on Pager.
- [ ] **Vector Storage**: Optimized storage for `Vec<f32>` properties.
- [ ] **Similarity Search**: Support `CALL vector.search(index, query_vector, k)` in Cypher.

---

## 🛡 Phase 3: Industrial Quality (Trust)
**Timeline**: Ongoing (v2.0.0-GA)
**Focus**: Reliability, Performance, and Security.

### 3.1 Extreme Testing
*Goal: Break it before the user does.*
- [ ] **Fuzz Testing**: Use `cargo-fuzz` to generate random Cypher queries and graph topologies to find panics.
- [ ] **Chaos Testing**: Simulate IO errors (disk full, permission denied) during WAL commits to verify recovery.
- [ ] **Long-Running Tests**: 24h stability tests under high concurrency.

### 3.2 Performance & Benchmarking
*Goal: Proven speed.*
- [ ] **Benchmark Suite**: Standardized comparison vs SQLite (Relational) and Neo4j (Graph).
- [ ] **Performance Profile**: Publish P99 latency numbers for common queries (1-hop, 2-hop, shortest path).

---

## 📊 Feature Matrix Target (v2.0 GA)

| Feature | SQLite | NervusDB v1 | NervusDB v2 (Goal) |
| :--- | :---: | :---: | :---: |
| **Storage Model** | B-Tree (Table) | Redb (KV) | **LSM-CSR (Graph)** |
| **File Format** | Single File | Single File | **Single File (at rest) / +WAL (runtime)** |
| **Vector Search** | Plugin (sqlite-vec) | ❌ | **Native (Built-in)** |
| **Language** | C | Rust | **Rust** |
| **Query Lang** | SQL | Cypher | **Cypher + Vector** |
| **Crash Safe** | ✅ | ✅ | **✅ (WAL)** |
| **Bindings** | All | Py/Node/C | **Py/Node/Rust/C** |

---

## 📝 Immediate Next Steps (The "Sprint")

1.  **Refactor Storage**: Add `Index` trait and `BTree` implementation in `nervusdb-v2-storage`.
2.  **Update Planner**: Add `Merge` and `OptionalMatch` nodes to `nervusdb-v2-query`.
3.  **Setup UniFFI**: Initialize `nervusdb-uniffi` crate structure.
