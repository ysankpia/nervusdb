# ADR 0008: Stability Freeze And Fsck-Lite

## Status

Accepted.

## Context

NervusDB `v0.0.4` has a Fjall-backed storage core, graph integrity checks,
Mini-Cypher 0.1 behavior, and an internally maintained node property equality
index. The next risk is not missing database features. The next risk is staying
in database-building mode instead of using NervusDB as a dependency for other
projects.

The only remaining hardening work worth doing before that shift is operational:
verify that derived keyspaces can be audited and rebuilt when a bug, interrupted
experiment, or future migration leaves index state stale.

## Decision

`v0.0.5` is a stability-freeze release. It adds an offline fsck-lite tool and
then treats NervusDB as ready for downstream project use.

The management surface is intentionally narrow:

- CLI entry: `nervusdb v2 fsck`.
- Rust surface: `nervusdb::admin` behind feature `unstable-admin`.
- Repair scope: rebuild derived indexes only.
- No public index-management API is introduced.

Repair may rebuild `label_nodes` and `idx_node_props` from canonical data:
`nodes`, `node_labels`, and `node_props`. It must not delete canonical graph
data such as nodes, properties, adjacency, or edge properties. Non-derived
corruption is reported, not guessed away.

## Non-Goals

- Range indexes.
- Public `create_index` / `lookup_index`.
- Edge IDs or parallel edges.
- Vector/HNSW search.
- Full Cypher.
- Multi-writer concurrency.
- Long-term storage-format compatibility promises before 0.1.

## Consequences

Fsck-lite is allowed to inspect storage internals. That is why it is not part of
the stable 0.1 Rust API. The CLI can use it for local operations, and downstream
projects can rely on the CLI as the supported management entry.

After `v0.0.5`, new database work should be driven by real downstream blockers,
not speculative feature roadmaps.
