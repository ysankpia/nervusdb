# ADR 0005: Fjall Storage Backend For 0.1

## Status

Accepted.

## Context

NervusDB is trying to become SQLite for property graphs: an embedded Rust graph
database with local persistence, crash-safe committed writes, labels,
properties, neighbor traversal, and a small query surface.

The current storage direction still carries a self-built stack:

- pager
- B+Tree
- write-ahead log
- CSR segments
- L0 runs
- overlay/read-path merge logic
- page-level property and index stores

That stack is too much surface for the current project stage. The project is
pre-0.1 and has no storage compatibility promise. The hard part for 0.1 is not
proving NervusDB can reimplement a general KV engine. The hard part is proving
the graph model, Rust facade, Mini-Cypher path, and local durability contract
are coherent and testable.

Recent code and docs also drifted:

- product docs still describe `.ndb + .wal` as the user-visible contract
- `nervusdb-query` depends directly on `nervusdb-storage`
- label and relationship type IDs are not cleanly separated in the old design
- core tests have started to pull advanced Cypher/index behavior back into 0.1
- old storage implementation details still shape public API wording

## Decision

NervusDB 0.1 will use Fjall as the persistent local KV/LSM substrate.

The public 0.1 storage contract becomes a local database directory opened by
`Db::open(path)`. Fjall's internal files are implementation details, not a
NervusDB byte-level public format.

The old self-built storage stack is replaced by a logical graph keyspace model:

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

The 0.1 graph model is:

- node identity: `InternalNodeId`
- external node identity: `ExternalId`
- edge identity: `(src_iid, rel_type_id, dst_iid)`
- no independent edge ID in 0.1
- no parallel edges in 0.1
- label IDs and relationship type IDs are separate namespaces
- property keys are stored as original UTF-8 strings with length framing
- property keys are not hashed as logical identity
- property range indexes are not part of 0.1

`nervusdb-api` is the boundary between query and storage. `nervusdb-query` must
not depend on `nervusdb-storage`.

## Replaced

The following implementation families are no longer the current architecture:

- pager
- page allocator
- page-level B+Tree
- NervusDB-owned WAL format
- CSR segment persistence
- L0 run publication
- overlay/read-path merge
- property sinking into a page B+Tree
- index catalog/backfill as a 0.1 core path

Git history remains the archive. Current docs must not use those components to
describe the 0.1 storage architecture.

## Rejected Alternatives

### Continue repairing the self-built storage engine

Rejected. It optimizes an intermediate artifact instead of the product goal.
The 0.1 goal is an embedded graph database, not a general-purpose storage
engine project.

### Simulate the old `.ndb + .wal` model on top of Fjall

Rejected. That would preserve a wrong abstraction and create two storage
contracts: Fjall internally and a fake old file model externally.

### Use hashed property keys

Rejected. Hashing property keys destroys prefix/range semantics and creates
collision-driven correctness failures. A database cannot accept silent logical
identity collisions.

### Add `eid` now

Rejected. Edge identity `(src, rel, dst)` is enough for 0.1. Independent edge IDs
and parallel edges can be designed later if real use cases demand them.

### Promote property indexes into 0.1

Rejected. Equality/range property indexes need a separate contract around value
ordering, update/delete cleanup, and planner use. They are not required for the
0.1 embedded graph core.

## Consequences

- Old `.ndb/.wal` data is not migrated.
- `Db::open(path)` opens a directory.
- `open_paths(ndb, wal)`, `ndb_path()`, and `wal_path()` are not 0.1 core APIs.
- `Db::compact()` and `Db::checkpoint()` are maintenance compatibility only; the
  Fjall backend does not expose NervusDB-managed page compaction.
- `Db::create_index()` and `GraphSnapshot::lookup_index()` are not 0.1 core
  promises.
- Storage validation focuses on graph semantics: reopen, label scan, traversal,
  properties, snapshot isolation, and crash/reopen smoke.

## Required Validation

The Fjall refactor is not done until these checks pass or a blocker records
exact failure evidence:

- create nodes, edges, labels, relationship types, and properties
- drop/reopen the database directory and read them back
- `MATCH (n:Label)` uses label-keyspace access, not only full node scan
- outgoing and incoming traversal are correct with relationship type filters
- node and edge properties round-trip
- old snapshots do not observe writes committed after the snapshot
- process-level crash/reopen smoke succeeds for committed writes
- `nervusdb-query` has no dependency on `nervusdb-storage`

## Follow-Up Documents

This ADR is implemented through:

- `docs/plans/active/010-fjall-storage-refactor.md`
- `docs/architecture/storage-model.md`
- `docs/reference/storage-format.md`
- `docs/product/direction-contract.md`
- `docs/product/scope-0.1.md`
- `docs/engineering/architecture-invariants.md`
- `docs/engineering/dependency-policy.md`
