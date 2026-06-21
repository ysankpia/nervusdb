# 014 Graph Integrity 0.0.3

## Status

Active.

## Goal

Make 0.0.3 a graph integrity release. The storage layer must reject invalid
graph writes and clean related keyspaces when nodes or edges are deleted.

0.0.3 is not a feature expansion release. It does not add property indexes,
edge IDs, parallel edges, advanced Cypher, vectors, or multi-writer concurrency.

## Scope

- Reject dangling edges: `create_edge(src, rel, dst)` requires live endpoints
  or endpoints created in the same transaction.
- Reject mutations on missing or tombstoned nodes and missing edges.
- Make direct Rust API `tombstone_node(node)` detach-clean related graph state.
- Clean node labels, label scan keys, node properties, adjacency keys, and edge
  properties in the same commit batch.
- Change low-level mutating write methods to return `Result<()>` where errors
  can occur.
- Keep edge identity `(src, rel, dst)` and duplicate-edge idempotence.

## Not In Scope

- Property equality indexes or range indexes.
- Public `create_index` / `lookup_index` hooks.
- Independent edge IDs or parallel edges.
- Buffered or unsafe durability mode.
- More Mini-Cypher syntax.
- Vector or HNSW features.
- Multi-writer OCC.

## Acceptance

- Storage tests prove dangling edge rejection, delete cleanup, property cleanup,
  reopen consistency, and duplicate-edge idempotence.
- Query tests prove `DELETE n` still rejects connected nodes unless `DETACH`
  is used, and `DETACH DELETE n` removes the node plus relationships.
- Rust API examples compile with `tombstone_node(...)?` and
  `tombstone_edge(...)?`.
- `cargo fmt --all -- --check`
- `cargo check -p nervusdb --examples`
- `cargo test -p nervusdb-storage --test core_0_1_storage`
- `cargo test -p nervusdb --test core_0_1_rust_api`
- `cargo test -p nervusdb --test core_0_1_mini_cypher`
- `cargo test -p nervusdb --test core_0_1_examples`
- `bash scripts/check.sh`
- `bash scripts/core_crash_recovery.sh`
- `cargo test --workspace` before release.
