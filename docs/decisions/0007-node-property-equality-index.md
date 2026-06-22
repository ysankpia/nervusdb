# ADR 0007: Internal Node Property Equality Index

## Status

Accepted.

## Context

NervusDB 0.0.3 made graph writes safer: dangling edges are rejected, invalid
mutations fail, and node deletion cleans related graph keyspaces. The next
practical bottleneck is query anchoring. `MATCH (n:Label) WHERE n.key = 'value'`
can be correct today, but it still has to scan label nodes and filter
properties.

The old public `create_index` / `lookup_index` hooks were removed because they
advertised an index-management contract before storage maintenance, delete
cleanup, and planner behavior existed. Reintroducing that API now would recreate
the same false surface.

## Decision

0.0.4 adds an internally maintained node property equality index. It is not a
user-managed schema feature.

The storage layer adds logical keyspace `idx_node_props`:

```text
[label_id: u32 BE]
[key_len: u16 BE]
[key_bytes]
[value_len: u32 BE]
[value_bytes: PropertyValue::encode()]
[iid: u32 BE]
```

The value is empty. The index is only for exact equality. `PropertyValue` bytes
are reused for equality identity and do not define ordering or range semantics.

The API boundary adds a storage-neutral `GraphSnapshot` method for
`nodes_with_label_and_property`. The default implementation filters
`nodes_with_label`; the Fjall-backed implementation scans `idx_node_props`.

## Scope

- Support node property exact equality for scalar values.
- Support query anchoring for `MATCH (n:Label) WHERE n.key = literal` and
  inline node properties such as `MATCH (n:Label {key: literal})`.
- Keep remaining filters in place as a correctness guard.
- Maintain index entries atomically in the same Fjall batch as graph writes.
- Preserve original property key strings. Do not hash property keys.

## Non-Goals

- Public `Db::create_index`, `GraphSnapshot::lookup_index`, or index-management
  API.
- Range indexes or ordered value semantics.
- Edge property indexes.
- Composite indexes.
- Unique constraints.
- Parameter-based index planning.
- Edge IDs, parallel edges, vectors, HNSW, multi-writer OCC, or broader Cypher.

## Consequences

Writes now pay extra maintenance cost for indexed node properties, labels, and
node deletion. That cost is acceptable for 0.0.4 only if correctness holds and
the 100k-node exact lookup benchmark is at least 10x faster than the scan
baseline.

If the index becomes stale, query results become wrong. Therefore index
maintenance tests are release blockers, not optional performance tests.
