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

## Physical Keyspaces

0.0.7 collapses the graph from many Fjall keyspaces into four physical
keyspaces:

| Keyspace | Purpose |
|---|---|
| `meta` | format epoch and ID counters |
| `graph_data` | tagged graph records, names, properties, and derived indexes |
| `adj_out` | outgoing adjacency keys |
| `adj_in` | incoming adjacency keys |

The old epoch 2 layout used separate physical keyspaces for `nodes`,
`ext2node`, `labels`, `reltypes`, `node_labels`, `label_nodes`, `adj_out`,
`adj_in`, `node_props`, `edge_props`, and `idx_node_props`. Epoch 3 rejects
epoch 2 directories with `StorageFormatMismatch` instead of migrating them.

## Tagged Graph Data

| Tag | Logical partition | Key | Value | Purpose |
|---:|---|---|---|---|
| `0x01` | `NODE` | `[tag][iid]` | external id, flags | node existence and state |
| `0x02` | `EXT2NODE` | `[tag][external_id]` | iid | external-to-internal lookup |
| `0x10` | `LABEL_NAME` | `[tag][name_len][name]` | label id | label name lookup |
| `0x11` | `LABEL_ID` | `[tag][label_id]` | name | label id lookup |
| `0x12` | `REL_NAME` | `[tag][name_len][name]` | rel id | relationship type name lookup |
| `0x13` | `REL_ID` | `[tag][rel_id]` | name | relationship type id lookup |
| `0x20` | `NODE_LABEL` | `[tag][iid][label_id]` | empty | labels attached to a node |
| `0x21` | `LABEL_NODE` | `[tag][label_id][iid]` | empty | storage-level label scan |
| `0x40` | `NODE_PROP` | `[tag][iid][key_len][key]` | encoded `PropertyValue` | node properties |
| `0x41` | `EDGE_PROP` | `[tag][src][rel][dst][key_len][key]` | encoded `PropertyValue` | edge properties |
| `0x50` | `NODE_PROP_INDEX` | `[tag][label_id][key_len][key][value_len][value][iid]` | empty | internal node property exact-match lookup |

Adjacency keyspaces use raw big-endian keys for hot traversal locality:

| Keyspace | Key | Value | Purpose |
|---|---|---|---|
| `adj_out` | `[src][rel][dst]` | empty | outgoing traversal |
| `adj_in` | `[dst][rel][src]` | empty | incoming traversal |

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

The logical `NODE_PROP_INDEX` partition uses encoded `PropertyValue` bytes for
exact equality only. Those bytes are not an ordering contract, so range
predicates such as `n.age > 30` remain scan/filter behavior and are not
index-backed.

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
