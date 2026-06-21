# 010 Fjall Storage Refactor

## Objective

Replace the current self-built Pager/WAL/B+Tree/CSR storage direction with a
Fjall-backed local directory backend, while resetting the 0.1 contract around a
small embedded Rust property graph core.

The refactor is intentionally destructive. The project is pre-0.1 and does not
promise storage compatibility for old `.ndb/.wal` data.

## Contract

The current 0.1 contract is:

```text
Rust API / CLI
  -> Mini-Cypher or direct graph API
  -> nervusdb-api traits
  -> storage adapter
  -> Fjall keyspaces in a local database directory
```

NervusDB owns graph semantics. Fjall owns low-level persistence mechanics.

## Keep

- Rust facade crate
- `nervusdb-api` shared IDs, `PropertyValue`, `GraphSnapshot`, `GraphStore`
- Mini-Cypher parser/planner/executor for the documented 0.1 surface
- CLI smoke/debug/query/write workflows
- focused core tests that prove graph behavior

## Delete Or Replace

- pager
- WAL format and replay code
- page B+Tree
- CSR segments
- L0 run publication and overlay merge
- old property sinking paths
- old storage format tests
- index catalog/backfill as a core path
- `.ndb + .wal` public path semantics

## Data Model Decisions

- Nodes use `InternalNodeId`.
- External IDs map through `ext2node`.
- Edges are identified by `(src_iid, rel_type_id, dst_iid)`.
- 0.1 has no independent edge ID.
- 0.1 has no parallel edges.
- Labels and relationship types use separate namespaces.
- Property keys are original UTF-8 strings with length framing.
- Property keys are not hashed.
- Property range indexes are not in 0.1.

## Fjall Keyspaces

| Keyspace | Key | Value |
|---|---|---|
| `meta` | named metadata key | format epoch and ID counters |
| `nodes` | `[iid]` | external id and flags |
| `ext2node` | `[external_id]` | iid |
| `labels` | `name/[name]`, `id/[label_id]` | label id or name |
| `reltypes` | `name/[name]`, `id/[rel_id]` | rel id or name |
| `node_labels` | `[iid][label_id]` | empty |
| `label_nodes` | `[label_id][iid]` | empty |
| `adj_out` | `[src][rel][dst]` | empty |
| `adj_in` | `[dst][rel][src]` | empty |
| `node_props` | `[iid][key_len][key_bytes]` | encoded `PropertyValue` |
| `edge_props` | `[src][rel][dst][key_len][key_bytes]` | encoded `PropertyValue` |

Integer key parts use big-endian encoding for prefix scan ordering.

## Stages

Status as of 2026-06-21: D0-D5 are complete in the current working tree.

### D0: Documentation Contract

Land this plan and ADR 0005. Update product, architecture, reference,
engineering, roadmap, progress, and debt docs so the current reading path
describes Fjall-backed directory storage.

Done when:

- ADR 0005 exists
- this active plan exists
- docs no longer present `.ndb + .wal` as the 0.1 public contract
- docs state no `eid`, no parallel edges, no hashed property keys
- docs state property indexes are outside 0.1 core

Result: complete. ADR 0005 and this active plan exist; product, architecture,
reference, engineering, roadmap, progress, and debt docs now describe the Fjall
directory-storage contract.

### D1: Query/Storage Boundary

Remove the direct `nervusdb-query -> nervusdb-storage` dependency.

Done when:

- `nervusdb-query/Cargo.toml` has no storage dependency
- `WriteableGraph` is storage-neutral
- `PropertyValue` is sourced from `nervusdb-api`
- `GraphSnapshot` exposes `nodes_with_label(label_id)`
- label scans use `nodes_with_label`

Result: complete. `WriteableGraph` moved to `nervusdb-api`; `PropertyValue` is
the API type; `nervusdb-query` has no dependency on `nervusdb-storage`; label
scans call `GraphSnapshot::nodes_with_label`.

### D2: Fjall Backend

Implement the Fjall keyspace backend for core graph operations.

Done when:

- storage opens a directory
- committed writes persist across reopen
- labels, relationship types, nodes, edges, properties, and visible counts work
- snapshots are immutable views
- old compaction/run merge paths are disabled or removed from current build

Result: complete. `nervusdb-storage` now contains Fjall-backed keyspaces for
meta, nodes, ext2node, labels, reltypes, node labels, label nodes, adjacency,
node properties, and edge properties.

### D3: Facade And CLI Path Semantics

Make the public facade and CLI use directory paths.

Done when:

- `Db::open(path)` opens a database directory
- examples and tests stop relying on `.ndb/.wal`
- `open_paths`, `ndb_path`, and `wal_path` are removed or non-core
- docs and rustdoc match the implementation

Result: complete. `Db::open(path)` opens a local database directory and the
facade exposes `storage_dir()`. The old `open_paths`, `ndb_path`, and
`wal_path` facade methods were removed.

### D4: Old Storage Deletion And Query Scope Cleanup

Remove old storage modules from the current build and shrink core query gates.

Done when:

- pager/WAL/B+Tree/CSR code is not compiled
- core tests match `docs/reference/mini-cypher.md`
- `OPTIONAL MATCH`, aggregation, `ORDER BY/SKIP`, and index backfill are not
  required for 0.1 validation

Result: complete. Pager, WAL, B+Tree, CSR, L0 run, overlay, old read-path, and
old storage examples were deleted from `nervusdb-storage`. Core tests and
examples no longer require `OPTIONAL MATCH`, aggregation, `ORDER BY/SKIP`, or
index backfill.

### D5: Validation

Run focused checks and the default repository check.

Required checks:

```bash
cargo test -p nervusdb-api
cargo test -p nervusdb-query
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_rust_api
cargo test -p nervusdb --test core_0_1_mini_cypher
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
bash scripts/check.sh
```

If any check is skipped, `PROGRESS.md` must record why and what replaces it.

Result: complete. All required checks passed, and `cargo test --workspace` was
also run successfully.

### D6: Public Surface Synchronization

Make the public-facing repository surface match the committed Fjall and query
scope changes.

Done when:

- README and README_CN describe local database directory storage
- CLI help describes `--db` as a directory
- rustdoc no longer describes old run merging, Pager/WAL files, or the old M3
  query surface as current behavior
- current architecture/codebase docs describe Fjall keyspaces and Mini-Cypher
  0.1 instead of pre-Fjall internals
- focused validation passes after the cleanup

Result: complete. README, README_CN, CLI help, rustdoc, current code
architecture, current codebase analysis, and progress records now describe the
committed Fjall directory-storage model and Mini-Cypher 0.1 query surface.

## Validation Evidence

Executed on 2026-06-21:

```bash
cargo check -p nervusdb-storage --lib --bins
cargo check -p nervusdb
cargo test -p nervusdb-api
cargo test -p nervusdb-query
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_rust_api
cargo test -p nervusdb --test core_0_1_mini_cypher
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
cargo fmt --all -- --check
bash scripts/check.sh
cargo test --workspace
cargo fmt --all -- --check
cargo check -p nervusdb-cli -p nervusdb-api -p nervusdb-query -p nervusdb
bash scripts/check.sh
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_examples
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
cargo test --workspace
```

## Forbidden Work

- Do not restore platform bindings.
- Do not promote full openCypher.
- Do not reimplement WAL, B+Tree, Pager, or CSR under new names.
- Do not preserve `.ndb/.wal` compatibility as a hidden constraint.
- Do not add `eid` or parallel edge semantics in this refactor.
- Do not add property range indexing to close a storage refactor test.
- Do not use hash-based property keys.

## Current Known Risks

- `Db::checkpoint` and `Db::close` remain explicit lifecycle helpers over Fjall
  persistence. `Db::compact`, `Db::create_index`, and
  `GraphSnapshot::lookup_index` were removed before 0.1 to avoid false
  compaction/index promises.
- Large-scale storage evidence remains manual. Release candidates should still
  run the documented large smoke and crash/reopen checks on recorded hardware.
- Historical completed plans and ADR context still mention old storage or
  advanced query work as past evidence. Current scope must be taken from this
  plan, ADR 0005, product scope, and the current architecture docs.
