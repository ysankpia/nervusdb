# Code Architecture

This document describes the current 0.1 code architecture after ADR 0005 and
the Fjall storage refactor. Pre-Fjall Pager/WAL/B+Tree/CSR details are historical
evidence only and live in archive/completed planning material.

Authoritative product boundaries remain:

- `docs/product/direction-contract.md`
- `docs/product/scope-0.1.md`
- `docs/architecture/overview.md`
- `docs/architecture/storage-model.md`
- `docs/architecture/query-model.md`
- `docs/reference/storage-format.md`

## Workspace Layout

```text
nervusdb/            public Rust crate: Db, api, storage, query
nervusdb/src/api.rs  graph traits and shared value types
nervusdb/src/storage Fjall-backed graph keyspaces and write transaction engine
nervusdb/src/query   Mini-Cypher parser, planner, and executor for 0.1
nervusdb-cli/        local debug, query, write, repl, and smoke workflows
```

`nervusdb-api/`, `nervusdb-storage/`, and `nervusdb-query/` are local
`publish = false` wrapper crates that re-export the implementation from
`nervusdb`. They are not separate public packages for the current line. Experimental binding
crates may remain in the repository, but they do not define the 0.1 embedded
graph core.

## Current Data Flow

```text
Rust API / CLI
  -> nervusdb facade
  -> direct write API or Mini-Cypher query API
  -> nervusdb::api traits
  -> nervusdb::storage Fjall keyspaces
```

`nervusdb::query` depends on `nervusdb::api`, not on `nervusdb::storage`.
`nervusdb::storage` implements `nervusdb::api` traits. The facade composes the
two. This boundary is intentional: query work must not reach into storage
implementation types.

## Storage Shape

The storage backend is a local database directory managed by Fjall. NervusDB does
not expose Fjall's internal files as a public byte-level format.

Logical keyspaces:

```text
meta
nodes
ext2node
labels
reltypes
node_labels
label_nodes
adj_out
adj_in
node_props
edge_props
```

Core rules:

- `Db::open(path)` opens a database directory.
- Old `.ndb + .wal` file pairs are not a public contract.
- Label and relationship type IDs are separate namespaces.
- Edge identity is `(src_iid, rel_type_id, dst_iid)`.
- Parallel edges are not a 0.1 feature.
- Property keys are stored as original strings, not hashes.
- Property indexes are not 0.1 core and have no public API hook before a future
  ADR defines them.

The storage contract is documented in `docs/architecture/storage-model.md` and
`docs/reference/storage-format.md`.

## Query Shape

The main query path implements Mini-Cypher 0.1 only:

```text
parse -> validate 0.1 surface -> compile supported plan -> execute over GraphSnapshot
```

Unsupported openCypher breadth must fail fast instead of remaining executable as
zombie behavior. This includes optional match, with/union/unwind, merge,
foreach/call/remove, aggregation, distinct, order/skip, named paths, and
variable-length paths unless a future ADR promotes one with tests and docs.

`MATCH (n:Label)` must use `GraphSnapshot::nodes_with_label(label_id)` so the
storage-level `label_nodes` keyspace matters in practice.

## Public Facade

0.1 core entry points:

- `Db::open(path)`
- `Db::storage_dir()`
- `Db::snapshot()`
- `Db::begin_read()`
- `Db::begin_write()`
- `WriteTxn::commit()`
- `GraphSnapshot` traversal, label, property, and count methods
- `nervusdb::query::prepare`
- `nervusdb::query::query_collect`

`checkpoint` and `close` are lifecycle helpers over Fjall persistence. The old
`compact` name and property-index hooks were removed from the 0.1 public API.

## Do Not Reintroduce

- self-built Pager/WAL/B+Tree/CSR storage under new names
- `.ndb/.wal` compatibility as a hidden constraint
- query-to-storage direct dependencies
- hashed property keys as logical identity
- shared label/relationship type ID space
- public no-op property index hooks
- old compaction naming for Fjall persistence
- executable advanced query paths outside the Mini-Cypher 0.1 contract
- full openCypher TCK pass rate as a 0.1 success metric

## Validation

Default local validation:

```bash
bash scripts/check.sh
```

Storage/query readiness evidence:

```bash
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_mini_cypher
cargo test -p nervusdb --test core_0_1_examples
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
```

Full workspace validation is manual:

```bash
cargo test --workspace
```
