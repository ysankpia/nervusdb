# Workspace Layers

## Core

- `nervusdb`
- `nervusdb-api`
- `nervusdb-storage`
- `nervusdb-query` Mini-Cypher path
- `nervusdb-cli` core smoke/debug/import subset

## Experimental

- bindings
- vector/HNSW
- optimizer work outside Mini-Cypher
- backup/vacuum workflows outside the embedded 0.1 loop
- full TCK harness
- fuzz/perf/chaos/soak scripts

## Frozen

- full openCypher semantics
- procedures
- subqueries
- pattern comprehension
- broad temporal/duration semantics
- complex aggregation
- cross-binding parity release gates
- release/stability windows

Promotion into core requires a new ADR plus updates to product, architecture,
validation, and active plan docs.

