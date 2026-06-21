# NervusDB 0.1 Scope

This document is the product boundary for the 0.1 refactor. New work must fit
this scope or explicitly isolate non-core work.

## In Scope

- Rust embedded API.
- Local database directory opened by `Db::open(path)`.
- Fjall-backed crash-safe committed persistence.
- Node, edge, label, relationship type, and property persistence.
- One writer and snapshot reads.
- Label scan through a storage-level label index.
- Neighbor traversal by relationship type.
- One-hop and two-hop traversal examples.
- Mini-Cypher for the supported 0.1 query surface.
- CLI smoke, debug, query, write, and import-style workflows.
- Reopen/crash smoke proving committed graph data survives.

## Storage Scope

0.1 storage is logical graph storage over Fjall keyspaces:

- `meta`
- `nodes`
- `ext2node`
- `labels`
- `reltypes`
- `node_labels`
- `label_nodes`
- `adj_out`
- `adj_in`
- `node_props`
- `edge_props`

The public storage path is a directory. Fjall's internal files are not a
NervusDB byte-level public format.

## Query Scope

Mini-Cypher 0.1 covers:

- constant `RETURN`
- node scan
- label scan
- directed one-hop traversal
- documented two-hop traversal
- simple property equality filters
- simple projection
- `LIMIT`
- basic `CREATE`
- basic `SET`
- basic `DELETE`
- `EXPLAIN` for supported plans

Code may temporarily accept more syntax. That behavior is experimental residue
unless `docs/reference/mini-cypher.md` promotes it.

## Out Of Scope Before 0.1

- `.ndb + .wal` as a public storage contract.
- Migration from old `.ndb/.wal` files.
- Independent edge IDs.
- Parallel edges.
- Property range indexes.
- Full openCypher compatibility.
- openCypher TCK pass rate as product success.
- Procedures, subqueries, pattern comprehension, and broad aggregation.
- `OPTIONAL MATCH`, `WITH`, `UNION`, `UNWIND`, `ORDER BY`, and `SKIP` as core
  gates.
- Stable Python, Node.js, or C APIs.
- Vector/HNSW as a default product path.
- Advanced optimizer work outside Mini-Cypher.
- Nightly chaos, soak, fuzz, perf, TCK, release, or stability windows as
  default gates.

## Acceptance

0.1 is credible when:

- A Rust program can create and reopen a local graph database directory.
- Nodes, edges, labels, relationship types, and properties persist across
  restart.
- Committed data survives crash/reopen smoke.
- One-hop and two-hop queries are documented and tested.
- Mini-Cypher results are deterministic for the supported surface.
- CLI smoke/write/query flows work against a local database directory.
- Rust API docs are clear enough to start without a server or non-Rust SDK.
- A manual large smoke can create 1,000,000 nodes and 5,000,000 edges without
  corruption on documented hardware.
