# NervusDB Architecture

> Rust-native, crash-safe embedded property graph database.

## Overview

NervusDB is organized as a layered Rust workspace. Each crate has a clear
responsibility and dependency direction flows downward.

```
┌──────────────────────────────────────────────────────┐
│  Language Bindings                                    │
│  Python (PyO3)  │  Node.js (N-API)                   │
├──────────────────────────────────────────────────────┤
│  nervusdb (Facade)                                   │
│  Db  │  ReadTxn  │  WriteTxn  │  DbSnapshot          │
├──────────────────────────────────────────────────────┤
│  nervusdb-query (Query Engine)                       │
│  Lexer → Parser → AST → Planner → Executor           │
├──────────────────────────────────────────────────────┤
│  nervusdb-storage (Storage Engine)                   │
│  WAL  │  MemTable  │  L0Run  │  CSR  │  Pager        │
├──────────────────────────────────────────────────────┤
│  nervusdb-api (Types + Traits)                       │
│  GraphStore  │  GraphSnapshot  │  PropertyValue       │
├──────────────────────────────────────────────────────┤
│  OS (pread/pwrite, fsync)                            │
└──────────────────────────────────────────────────────┘
```

## Crate Structure

```
nervusdb/
├── nervusdb-api/       # Layer 0 — trait definitions, shared types
├── nervusdb-storage/   # Layer 1 — storage engine
├── nervusdb-query/     # Layer 2 — query engine
├── nervusdb/           # Layer 3 — public facade (Db::open, query, execute)
├── nervusdb-cli/       # CLI tool
├── nervusdb-pyo3/      # Python binding (PyO3)
├── nervusdb-node/      # Node.js binding (N-API, separate workspace)
├── fuzz/               # Fuzz targets (separate workspace)
└── scripts/            # TCK gates, benchmarks, release scripts
```

Dependency graph:

```
nervusdb (Facade)
  ├── nervusdb-api      (trait definitions)
  ├── nervusdb-storage  (storage engine) ── depends on ── nervusdb-api
  └── nervusdb-query    (query engine)   ── depends on ── nervusdb-api
```

## Storage Engine (nervusdb-storage)

### File Layout

Two files per database: `<path>.ndb` (page store) + `<path>.wal` (redo log).

- Page size: 8 KB (hardcoded)
- Node IDs: `u32` (up to ~4 billion)
- Edge identity: `(src, rel_type, dst)` triple

Page 0 is the meta page containing file magic, version, bitmap pointer,
ID counters, index catalog root, and `storage_format_epoch`.

### Write Path (LSM-Tree Variant)

```
Client → WAL (fsync) → MemTable → commit → L0Run (in-memory)
                                                 ↓ compact()
                                           CsrSegment (on-disk .ndb)
```

1. Every mutation is first appended to the WAL with CRC32 checksums.
2. The MemTable accumulates edges, properties, and tombstones in memory.
3. On commit, the MemTable freezes into an immutable L0Run.
4. Compaction merges L0Runs into on-disk CsrSegments (CSR format).

### Read Path

```
Client → Snapshot
           ├── L0Run[] (newest first, in-memory)
           ├── CsrSegment[] (on-disk)
           └── B-Tree Property Store (on-disk)
```

Snapshots are lock-free Arc clones. Property reads check L0Runs first
(no lock), then fall back to the B-Tree property store (requires Pager
read lock).

### WAL Records

The WAL uses a hybrid record format — both logical records (CreateNode,
SetNodeProperty) and physical records (PageWrite). Recovery replays only
committed transactions.

### CSR Segments

Compressed Sparse Row format for edge storage. Each segment stores both
outgoing and incoming edge arrays with row offsets, enabling efficient
neighbor traversal in both directions.

### Property Storage

Two-tier architecture:
1. L0Run in-memory properties (latest uncommitted data, lock-free reads)
2. B-Tree persistent store (compacted data on disk)

Key encoding: `[tag:1][node_id:4][key_len:4][key_bytes]`

### Auxiliary Subsystems

| Module | Purpose |
|--------|---------|
| backup | Online backup API (copies .ndb file) |
| bulkload | Offline bulk loader (bypasses WAL) |
| vacuum | In-place vacuum (rewrites .ndb with only reachable pages) |
| blob_store | Large value storage (4 KB page chains) |
| idmap | ExternalId ↔ InternalNodeId mapping |
| label_interner | Label name ↔ LabelId mapping |
| index_catalog | B-Tree and HNSW index management |

## Query Engine (nervusdb-query)

### Pipeline

```
Cypher String → Lexer → Parser → AST → prepare() → PreparedQuery
                                                         ↓
                                           executor::execute_plan()
                                                         ↓
                                                    Row Stream
```

### AST

The parser produces a full Cypher AST supporting:
- Clauses: MATCH, CREATE, MERGE, DELETE, SET, REMOVE, WITH, RETURN,
  WHERE, UNWIND, CALL, UNION, FOREACH, ORDER BY, SKIP, LIMIT
- Expressions: literals, variables, property access, binary/unary ops,
  function calls, CASE, EXISTS, list comprehension, map projection,
  parameters

### Key Files

| File | Purpose |
|------|---------|
| `executor.rs` | All execution logic (pattern matching, mutations, aggregation) |
| `evaluator.rs` | Expression evaluation |
| `query_api.rs` | Query preparation and plan generation |
| `parser.rs` | Cypher parser |
| `ast.rs` | AST type definitions |

## Transaction Model

- **Single Writer**: one write transaction at a time, serialized via mutex.
- **Snapshot Readers**: concurrent readers get a consistent snapshot (Arc clone)
  without blocking the writer.
- **Isolation**: snapshot isolation — readers see a frozen point-in-time view.

## Binding Architecture

### Python (PyO3)

The `nervusdb-pyo3` crate wraps the Rust `Db` type with Python-friendly APIs.
Write statements must go through `execute_write()` or `WriteTxn`. Error types
map to a Python exception hierarchy: `NervusError` → `SyntaxError` /
`ExecutionError` / `StorageError` / `CompatibilityError`.

### Node.js (N-API)

The `nervusdb-node` crate (separate workspace) exposes a synchronous N-API
binding. Errors are returned as structured JSON payloads with `code`,
`category`, and `message` fields.

### Parity Principle

All three platforms (Rust, Python, Node.js) must exhibit identical behavior
for the same Cypher input. Binding-level differences are not tolerated —
if Rust has a gap, all three platforms must reflect that same gap.
