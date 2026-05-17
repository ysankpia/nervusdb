# Binding Parity Matrix (Rust Baseline)

- Updated: 2026-02-16
- Baseline: Rust behavior is the single source of truth. Node and Python must be isomorphic.
- Policy: No `skip` to mask binding gaps. If Rust core has a gap, all three platforms assert the same gap.

## Scope

This matrix covers three API layers:
1. `Db` high-level API
2. `WriteTxn` low-level write transaction API
3. Module-level maintenance API (`vacuum` / `backup` / `bulkload`)

## Capability Matrix

| Capability | Rust | Node | Python | Notes |
|---|---|---|---|---|
| `open(path)` | ok | ok | ok | |
| `open_paths` / `openPaths` | ok | ok | ok | |
| `path` | ok | ok | ok | |
| `ndb_path` / `ndbPath` | ok | ok | ok | |
| `wal_path` / `walPath` | ok | ok | ok | |
| `query(cypher, params?)` | ok | ok | ok | Parameterized, aligned |
| `execute_write` / `executeWrite` | ok | ok | ok | Writes enforced |
| `begin_write` / `beginWrite` | ok | ok | ok | |
| `compact` | ok | ok | ok | |
| `checkpoint` | ok | ok | ok | |
| `create_index` / `createIndex` | ok | ok | ok | |
| `search_vector` / `searchVector` | ok | ok | ok | |
| `close` | ok | ok | ok | |

### WriteTxn API

| Capability | Rust | Node | Python | Notes |
|---|---|---|---|---|
| `WriteTxn.query` | ok | ok | ok | |
| `WriteTxn.commit` / `rollback` | ok | ok | ok | |
| `WriteTxn.create_node` / `createNode` | ok | ok | ok | |
| `WriteTxn.get_or_create_label` / `getOrCreateLabel` | ok | ok | ok | |
| `WriteTxn.get_or_create_rel_type` / `getOrCreateRelType` | ok | ok | ok | |
| `WriteTxn.create_edge` / `createEdge` | ok | ok | ok | |
| `WriteTxn.tombstone_node` / `tombstoneNode` | ok | ok | ok | |
| `WriteTxn.tombstone_edge` / `tombstoneEdge` | ok | ok | ok | |
| `WriteTxn.set_node_property` / `setNodeProperty` | ok | ok | ok | |
| `WriteTxn.set_edge_property` / `setEdgeProperty` | ok | ok | ok | |
| `WriteTxn.remove_node_property` / `removeNodeProperty` | ok | ok | ok | |
| `WriteTxn.remove_edge_property` / `removeEdgeProperty` | ok | ok | ok | |
| `WriteTxn.set_vector` / `setVector` | ok | ok | ok | |

### Module-Level API

| Capability | Rust | Node | Python | Notes |
|---|---|---|---|---|
| `vacuum(path)` | ok | ok | ok | |
| `backup(path, backup_dir)` | ok | ok | ok | |
| `bulkload(path, nodes, edges)` | ok | ok | ok | Node: camelCase fields; Python: snake_case |

## Naming Conventions

| Rust | Node.js | Python |
|------|---------|--------|
| `snake_case` | `camelCase` | `snake_case` |
| `begin_write()` | `beginWrite()` | `begin_write()` |
| `execute_write()` | `executeWrite()` | `execute_write()` |
| `search_vector()` | `searchVector()` | `search_vector()` |

## Error Semantics

| Platform | Error Model |
|----------|-------------|
| Rust | Native `Error` with category classification |
| Node.js | Structured JSON payload: `{ code, category, message }` |
| Python | Typed exceptions: `SyntaxError`, `ExecutionError`, `StorageError`, `CompatibilityError` |

Rule: same input must produce the same error category on all three platforms.

## Core Gap Status (Engine-Level, Cross-Binding)

No open engine-level core gaps are currently tracked in the parity suite.

Resolved in recent cycles:
- Multi-label subset matching now works for multi-label nodes (`MATCH (n:Manager)`).
- Relationship `MERGE` now enforces idempotent creation semantics.
- `left()` / `right()` string functions are implemented and asserted on all bindings.
- `MATCH p = shortestPath((...)-[*]->(...))` parsing/execution is enabled and asserted on all bindings.

## Alignment Status

| Phase | Status |
|-------|--------|
| S1: Semantics alignment and gap freeze | Complete |
| S2: Node behavioral convergence | Complete |
| S3: API surface alignment | Complete |
| S4: Maintenance and advanced API alignment | Complete |
| S5: CI gate enforcement | In progress |

## Gate Commands

```bash
bash examples-test/run_all.sh
bash scripts/binding_parity_gate.sh
```

Pass criteria:
1. Rust / Node / Python capability tests all green.
2. Parity gate report output to `artifacts/tck/`.
3. No `skip` masking binding differences.
