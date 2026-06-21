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

Fjall's own internal versioning is separate. NervusDB docs do not promise a
stable byte layout for Fjall files.

## Key Encoding

Integer key parts are big-endian. String key parts are UTF-8 bytes with explicit
length framing where needed to avoid ambiguous concatenation.

Property keys are stored as original strings. They are not hashed. Hashing
property keys would break prefix/range semantics and introduce collision-driven
wrong results.

## Logical Keyspaces

```text
meta        named metadata
nodes       [iid] -> external id, flags
ext2node    [external_id] -> iid
labels      name/[name] <-> id/[label_id]
reltypes    name/[name] <-> id/[rel_type_id]
node_labels [iid][label_id] -> empty
label_nodes [label_id][iid] -> empty
adj_out     [src][rel][dst] -> empty
adj_in      [dst][rel][src] -> empty
node_props  [iid][key_len][key_bytes] -> encoded PropertyValue
edge_props  [src][rel][dst][key_len][key_bytes] -> encoded PropertyValue
```

## Value Encoding

`PropertyValue` encoding is owned by `nervusdb::api`. Storage must not create a
second public property-value type. Any encoding change that affects persisted
values requires a format epoch decision and compatibility handling.

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
- Property index formats and planner integration.

Changes here require storage-model docs and crash/reopen validation.
