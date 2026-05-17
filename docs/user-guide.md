# NervusDB 0.1 User Guide

This guide documents the current 0.1 path: Rust-first embedded use, local files,
WAL-backed persistence, Mini-Cypher, and CLI smoke workflows. It intentionally
does not treat Python, Node.js, C, vector search, full TCK compatibility, or
release-window gates as the main product surface.

## Install

For local development, use the workspace crate directly:

```toml
[dependencies]
nervusdb = { path = "nervusdb" }
nervusdb-query = { path = "nervusdb-query" }
```

Published package instructions should be updated only when the 0.1 release line
is cut.

## Open A Local Database

NervusDB derives two files from the path:

- `<path>.ndb`: page store
- `<path>.wal`: write-ahead log

```rust
use nervusdb::Db;

let db = Db::open("/tmp/nervusdb-demo")?;
let db = Db::open_paths("/tmp/nervusdb-demo.ndb", "/tmp/nervusdb-demo.wal")?;
```

Use `Db::snapshot()` for reads and `Db::begin_write()` for writes. There is one
writer at a time, with snapshot-style reads.

## Write Data

The stable 0.1 path is explicit: prepare a write statement, execute it against a
write transaction, then commit.

```rust
use nervusdb::Db;
use nervusdb_query::{prepare, Params};

let db = Db::open("/tmp/nervusdb-demo")?;
let snapshot = db.snapshot();
let create = prepare("CREATE (n:Person {name: 'Alice'})")?;

let mut txn = db.begin_write();
let count = create.execute_write(&snapshot, &mut txn, &Params::new())?;
txn.commit()?;

assert_eq!(count, 1);
```

For lower-level setup and tests, use `WriteTxn` directly:

```rust
use nervusdb::{Db, PropertyValue};

let db = Db::open("/tmp/nervusdb-demo")?;
let mut txn = db.begin_write();
let person = txn.get_or_create_label("Person")?;
let alice = txn.create_node(1, person)?;
txn.set_node_property(
    alice,
    "name".to_string(),
    PropertyValue::String("Alice".to_string()),
)?;
txn.commit()?;
```

## Query Data

Use `query_collect` for simple read queries:

```rust
use nervusdb::Db;
use nervusdb_query::{query_collect, Params};

let db = Db::open("/tmp/nervusdb-demo")?;
let rows = query_collect(
    &db.snapshot(),
    "MATCH (n:Person) RETURN n.name LIMIT 10",
    &Params::new(),
)?;

for row in rows {
    println!("{:?}", row.columns());
}
```

Keep 0.1 queries inside `docs/reference/mini-cypher.md`.

## Mini-Cypher 0.1

The supported 0.1 surface is deliberately small:

- `RETURN 1`
- `MATCH (n)`
- `MATCH (n:Label)`
- `MATCH (a)-[:TYPE]->(b)`
- simple property equality in `WHERE`
- `RETURN`
- `LIMIT`
- basic `CREATE`
- basic `DELETE`
- basic `SET` where already stable
- `EXPLAIN` for supported plans

These are frozen before 0.1: `OPTIONAL MATCH`, `WITH`, `UNION`, `UNWIND`,
aggregation, subqueries, procedures, pattern comprehension, broad
temporal/duration semantics, and full openCypher edge compatibility.

## CLI

The CLI is a local smoke/debug/import tool, not a separate platform surface.

```bash
cargo run -p nervusdb-cli -- v2 write \
  --db /tmp/nervusdb-demo \
  --cypher "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})"

cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/nervusdb-demo \
  --cypher "MATCH (a)-[:KNOWS]->(b) RETURN a.name, b.name LIMIT 10"
```

The CLI emits NDJSON for query rows and JSON for write counts.

## Ten 0.1 Examples

These are the examples that should stay runnable before 0.1:

| Example | Core graph shape | Query to prove |
|---------|------------------|----------------|
| Social graph | people and `KNOWS` edges | one-hop friend lookup |
| Dependency graph | packages and `DEPENDS_ON` edges | direct dependency lookup |
| File/module graph | files and `IMPORTS` edges | module fan-out |
| Tag graph | items and `TAGGED_AS` edges | label/property filter |
| Local knowledge graph | notes and `LINKS_TO` edges | nearby note traversal |
| Parent-child hierarchy | nodes and `PARENT_OF` edges | children lookup |
| Package relationship graph | crates and `USES` edges | dependency smoke |
| Ownership graph | owners and `OWNS` edges | asset lookup |
| Small recommendation traversal | user/item/category edges | two-hop candidate lookup |
| Import then query smoke | imported nodes/edges | write then read back |

The script entry points are:

```bash
bash scripts/core_smoke.sh
bash scripts/core_crash_recovery.sh
bash scripts/core_bench.sh --small
```

Large acceptance runs are manual:

```bash
bash scripts/core_bench.sh --large
```

Record hardware, command, data scale, and P50/P95/P99 output for large runs.

## Experimental And Maintenance Areas

These remain in the repository but are not the 0.1 quick path:

- Python, Node.js, and C bindings
- HNSW/vector search
- full openCypher TCK
- binding parity and examples-test gates
- backup/vacuum/compact/checkpoint APIs outside the embedded smoke loop
- fuzz, chaos, soak, performance, and release-window scripts

Run those checks manually only when touching their area.

## Validation

Default local validation is:

```bash
bash scripts/check.sh
```

This runs formatting, clippy, and workspace quick tests. See
`docs/runbooks/local-validation.md` for manual area-specific commands.

