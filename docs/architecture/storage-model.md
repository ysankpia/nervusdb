# Storage Model

The storage layer is the foundation of the 0.1 product. Query language breadth
does not matter if committed graph data is lost or reopened incorrectly.

## Storage Direction

NervusDB 0.1 uses Fjall as the local persistent KV/LSM substrate. NervusDB owns
logical graph layout, encoding, API behavior, and validation. Fjall owns
low-level persistence mechanics.

The public storage path is a local database directory opened by `Db::open(path)`.
Fjall's internal files are implementation details and are not a public
NervusDB byte-level format.

## Logical Keyspaces

| Keyspace | Key | Value | Purpose |
|---|---|---|---|
| `meta` | metadata name | encoded scalar | format epoch and ID counters |
| `nodes` | `[iid]` | external id, flags | node existence and state |
| `ext2node` | `[external_id]` | iid | external-to-internal lookup |
| `labels` | `name/[name]`, `id/[label_id]` | id or name | label namespace |
| `reltypes` | `name/[name]`, `id/[rel_id]` | id or name | relationship type namespace |
| `node_labels` | `[iid][label_id]` | empty | labels attached to a node |
| `label_nodes` | `[label_id][iid]` | empty | storage-level label scan |
| `adj_out` | `[src][rel][dst]` | empty | outgoing traversal |
| `adj_in` | `[dst][rel][src]` | empty | incoming traversal |
| `node_props` | `[iid][key_len][key_bytes]` | encoded `PropertyValue` | node properties |
| `edge_props` | `[src][rel][dst][key_len][key_bytes]` | encoded `PropertyValue` | edge properties |
| `idx_node_props` | `[label_id][key_len][key_bytes][value_len][value_bytes][iid]` | empty | internal node property exact-match lookup |

Integer key parts use big-endian encoding so prefix scans preserve numeric
ordering. Property keys are stored as original UTF-8 bytes with length framing.
Property key hashes are not valid logical identity.

## Graph Identity

- Node identity is `InternalNodeId`.
- External node identity is `ExternalId` through `ext2node`.
- Edge identity is `(src_iid, rel_type_id, dst_iid)`.
- 0.1 has no independent edge ID.
- 0.1 has no parallel edges.
- Re-creating the same `(src, rel, dst)` edge is idempotent or overwrites the
  same logical edge; it must not create a second edge.
- Labels and relationship types are separate ID namespaces.

## Snapshot Semantics

Snapshots are immutable graph views. A snapshot created before a write commit
does not observe that commit. A new snapshot created after commit observes it.

Storage implementations may use Fjall snapshots, cloned immutable maps, or
other storage-owned mechanisms, but the public behavior is the `GraphSnapshot`
contract.

## Commit Semantics

Write transactions are serialized by the storage layer. A commit must make all
changes in the transaction visible to later snapshots as one unit. Dropping a
write transaction without commit discards its staged changes.

Fjall provides the low-level persistence and recovery mechanics. NervusDB tests
the graph-level outcome: committed graph data survives reopen, and incomplete
writes do not become visible through the public API.

## Delete Semantics

0.1 delete support is graph-level tombstone/delete behavior, not a public
compaction promise. Direct Rust API `tombstone_node(node)` uses detach-clean
semantics: it marks the node tombstoned and removes related node properties,
node label keys, label scan keys, incident adjacency keys, and incident edge
properties in the same commit batch.

Mini-Cypher keeps its user-facing distinction: plain `DELETE n` rejects
connected nodes, while `DETACH DELETE n` removes the node and relationships.

If a node is tombstoned, it must not be returned by `nodes()` or label scans,
and traversal must not expose edges whose endpoint is tombstoned. New writes
must reject dangling edges and mutations on missing or tombstoned graph
entities.

## Counts

Counts are API hints but must not lie for committed visible graph state. The
current Fjall backend computes counts from snapshot-visible keyspaces instead
of persisting count values in `meta`. If future versions add stored counters,
correctness still wins over speed: recompute or return conservative values
rather than exposing stale counts.

## Indexes

0.0.4 adds an internally maintained node property equality index for
`MATCH (n:Label) WHERE n.key = literal`. It is not a public schema feature and
does not restore `create_index` or `lookup_index`.

`idx_node_props` uses encoded `PropertyValue` bytes for exact equality only.
Those bytes are not an ordering contract, so range predicates such as
`n.age > 30` remain scan/filter behavior and are not index-backed.

Property range indexes, edge property indexes, composite indexes, and unique
constraints are still out of scope.

## Required Validation For Storage Changes

- Targeted storage tests for the changed graph invariant.
- Reopen-oriented tests.
- `bash scripts/core_crash_recovery.sh` when crash/reopen behavior can be
  affected.
- `cargo test -p nervusdb-storage --test core_0_1_storage` for storage backend
  behavior.
- No full workspace test by default; use it only for broad cross-workspace
  changes.
