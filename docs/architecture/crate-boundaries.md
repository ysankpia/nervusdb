# Crate Boundaries

## Core Crates

- `nervusdb`: public Rust facade for open/reopen, snapshots, write
  transactions, and query composition.
- `nervusdb-api`: graph traits, shared IDs, `PropertyValue`, and storage-neutral
  read/write boundaries.
- `nervusdb-storage`: Fjall-backed local graph storage and graph-keyspace
  implementation.
- `nervusdb-query`: Mini-Cypher parser/planner/executor path.
- `nervusdb-cli`: local smoke/debug/import/query/write commands.

## Required Dependency Direction

```text
nervusdb-cli -> nervusdb
nervusdb     -> nervusdb-api
nervusdb     -> nervusdb-storage
nervusdb     -> nervusdb-query
nervusdb-storage -> nervusdb-api
nervusdb-query   -> nervusdb-api
```

`nervusdb-query` and `nervusdb-storage` must not depend on each other. Their
contract is `nervusdb-api`.

## Experimental Or Frozen Areas

- full openCypher
- full TCK harness
- vector/HNSW
- Python, Node.js, and C bindings
- cross-binding parity gates
- perf, chaos, soak, fuzz, benchmark, and stability matrices

Frozen means build/security maintenance is allowed. New capability work before
0.1 requires an ADR.
