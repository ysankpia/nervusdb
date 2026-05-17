# Design T100: NervusDB v2.0 Architecture

> **Status**: Accepted
> **Author**: Linus-AGI
> **Date**: 2025-12-30
> **Scope**: Storage (Indexing), Query (Merge/Transactions), Lifecycle

## 1. Overview

This document defines the architecture for the **v2.0 "Graph SQLite" Initiative**.
The core goal is to enable **Native Indexing** and **Production-Grade Cypher** without compromising the "Single File" (embedded) philosophy.

---

## 2. Storage Architecture: LSM-Tree + Page-Backed B+Tree

We retain the LSM-Tree structure for Graph Data (Nodes/Edges) but introduce a **Page-Backed B+Tree** for Secondary Indexes (Properties).

### 2.1 The "Hybrid" LSM Model

| Component | Storage Format | Persistence | Role |
| :--- | :--- | :--- | :--- |
| **MemTable** | `BTreeMap` (Memory) | WAL (Logical) | Buffered Writes (Graph + Index updates) |
| **L0 Run** | `CsrSegment` (Frozen) | Pager (Physical) | Recent Graph Data |
| **Index** | **B+Tree Pages** (New) | Pager (Physical) | **Merged Index Data** |

### 2.2 Index Persistence Strategy

Unlike v1 (separate RocksDB/Sled), v2 Indexes live inside the `.ndb` Pager.

1.  **Write Path (No Write Amp)**:
    -   `INSERT (n {age: 30})` -> Writes to `MemTable` (Memory).
    -   `MemTable` accumulates index entries: `(age=30, id=100)`.
    -   WAL logs: `SetNodeProperty { ... }` (No specific "Index" log needed, replay rebuilds MemTable).

2.  **Flush/Compaction Path (Merge)**:
    -   When `MemTable` flushes or `Compact` runs:
    -   We scan the `MemTable` (and potentially L0 runs).
    -   We **bulk insert** these keys into the On-Disk B+Tree.
    -   **Critical**: B+Tree updates utilize **Shadow Paging (CoW)** or are atomic within the Compaction Transaction.
    -   Since Compaction is a "System Transaction" (logged in WAL), if it crashes, we rollback to the previous Manifest.

3.  **Read Path**:
    -   `MATCH (n) WHERE n.age = 30`
    -   Step 1: Scan `MemTable` (Memory) -> Find `id=100`.
    -   Step 2: Seek On-Disk B+Tree (Pager) -> Find `id=50, id=20`.
    -   Result: `[100, 50, 20]`.

### 2.3 Page Structure (New)

We reserve a new `PageType` in the Pager.

```rust
struct IndexMetaPage {
    magic: [u8; 8], // "NDBIDXv1"
    root_page: u64,
    depth: u32,
    // Catalog of named indexes? Or one tree per index?
    // Start with: One Master B-Tree mapping "IndexName" -> "RootPageId"
}

struct BTreeNodePage {
    flags: u8, // Leaf or Internal
    count: u16,
    right_sibling: u64, // For range scans
    cells: [u8], // Key-Value pairs
}
```

---

## 3. Query Architecture: MERGE & Transactions

### 3.1 The `MERGE` Operator

`MERGE (n {id: 1})` is "Read-then-Write".

**Execution Flow**:
1.  **Begin Write Txn**: Acquire Global Write Lock.
2.  **Scan**: Execute `MATCH (n {id: 1})`.
    -   Use Index if available.
    -   Use Scan if not.
3.  **Branch**:
    -   If found: Return existing `n`.
    -   If not found: Call `CreateNode`, then `SetProperty`.
4.  **Commit**: Write to WAL.

**Concurrency**:
Since v2 is **Single Writer**, `MERGE` is safe from race conditions (no "Double Insert" risk) as long as we hold the write lock throughout the operation.

### 3.2 Transaction Exposure

Currently, `nervusdb` API implicitly creates transactions. We need explicit control for bindings.

```rust
// New API
let mut tx = db.begin_write()?;
tx.query("MERGE (n {id: 1})")?;
tx.query("CREATE (n)-[:KNOWS]->(m)")?;
tx.commit()?;
```

---

## 4. Lifecycle: Checkpoint-on-Close

To achieve the "Single File at Rest" goal:

1.  **Runtime**: `db.open()` creates/opens `graph.wal`. Writes append to WAL.
2.  **Shutdown**: `db.close()` (or Drop) triggers `CheckpointMode::Force`.
    -   Replay ALL WAL frames into `.ndb` pages.
    -   `fsync` `.ndb`.
    -   Delete `graph.wal`.
3.  **Crash Recovery**:
    -   If `db.open()` sees `graph.wal`, it means unclean shutdown.
    -   Run standard WAL Recovery (Replay).

---

## 5. Ecosystem: UniFFI Architecture

We will introduce a new crate `nervusdb-uniffi`.

```text
nervusdb/
  ├── nervusdb/         (Rust API)
  └── nervusdb-uniffi/     (Foreign Binding Layer)
      ├── src/lib.rs       (UDL/Proc Macros)
      └── build.rs
```

**Interface (UDL)**:
```webidl
interface GraphDB {
    constructor(string path);
    void execute(string cypher);
    sequence<Row> query(string cypher);
    void close();
};
```
