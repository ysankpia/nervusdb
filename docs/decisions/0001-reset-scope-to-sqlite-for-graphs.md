# ADR 0001: Reset Scope To SQLite For Graphs

## Status

Accepted

## Context

The repository accumulated work across full Cypher compatibility, query
optimization, WAL storage, vector indexing, multiple language bindings, TCK
coverage, fuzzing, chaos, soak, perf, release gates, and SDK parity. That made
the project look complete in breadth while weakening the focus needed for a
credible embedded database 0.1.

The durable product idea is narrower: SQLite-style local graph storage.

## Decision

The 0.1 line is Rust-first and embedded-first. It focuses on local files,
crash-safe graph persistence, basic traversal, and a small Mini-Cypher surface.

Full openCypher, procedures, subqueries, pattern comprehension, default vector
search, SDK expansion, and industrial nightly gate matrices are frozen before
0.1 unless this decision is superseded.

## Consequences

- Current historical achievements remain in the repository but stop driving the
  main scope.
- New work must justify itself against `docs/product/scope-0.1.md`.
- Binding, vector, TCK, and perf scripts can still be run manually for relevant
  changes, but they are not the default development loop.
- Future expansion needs new ADRs after the embedded Rust core is credible.
