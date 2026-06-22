# ADR 0009: Storage Keyspace Consolidation

## Status

Accepted for 0.0.7.

## Context

0.0.6 removed the obvious storage hot-path bugs. The remaining medium benchmark
costs are dominated by durable `batch.commit`, raw database reopen, file count,
and storage footprint. The current physical layout spreads one logical graph
over many Fjall keyspaces, which increases the number of active LSM structures
Fjall must open, flush, and recover.

NervusDB is still pre-0.1. There is no stable on-disk compatibility promise and
there are few users. Carrying an ugly migration path now would optimize for the
wrong thing.

## Decision

0.0.7 changes the logical storage epoch from `2` to `3` and consolidates the
physical Fjall layout to four keyspaces:

```text
meta
graph_data
adj_out
adj_in
```

`meta` stores only `format_epoch` and ID counters. `graph_data` stores every
non-adjacency graph record and derived index using a one-byte logical tag
followed by the existing big-endian key parts. `adj_out` and `adj_in` remain
separate hot traversal keyspaces because the first two-keyspace cut reduced file
count and clean reopen but regressed two-hop traversal.

```text
0x01 NODE             [tag][iid:u32] -> encode_node_value(external_id, flags)
0x02 EXT2NODE         [tag][external_id:u64] -> iid:u32
0x10 LABEL_NAME       [tag][name_len:u16][name_bytes] -> label_id:u32
0x11 LABEL_ID         [tag][label_id:u32] -> name_bytes
0x12 REL_NAME         [tag][name_len:u16][name_bytes] -> rel_id:u32
0x13 REL_ID           [tag][rel_id:u32] -> name_bytes
0x20 NODE_LABEL       [tag][iid:u32][label_id:u32] -> empty
0x21 LABEL_NODE       [tag][label_id:u32][iid:u32] -> empty
0x40 NODE_PROP        [tag][iid:u32][key_len:u32][key_bytes] -> PropertyValue::encode()
0x41 EDGE_PROP        [tag][src:u32][rel:u32][dst:u32][key_len:u32][key_bytes] -> PropertyValue::encode()
0x50 NODE_PROP_INDEX  [tag][label_id:u32][key_len:u16][key_bytes][value_len:u32][value_bytes][iid:u32] -> empty
```

Adjacency keyspaces keep 12-byte raw keys:

```text
adj_out [src:u32][rel:u32][dst:u32] -> empty
adj_in  [dst:u32][rel:u32][src:u32] -> empty
```

Opening an epoch 2 database returns `StorageFormatMismatch`. No migration tool
is provided for 0.0.7. Users must recreate, export/reimport, or rebuild the
database.

## Consequences

The good part: Fjall has far fewer physical keyspaces to open, recover, flush,
and sync than the epoch 2 layout. The graph layout is still simple: cold/general
graph records live in one tagged keyspace, while the two hottest traversal
ranges keep dedicated LSM locality.

The cost: 0.0.6 database directories are not readable by 0.0.7. This is an
intentional pre-0.1 break. Long-term disk compatibility remains out of scope
until the 0.1 storage contract is frozen.

`fsck-lite` must scan `graph_data` by logical tag and `adj_out` / `adj_in`
directly. Repair remains conservative: it rebuilds only derived `LABEL_NODE` and
`NODE_PROP_INDEX` records from canonical nodes, node labels, and node
properties. It must not delete user graph data.

## Non-Goals

- No SQLite-style single-file storage.
- No public storage migration API.
- No public `DbOptions`, bulk loader, or unsafe durability switch.
- No EdgeId or parallel edges.
- No vector/HNSW, range index, multi-writer, or full Cypher work.

## Validation

Minimum correctness evidence:

```bash
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --features unstable-admin admin::tests
bash scripts/core_crash_recovery.sh
```

Minimum performance evidence before marking 0.0.7 complete:

```bash
bash scripts/cross_db_bench.sh --system nervusdb --medium
NERVUSDB_PROFILE_STORAGE=1 bash scripts/cross_db_bench.sh --system nervusdb --medium
```

0.0.7 should not be released as solved unless reopen, commit, and file-count
evidence shows the consolidation was worth the destructive format break.
