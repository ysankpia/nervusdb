# Architecture Overview

NervusDB is a Rust workspace organized around an embedded database kernel.

## Current Crate Boundaries

- `nervusdb`: public Rust facade (`Db::open`, query, write execution).
- `nervusdb-storage`: page store, WAL, snapshots, indexes, recovery, and file
  format mechanics.
- `nervusdb-query`: parser, planner, evaluator, and executor for the query
  surface.
- `nervusdb-api`: traits that separate query execution from storage access.
- `nervusdb-cli`: command-line entry point for local debugging and smoke usage.
- `nervusdb-pyo3`, `nervusdb-node`, `nervusdb-capi`: existing bindings kept out
  of the 0.1 growth path.

See `docs/architecture/workspace-layers.md` for the current core,
experimental, and frozen classification.

## 0.1 Architecture Direction

The core path is:

```text
Rust API / CLI
  -> Mini-Cypher or direct operation
  -> query/storage boundary
  -> storage engine
  -> local page store + WAL
```

The storage layer is the foundation. It must keep file format changes explicit,
reject incompatible epochs cleanly, and preserve crash recovery behavior before
query language breadth expands.

## Boundaries To Protect

- Public Rust API should remain small and stable.
- Storage format changes require tests and versioning.
- Query semantics should stay within the 0.1 Mini-Cypher scope.
- Bindings should not drive core design before the Rust API is settled.
- Experimental vector and full-Cypher work must not become required for the
  default local embedded path.
- CI must not make experimental or frozen areas required for ordinary 0.1
  development.
