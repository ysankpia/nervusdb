# API Surface

The 0.1 API is Rust-first. Public surface should make embedded local graph use
obvious and should not be shaped by old storage files or bindings before the
Rust core is credible.

The current user-facing API contract lives in `docs/reference/rust-api.md`. If a
public item is not listed as 0.1 core there, it is not a stability promise before
0.1.

## 0.1 Core API

Database lifecycle:

- `Db::open(path)` opens a local database directory.
- `Db::storage_dir()` or equivalent read-only path accessor may expose the
  directory path if needed.
- `Db::checkpoint()` persists committed graph state through the backend.
- `Db::close()` consumes the handle after a best-effort checkpoint.

Read path:

- `Db::snapshot`
- `Db::begin_read`
- `DbSnapshot` as the `GraphSnapshot` implementation returned by `Db::snapshot`
- `GraphSnapshot::nodes`
- `GraphSnapshot::nodes_with_label`
- `GraphSnapshot::neighbors`
- `GraphSnapshot::incoming_neighbors`
- `GraphSnapshot` property/name/count methods used by Mini-Cypher
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

- Mini-Cypher execution through `nervusdb::query`

## Removed From 0.1 Core

- `Db::open_paths(ndb_path, wal_path)`
- `Db::ndb_path`
- `Db::wal_path`
- `Db::compact`
- `Db::create_index`
- `GraphSnapshot::lookup_index`

The path APIs describe the old `.ndb + .wal` model. `compact` carried an old
storage-engine name that does not match the Fjall backend. The index hooks were
removed because property indexes are not part of the 0.1 query or storage
contract.

## Experimental Or Maintenance API

- backup, vacuum, and bulkload concepts
- binding-facing compatibility wrappers

Promoting any of these to core requires an ADR, updated docs, focused tests, and
validation policy updates.

## Contract Rule

New 0.1 work should use the core path:

```text
Db::open(directory)
  -> Db::begin_write / WriteTxn::commit
  -> Db::snapshot / GraphSnapshot
  -> direct traversal or Mini-Cypher
```
