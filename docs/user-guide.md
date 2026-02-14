# NervusDB User Guide

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [Database Operations](#database-operations)
4. [Cypher Query Guide](#cypher-query-guide)
5. [Backup and Restore](#backup-and-restore)
6. [Configuration](#configuration)
7. [Performance Tips](#performance-tips)

---

## Installation

### Prerequisites

- Rust 1.70 or later
- Cargo (comes with Rust)

### Install from Source

```bash
cargo install nervusdb-cli
```

### Install from Crates.io

```bash
cargo add nervusdb
```

### Download Binary

Download prebuilt binaries from [GitHub Releases](https://github.com/LuQing-Studio/nervusdb/releases).

---

## Quick Start

### Opening a Database

```rust
use nervusdb::Db;

// Open or create a database at the specified path
let db = Db::open_paths(["/path/to/mygraph.ndb"]).unwrap();
```

### Creating Nodes and Relationships

```cypher
// Create a single node
CREATE (person {name: 'Alice', age: 30})

// Create nodes and a relationship
CREATE (a {name: 'Alice'})-[:FRIEND {since: 2020}]->(b {name: 'Bob'})
```

### Querying Data

```cypher
// Find all friends of Alice
MATCH (a {name: 'Alice'})-[:FRIEND]->(b) RETURN b.name, b.age

// Find paths up to 5 hops
MATCH (a)-[*1..5]->(b) WHERE a.name = 'Alice' RETURN a, b
```

---

## Database Operations

### Rust API

```rust
use nervusdb::Db;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open database
    let db = Db::open_paths(["/tmp/graph.ndb"])?;

    // Execute write operations
    db.execute("CREATE (n {id: 1, name: 'Test'})", None)?;

    // Execute read queries
    let results = db.query("MATCH (n) RETURN n", None)?;

    for record in results {
        println!("{:?}", record);
    }

    Ok(())
}
```

### Using Snapshots

For consistent reads across multiple queries, use snapshots:

```rust
let snapshot = db.snapshot()?;
let results = snapshot.query("MATCH (n) RETURN n")?;
```

---

## Cypher Query Guide

### Reading Data

#### Basic Match

```cypher
-- Match all nodes
MATCH (n) RETURN n

-- Match with label
MATCH (p:Person) RETURN p.name, p.age

-- Match with relationship
MATCH (a)-[:FRIEND]->(b) RETURN a.name, b.name
```

#### Filtering with WHERE

```cypher
-- Property comparison
MATCH (n) WHERE n.age > 25 RETURN n

-- Boolean operators
MATCH (n) WHERE n.name = 'Alice' AND n.age >= 30 RETURN n

-- IN clause
MATCH (n) WHERE n.name IN ['Alice', 'Bob'] RETURN n
```

#### Ordering and Pagination

```cypher
-- Order by
MATCH (n) RETURN n ORDER BY n.age DESC

-- Skip and limit
MATCH (n) RETURN n ORDER BY n.age SKIP 10 LIMIT 5
```

#### Aggregation

```cypher
-- Count
MATCH (n) RETURN COUNT(n)

-- Collect
MATCH (a)-[:FRIEND]->(b) RETURN a.name, COLLECT(b.name) AS friends

-- Min/Max/Sum
MATCH (n) RETURN MIN(n.age), MAX(n.age), AVG(n.age)
```

### Writing Data

#### CREATE

```cypher
-- Create node
CREATE (n {name: 'Alice', age: 30})

-- Create with label
CREATE (p:Person {name: 'Bob'})

-- Create relationship
CREATE (a {name: 'Alice'})-[:KNOWS {since: 2021}]->(b {name: 'Carol'})
```

#### MERGE (Idempotent Create)

```cypher
-- Merge node (create if not exists)
MERGE (p:Person {name: 'Alice'})
ON CREATE SET p.created_at = timestamp()

-- Merge relationship
MATCH (a {name: 'Alice'}), (b {name: 'Bob'})
MERGE (a)-[r:FRIEND]->(b)
ON CREATE SET r.since = 2023
```

#### DELETE

```cypher
-- Delete node
MATCH (n {name: 'Bob'}) DELETE n

-- Detach delete (remove relationships first)
MATCH (n {name: 'Bob'}) DETACH DELETE n
```

#### SET

```cypher
-- Set property
MATCH (n {name: 'Alice'}) SET n.age = 31

-- Add label
MATCH (n) SET n:Premium
```

---

## Backup and Restore

### Creating a Backup

```rust
use nervusdb::Db;

let db = Db::open_paths(["/tmp/graph.ndb"])?;

// Start a backup
let handle = db.begin_backup()?;

// Execute backup (can be done in background thread)
db.execute_backup(&handle)?;

// Mark backup as complete
db.complete_backup(&handle)?;
```

### Restoring from Backup

```rust
use nervusdb::Db;

// Restore to a new location
Db::restore_from_backup("/tmp/backup", "/tmp/restored.ndb")?;
```

---

## Configuration

### Database Options

```rust
use nervusdb::DbOptions;

let options = DbOptions::default()
    .with_cache_size(1024 * 1024 * 100); // 100MB cache

let db = Db::open_paths_with_opts(["/tmp/graph.ndb"], options)?;
```

### CLI Options

```bash
# Specify database path
nervusdb-cli v2 write --db /path/to/db --cypher "CREATE (n)"

# Query with output limit
nervusdb-cli v2 query --db /path/to/db --cypher "MATCH (n) RETURN n" --limit 100
```

---

## Performance Tips

### 1. Use Indexes for Frequent Lookups

```cypher
-- Create index on property
CREATE INDEX ON :Person(name)
```

### 2. Batch Writes for Large Data

```rust
// Use transaction for batch operations
let tx = db.begin_transaction()?;
for i in 0..10000 {
    tx.execute(&format!("CREATE (n {{id: {}}})", i), None)?;
}
tx.commit()?;
```

### 3. Use Appropriate Query Patterns

```cypher
-- Good: Filter early
MATCH (a)-[:FRIEND]->(b)
WHERE a.name = 'Alice'
RETURN b

-- Avoid: Filter late (more data in memory)
MATCH (a)-[:FRIEND]->(b)
RETURN b
WHERE a.name = 'Alice'
```

### 4. Limit Result Sets

```cypher
-- Always use LIMIT for exploratory queries
MATCH (n) RETURN n LIMIT 100
```

### 5. Use Variable Length Paths Carefully

```cypher
-- Set reasonable max depth
MATCH (a)-[:FRIEND*1..5]->(b) RETURN b

-- Avoid unbounded paths
-- DON'T: MATCH (a)-[:FRIEND*]->(b) RETURN b
```

---

## Troubleshooting

### Common Errors

| Error | Cause | Solution |
|-------|-------|----------|
| `not implemented: <feature>` | Feature not in Cypher subset | Check [cypher_support.md](reference/cypher_support.md) |
| `Backup in progress` | Another backup is running | Wait or cancel previous backup |
| `Database locked` | Another process is using the database | Close other processes |

### Debug Mode

Set environment variable for verbose logging:

```bash
RUST_LOG=debug nervusdb-cli v2 query --db /tmp/graph --cypher "MATCH (n) RETURN n"
```

---

## Next Steps

- [CLI Reference](cli.md)
- [Cypher Support Details](reference/cypher_support.md)
- [API Documentation](https://docs.rs/nervusdb)
