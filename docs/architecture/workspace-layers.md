# Workspace Layers

## Core

- `nervusdb`
- `nervusdb-api`
- `nervusdb-storage`
- `nervusdb-query` Mini-Cypher path
- `nervusdb-cli` core smoke/debug/import subset

The core is embedded Rust graph storage and query. It uses a local database
directory and Fjall-backed keyspaces.

## Experimental

- property indexes beyond 0.1 equality/filter execution
- optimizer work outside Mini-Cypher
- backup/vacuum workflows outside the embedded 0.1 loop
- benchmark recording and comparison

## Frozen

- full openCypher semantics
- procedures
- subqueries
- pattern comprehension
- broad temporal/duration semantics
- complex aggregation
- `OPTIONAL MATCH`, `WITH`, `UNION`, `UNWIND`, `ORDER BY/SKIP` as core gates
- Python, Node.js, and C bindings
- vector/HNSW
- cross-binding parity release gates
- release/stability windows

Promotion into core requires a new ADR plus updates to product, architecture,
validation, and active plan docs.
