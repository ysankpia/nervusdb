# Crate Boundaries

## Core Crates

- `nervusdb`: public Rust facade for open/reopen, snapshots, and write
  transactions.
- `nervusdb-api`: graph traits and shared IDs at the query/storage boundary.
- `nervusdb-storage`: local page store, WAL, recovery, labels, properties,
  indexes needed by core, and traversal storage.
- `nervusdb-query`: Mini-Cypher parser/planner/executor path.
- `nervusdb-cli`: local smoke/debug/import/query/write commands.

## Experimental Or Frozen Areas

- `nervusdb-pyo3`
- `nervusdb-node`
- `nervusdb-capi`
- full TCK harness
- vector/HNSW
- cross-binding parity gates
- perf, chaos, soak, fuzz, benchmark, and stability scripts

Frozen means build/security maintenance is allowed. New capability work before
0.1 requires an ADR.

