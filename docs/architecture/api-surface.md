# API Surface

The 0.1 API is Rust-first. Public surface should make embedded local graph use
obvious and should not be shaped by bindings before the Rust core is credible.

The current user-facing API contract lives in `docs/reference/rust-api.md`. If a
public item is not listed as 0.1 core there, it is not a stability promise before
0.1.

## 0.1 Core API

- `Db::open`
- `Db::open_paths`
- `Db::ndb_path`
- `Db::wal_path`
- `Db::snapshot`
- `Db::begin_read`
- `Db::begin_write`
- `ReadTxn::neighbors`
- `WriteTxn::get_or_create_label`
- `WriteTxn::get_or_create_rel_type`
- `WriteTxn::create_node`
- `WriteTxn::create_edge`
- `WriteTxn::set_node_property`
- `WriteTxn::set_edge_property`
- `WriteTxn::remove_node_property`
- `WriteTxn::remove_edge_property`
- `WriteTxn::tombstone_node`
- `WriteTxn::tombstone_edge`
- `WriteTxn::commit`
- `DbSnapshot` as the `GraphSnapshot` implementation returned by `Db::snapshot`
- Mini-Cypher execution through `nervusdb::query` or `nervusdb_query`

## Experimental Or Maintenance API

- `Db::compact`
- `Db::checkpoint`
- `Db::close`
- `Db::create_index`
- `Db::search_vector`
- `vacuum`
- `backup`
- `bulkload`
- backup, vacuum, and bulkload exported types
- `WriteTxn::set_vector`
- binding-facing compatibility wrappers

Do not remove these in Phase 005. Classify first, then decide later whether to
feature-gate, hide from docs, or move modules.

## Contract Rule

Phase 005 does not introduce breaking removals. It also does not make
experimental or maintenance APIs stable. New 0.1 work should use the core path:

```text
Db::open
  -> Db::begin_write / WriteTxn::commit
  -> Db::snapshot / GraphSnapshot
  -> direct traversal or Mini-Cypher
```
