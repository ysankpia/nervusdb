# Plan 005: API Surface Refactor

## Status

In progress

## Goal

Make the Rust embedded API obvious and stable enough for 0.1 without first
breaking or deleting historical public functions.

## Scope

- Classify API surface as core, experimental, or maintenance.
- Improve docs around `Db::open`, `Db::open_paths`, snapshots, write
  transactions, graph persistence, traversal, and Mini-Cypher execution.
- Keep binding-facing wrappers out of the 0.1 story unless needed for build
  maintenance.
- Decide later whether experimental APIs should become feature-gated,
  `#[doc(hidden)]`, or moved.

## Not In Scope

- First-pass breaking removal.
- Stable Python, Node.js, or C API expansion.
- API additions that only serve vector, optimizer, or full-Cypher ambitions.

## Current Audit

| API area | Public items | 0.1 status | Evidence / action |
|---|---|---|---|
| Open local database | `Db::open`, `Db::open_paths`, `Db::ndb_path`, `Db::wal_path` | Core | Add facade baseline test for path derivation |
| Read snapshots | `Db::snapshot`, `DbSnapshot`, `GraphSnapshot` methods | Core | Add facade baseline test for labels, properties, traversal |
| Read transactions | `Db::begin_read`, `ReadTxn::neighbors` | Core | Add facade baseline test for neighbor traversal |
| Write transactions | `Db::begin_write`, `WriteTxn::*` graph persistence methods, `commit` | Core | Add facade baseline test for node, edge, label, property persistence |
| Mini-Cypher execution | `nervusdb::query`, `nervusdb_query` prepare/execute helpers | Core | Already covered by Phase 004 query baseline |
| Maintenance IO | `Db::compact`, `Db::checkpoint`, `Db::close`, `vacuum`, `backup`, `bulkload` | Experimental / maintenance | Classify in docs; no removal |
| Index/vector | `Db::create_index`, `Db::search_vector`, `WriteTxn::set_vector` | Experimental / maintenance | Classify in docs; no 0.1 stability promise |
| Bindings compatibility | binding-facing wrappers and exported support types | Experimental / maintenance | Keep out of root 0.1 story |

## Steps

1. Audit root facade docs and public examples.
2. Mark the 0.1 core API path in docs.
3. Classify maintenance/experimental APIs without removing them.
4. Add facade-level examples or tests where the core path is unclear.
5. Update README quick start if the public path changes.

## Validation

- `cargo doc -p nervusdb --no-deps` when API docs change.
- Focused facade tests or examples.
- `bash scripts/check.sh` before commit.

## Docs To Update

- `docs/architecture/api-surface.md`
- `docs/reference/rust-api.md`
- `docs/product/scope-0.1.md` if public scope changes.
- `README.md` and `README_CN.md` if quick start changes.

## Completion Evidence

- `docs/reference/rust-api.md` lists 0.1 core and experimental/maintenance APIs.
- `nervusdb/src/lib.rs` rustdoc classifies public facade methods without
  signature changes.
- `nervusdb/tests/core_0_1_rust_api.rs` proves the Rust facade path.
- `cargo doc -p nervusdb --no-deps` and `bash scripts/check.sh` pass.
- Experimental/maintenance APIs remain callable but are not 0.1 stability
  promises.
