# Architecture Overview

NervusDB 0.1 is an embedded Rust property graph database. The default path is
local, single-process, and storage-directory based:

```text
Rust API / CLI
  -> Mini-Cypher or direct graph API
  -> nervusdb-api traits
  -> storage adapter
  -> Fjall keyspaces in a local database directory
```

## Boundary Rules

- `nervusdb-api` owns shared IDs, `PropertyValue`, `GraphSnapshot`,
  `GraphStore`, and write-boundary traits.
- `nervusdb-storage` owns graph persistence, keyspace layout, durability,
  recovery-facing behavior, labels, relationship types, properties, and
  traversal storage.
- `nervusdb-query` owns the documented Mini-Cypher path before 0.1. It must not
  depend on `nervusdb-storage`.
- `nervusdb` is the Rust facade. It composes storage and query and should not
  grow platform SDK behavior.
- `nervusdb-cli` is a smoke/debug/import-style tool, not a separate product
  surface.
- Bindings, vector search, full TCK, parity gates, and perf matrices are
  historical or experimental until promoted by ADR.

## Design Bias

Keep the core boring: local database directories, explicit logical format
versioning, single-writer transactions, snapshot reads, deterministic query
results, and graph-level recovery tests.
