# Architecture Overview

NervusDB 0.1 is an embedded Rust graph database. The default path is local and
single-process:

```text
Rust API / CLI
  -> Mini-Cypher or direct write API
  -> nervusdb-api traits
  -> nervusdb-storage
  -> .ndb page store + .wal
```

## Boundary Rules

- `nervusdb-storage` owns durability, file layout, WAL, recovery, labels,
  properties, and traversal storage.
- `nervusdb-query` owns only the Mini-Cypher path before 0.1.
- `nervusdb` is the Rust facade. It should not grow platform SDK behavior.
- `nervusdb-cli` is a smoke/debug/import tool, not a separate product surface.
- Bindings, vector search, full TCK, parity gates, and perf matrices are
  historical or experimental until promoted by ADR.

## Design Bias

Keep the core boring: local files, explicit format versioning, single-writer
transactions, snapshot reads, and deterministic query results.
