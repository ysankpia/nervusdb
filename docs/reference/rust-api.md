# Rust API 0.1 Reference

The Rust facade is the primary 0.1 API. It should let an embedded Rust program
open a local graph database directory, write graph data, read snapshots,
traverse neighbors, and run Mini-Cypher without touching storage internals or
non-Rust bindings.

If an API is not listed as 0.1 core here, it is not a stability promise before
0.1. Existing public items may remain callable for maintenance or experiments,
but they must not drive new 0.1 scope.

## 0.1 Core API

Database lifecycle:

- `Db::open(path)` opens a local database directory.
- `Db::storage_dir()` or equivalent path accessor may expose that directory.

Read path:

- `Db::snapshot`
- `Db::begin_read`
- `DbSnapshot` as the `GraphSnapshot` implementation returned by `Db::snapshot`
- `GraphSnapshot::nodes`
- `GraphSnapshot::nodes_with_label`
- `GraphSnapshot::neighbors`
- `GraphSnapshot::incoming_neighbors`
- `ReadTxn::neighbors`

Write path:

- `Db::begin_write`
- `WriteTxn::get_or_create_label`
- `WriteTxn::get_or_create_rel_type`
- `WriteTxn::create_node`
- `WriteTxn::add_node_label`
- `WriteTxn::remove_node_label`
- `WriteTxn::create_edge`
- `WriteTxn::set_node_property`
- `WriteTxn::set_edge_property`
- `WriteTxn::remove_node_property`
- `WriteTxn::remove_edge_property`
- `WriteTxn::tombstone_node`
- `WriteTxn::tombstone_edge`
- `WriteTxn::commit`

Query path:

- `nervusdb::query` re-export for Mini-Cypher
- `nervusdb_query::prepare`
- `nervusdb_query::query_collect`

## Removed From 0.1 Core

- `Db::open_paths`
- `Db::ndb_path`
- `Db::wal_path`

These belong to the old `.ndb + .wal` model. New code should use directory
paths.

## Experimental Or Maintenance API

The following APIs are not part of the 0.1 core stability promise:

- `Db::compact`
- `Db::checkpoint`
- `Db::close`
- `Db::create_index`
- `GraphSnapshot::lookup_index`
- backup, vacuum, and bulkload concepts
- binding-facing compatibility wrappers

They can remain available before 0.1 for maintenance and manual experiments.
Promoting any of them to the core API requires an ADR, updated docs, focused
tests, and validation policy updates.

## Expected 0.1 Usage Shape

```rust
use nervusdb::{Db, GraphSnapshot, PropertyValue};

let db = Db::open("/tmp/example-graph")?;

let mut txn = db.begin_write();
let person = txn.get_or_create_label("Person")?;
let knows = txn.get_or_create_rel_type("KNOWS")?;
let alice = txn.create_node(1, person)?;
let bob = txn.create_node(2, person)?;
txn.set_node_property(
    alice,
    "name".to_string(),
    PropertyValue::String("Alice".to_string()),
)?;
txn.create_edge(alice, knows, bob)?;
txn.commit()?;

let snapshot = db.snapshot();
let people: Vec<_> = snapshot.nodes_with_label(person).collect();
let outgoing: Vec<_> = snapshot.neighbors(alice, Some(knows)).collect();
assert_eq!(people.len(), 2);
assert_eq!(outgoing.len(), 1);
# Ok::<(), nervusdb::Error>(())
```

The example shows the direct Rust facade path. Mini-Cypher is also a 0.1 query
path, but its supported syntax is defined separately in
`docs/reference/mini-cypher.md`.
