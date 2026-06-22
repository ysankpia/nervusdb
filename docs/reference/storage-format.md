# Storage Format Reference

This reference records the 0.1 storage contract. It is a logical graph storage
contract, not a byte-level description of Fjall's internal files.

## Public Path

`Db::open(path)` opens a local database directory. The directory is managed by
the storage backend. Callers must not depend on internal file names created by
Fjall.

Old `.ndb + .wal` file pairs are not part of the current public 0.1 contract,
and old data is not migrated by this refactor.

## Versioning

NervusDB stores a logical format epoch in the `meta` keyspace. Incompatible
logical format changes must bump the epoch and fail fast with a clear
compatibility error when the on-disk graph contract cannot be interpreted
safely.

Current development epoch:

```text
STORAGE_FORMAT_EPOCH = 3
```

Epoch 3 is a destructive 0.0.7 storage-layout change. Epoch 2 database
directories are rejected with `StorageFormatMismatch`; there is no migration
tool in 0.0.7.

Fjall's own internal versioning is separate. NervusDB docs do not promise a
stable byte layout for Fjall files.

## Key Encoding

Integer key parts are big-endian. String key parts are UTF-8 bytes with explicit
length framing where needed to avoid ambiguous concatenation.

Property keys are stored as original strings. They are not hashed. Hashing
property keys would break prefix/range semantics and introduce collision-driven
wrong results.

## Physical Keyspaces

```text
meta        format epoch and ID counters
graph_data  tagged non-adjacency graph records and derived indexes
adj_out     outgoing adjacency keys
adj_in      incoming adjacency keys
```

## Tagged `graph_data` Layout

```text
0x01 NODE             [tag][iid:u32] -> encode_node_value(external_id, flags)
0x02 EXT2NODE         [tag][external_id:u64] -> iid:u32

0x10 LABEL_NAME       [tag][name_len:u16][name_bytes] -> label_id:u32
0x11 LABEL_ID         [tag][label_id:u32] -> name_bytes
0x12 REL_NAME         [tag][name_len:u16][name_bytes] -> rel_id:u32
0x13 REL_ID           [tag][rel_id:u32] -> name_bytes

0x20 NODE_LABEL       [tag][iid:u32][label_id:u32] -> empty
0x21 LABEL_NODE       [tag][label_id:u32][iid:u32] -> empty

0x40 NODE_PROP        [tag][iid:u32][key_len:u32][key_bytes] -> encoded PropertyValue
0x41 EDGE_PROP        [tag][src:u32][rel:u32][dst:u32][key_len:u32][key_bytes] -> encoded PropertyValue

0x50 NODE_PROP_INDEX  [tag][label_id:u32][key_len:u16][key_bytes][value_len:u32][value_bytes][iid:u32] -> empty
```

## Adjacency Keyspaces

```text
adj_out [src:u32][rel:u32][dst:u32] -> empty
adj_in  [dst:u32][rel:u32][src:u32] -> empty
```

Prefix scan contracts:

```text
nodes()                          prefix [NODE]
node_labels(iid)                 prefix [NODE_LABEL][iid]
nodes_with_label(label)          prefix [LABEL_NODE][label]
neighbors(src, None)             adj_out prefix [src]
neighbors(src, Some(rel))        adj_out prefix [src][rel]
incoming_neighbors(dst, None)    adj_in prefix [dst]
incoming_neighbors(dst, Some(r)) adj_in prefix [dst][r]
node_properties(iid)             prefix [NODE_PROP][iid]
edge_properties(edge)            prefix [EDGE_PROP][src][rel][dst]
property equality lookup         prefix [NODE_PROP_INDEX][label][key][value]
```

## Value Encoding

`PropertyValue` encoding is owned by `nervusdb::api`. Storage must not create a
second public property-value type. Any encoding change that affects persisted
values requires a format epoch decision and compatibility handling.

`idx_node_props` reuses `PropertyValue::encode()` only as exact-match identity.
It does not define range ordering for encoded values.

In epoch 3, `idx_node_props` is the logical `NODE_PROP_INDEX` tag inside
`graph_data`; it is not a separate physical Fjall keyspace.

## Recovery Assumptions

- Committed writes survive process failure and reopen.
- Uncommitted or partial writes do not become visible after recovery.
- Recovery preserves nodes, edges, labels, relationship types, and properties.
- Recovery errors surface as errors, not ignored state.

The recovery proof is graph-level. Tests verify what users can observe through
`Db`, `GraphSnapshot`, Mini-Cypher, and CLI.

## Not Stable Yet

- Long-term cross-version compatibility policy.
- Byte-level guarantees for backend files.
- Backup, vacuum, and backend compaction behavior as user-facing 0.1 promises.
- Range index formats and public index-management APIs.
- Cross-version on-disk migration from epoch 2 to epoch 3.

Changes here require storage-model docs and crash/reopen validation.
