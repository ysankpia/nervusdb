# Workspace Layers

This file classifies the current repository without physically deleting code.
Soft isolation comes first: docs, README, and CI stop promoting non-core work.
Hard isolation can happen later with Cargo features, workspace exclusions, or
crate moves.

## Core

Core code must stay healthy in the default development loop.

- `nervusdb`: public Rust facade for open/reopen, snapshots, and write
  transactions.
- `nervusdb-api`: graph traits and shared IDs at the query/storage boundary.
- `nervusdb-storage`: local page store, WAL, snapshots, recovery, labels,
  properties, and traversal storage.
- `nervusdb-query`: Mini-Cypher parser/planner/executor path.
- `nervusdb-cli`: local open/query/write/import-smoke workflows.

## Experimental

Experimental areas can remain in the repository, but they do not define 0.1.
They should not be required by default CI or root quick starts.

- `nervusdb-pyo3`
- `nervusdb-node`
- `nervusdb-capi`
- `examples-test/`
- HNSW/vector search
- optimizer work outside Mini-Cypher
- backup/vacuum APIs outside the embedded 0.1 loop
- full TCK harness
- fuzz targets
- perf, chaos, soak, and benchmark scripts

## Graveyard / Frozen

Frozen areas are not deleted yet, but they must not receive new capability work
before 0.1 without a new ADR.

- procedures
- subqueries
- pattern comprehension
- full openCypher semantics
- broad temporal/duration semantics
- complex aggregation
- cross-binding parity release gates
- stability/release windows

## Promotion Rule

To move anything from experimental or frozen into core, update these files in
the same change:

- `docs/spec.md`
- `docs/product/scope-0.1.md`
- this file
- `docs/engineering/testing-strategy.md`
- any affected local validation or CI workflow

