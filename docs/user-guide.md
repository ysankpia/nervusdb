# NervusDB User Guide

## Table of Contents

1. [Installation](#installation)
2. [Database Lifecycle](#database-lifecycle)
3. [Querying with Cypher](#querying-with-cypher)
4. [Write Operations](#write-operations)
5. [Transactions](#transactions)
6. [Streaming Queries](#streaming-queries)
7. [Indexes](#indexes)
8. [Vector Search](#vector-search)
9. [Backup and Maintenance](#backup-and-maintenance)
10. [Error Handling](#error-handling)

---

## Installation

### Rust

Add to `Cargo.toml`:

```toml
[dependencies]
nervusdb = "0.0.1"
nervusdb-query = "0.0.1"
```

### Python

```bash
pip install maturin
maturin develop -m nervusdb-pyo3/Cargo.toml
```

### Node.js

```bash
cargo build --manifest-path nervusdb-node/Cargo.toml --release
```

### CLI

```bash
cargo install --path nervusdb-cli
```

---

## Database Lifecycle

### Opening a Database

All platforms use a single path. NervusDB creates two files:
`<path>.ndb` (page store) and `<path>.wal` (write-ahead log).

**Rust:**

```rust
use nervusdb::Db;

let db = Db::open("/tmp/mydb")?;
// Or with explicit paths:
let db = Db::open_paths("/tmp/mydb.ndb", "/tmp/mydb.wal")?;
```

**Python:**

```python
import nervusdb

db = nervusdb.open("/tmp/mydb")
# Or with explicit paths:
db = nervusdb.open_paths("/tmp/mydb.ndb", "/tmp/mydb.wal")
```

**Node.js:**

```typescript
const { Db } = require("./nervusdb-node");

const db = Db.open("/tmp/mydb");
// Or with explicit paths:
const db = Db.openPaths("/tmp/mydb.ndb", "/tmp/mydb.wal");
```

### Closing a Database

Always close the database when done to flush pending writes.

```rust
db.close()?;           // Rust
```
```python
db.close()             # Python
```
```typescript
db.close();            // Node.js
```

---

## Querying with Cypher

Read queries use `query` (returns all rows) or `query_stream` (Python, returns iterator).

**Rust** — uses `nervusdb_query::query_collect` or the `QueryExt` trait:

```rust
use nervusdb::{Db, GraphSnapshot};
use nervusdb_query::{query_collect, Params};

let db = Db::open("/tmp/mydb")?;
let snapshot = db.snapshot();
let rows = query_collect(&snapshot, "MATCH (n:Person) RETURN n.name", &Params::new())?;
for row in &rows {
    println!("{:?}", row);
}
```

**Python:**

```python
rows = db.query("MATCH (n:Person) RETURN n.name")
for row in rows:
    print(row)
```

**Node.js:**

```typescript
const rows = db.query("MATCH (n:Person) RETURN n.name");
console.log(rows);
```

### Parameterized Queries

Pass parameters to avoid Cypher injection and improve readability.

**Python:**

```python
rows = db.query("MATCH (n:Person) WHERE n.name = $name RETURN n", {"name": "Alice"})
```

**Node.js:**

```typescript
const rows = db.query("MATCH (n:Person) WHERE n.name = $name RETURN n", { name: "Alice" });
```

---

## Write Operations

Write statements (`CREATE`, `MERGE`, `DELETE`, `SET`, `REMOVE`) must use the
write API. Calling `query()` with a write statement raises an error.

**Rust** — use `prepare` + `execute_write` with a write transaction:

```rust
use nervusdb::Db;
use nervusdb_query::{prepare, Params};

let db = Db::open("/tmp/mydb")?;
let snapshot = db.snapshot();
let stmt = prepare("CREATE (n:Person {name: 'Alice'})")?;
let mut txn = db.begin_write();
let count = stmt.execute_write(&snapshot, &mut txn, &Params::new())?;
txn.commit()?;
println!("Created {} node(s)", count);
```

**Python:**

```python
count = db.execute_write("CREATE (n:Person {name: 'Alice'})")
```

**Node.js:**

```typescript
const count = db.executeWrite("CREATE (n:Person {name: 'Alice'})");
```

---

## Transactions

### Write Transactions

Group multiple writes into a single atomic transaction.

**Python:**

```python
txn = db.begin_write()
txn.query("CREATE (a:Person {name: 'Alice'})")
txn.query("CREATE (b:Person {name: 'Bob'})")
txn.query("CREATE (a)-[:KNOWS]->(b)")
txn.commit()
```

**Node.js:**

```typescript
const txn = db.beginWrite();
txn.query("CREATE (a:Person {name: 'Alice'})");
txn.query("CREATE (b:Person {name: 'Bob'})");
txn.commit();
```

**Rust:**

```rust
let snapshot = db.snapshot();
let mut txn = db.begin_write();
let stmt1 = prepare("CREATE (n:Person {name: 'Alice'})")?;
let stmt2 = prepare("CREATE (n:Person {name: 'Bob'})")?;
stmt1.execute_write(&snapshot, &mut txn, &Params::new())?;
stmt2.execute_write(&snapshot, &mut txn, &Params::new())?;
txn.commit()?;
```

### Read Snapshots

Snapshots provide a consistent point-in-time view for reads.

```rust
let snapshot = db.snapshot();
// All queries on this snapshot see the same data,
// even if writes happen concurrently.
let rows = query_collect(&snapshot, "MATCH (n) RETURN n", &Params::new())?;
```

---

## Streaming Queries

Python supports streaming results for memory-efficient processing:

```python
for row in db.query_stream("MATCH (n:Person) RETURN n.name"):
    print(row)
```

---

## Indexes

Create property indexes to speed up lookups.

```rust
db.create_index("Person", "name")?;   // Rust
```
```python
db.create_index("Person", "name")     # Python
```
```typescript
db.createIndex("Person", "name");     // Node.js
```

---

## Vector Search

NervusDB includes a built-in HNSW vector index for similarity search.

```rust
let hits = db.search_vector(&[0.1, 0.2, 0.3], 10)?;  // Rust
```
```python
hits = db.search_vector([0.1, 0.2, 0.3], 10)          # Python
```
```typescript
const hits = db.searchVector([0.1, 0.2, 0.3], 10);    // Node.js
```

Each hit returns `(node_id, distance)`.

---

## Backup and Maintenance

### Backup

```python
nervusdb.backup("/tmp/mydb", "/tmp/backup-dir")   # Python
```
```typescript
const { backup } = require("./nervusdb-node");
backup("/tmp/mydb", "/tmp/backup-dir");            // Node.js
```

### Vacuum (Reclaim Space)

```python
nervusdb.vacuum("/tmp/mydb")                       # Python
```
```typescript
const { vacuum } = require("./nervusdb-node");
vacuum("/tmp/mydb");                               // Node.js
```

### Compaction and Checkpoint

```python
db.compact()      # Merge segments
db.checkpoint()   # Flush WAL to page store
```

---

## Error Handling

NervusDB uses four error categories across all platforms:

| Category | Meaning |
|----------|---------|
| `Syntax` | Invalid Cypher query |
| `Execution` | Runtime error (e.g., write in read context) |
| `Storage` | I/O or corruption error |
| `Compatibility` | Storage format epoch mismatch |

**Python** — typed exceptions:

```python
from nervusdb import SyntaxError, ExecutionError, StorageError, CompatibilityError

try:
    db.query("INVALID CYPHER")
except SyntaxError as e:
    print(f"Bad query: {e}")
```

**Node.js** — structured error payload:

```typescript
try {
    db.query("INVALID CYPHER");
} catch (e) {
    // e has: { code, category, message }
    console.error(e.category, e.message);
}
```

---

## CLI Quick Reference

```bash
# Write
cargo run -p nervusdb-cli -- v2 write --db /tmp/demo \
  --cypher "CREATE (n:Person {name: 'Alice'})"

# Query (NDJSON output)
cargo run -p nervusdb-cli -- v2 query --db /tmp/demo \
  --cypher "MATCH (n:Person) RETURN n.name"
```

See [CLI Reference](cli.md) for full details.

---

## Next Steps

- [Cypher Support Matrix](cypher-support.md) — full list of supported clauses
- [Architecture](architecture.md) — storage and query internals
- [Binding Parity](binding-parity.md) — cross-platform API coverage
