# ADR 0004: Core Experimental Frozen Layering

## Status

Accepted

## Context

The repository contains useful work that is not useful for finishing 0.1 right
now. Deleting it immediately would create avoidable build and history risk.
Letting it remain on the default path would keep rewarding feature creep.

## Decision

Classify the workspace as three layers:

- Core: Rust embedded API, local storage, WAL recovery, graph persistence,
  traversal, Mini-Cypher, and CLI smoke/debug/import.
- Experimental: bindings, vector/HNSW, optimizer work outside Mini-Cypher,
  backup/vacuum workflows, full TCK harness, and stress/perf scripts.
- Frozen: full openCypher semantics, procedures, subqueries, pattern
  comprehension, broad temporal/duration work, complex aggregation,
  cross-binding parity gates, and release/stability windows.

Promotion from experimental or frozen to core requires a new ADR plus product,
architecture, validation, and plan updates.

## Consequences

- Current code can remain while the default path becomes smaller.
- CI and docs stop presenting experimental/frozen work as 0.1 success criteria.
- Future hard isolation can use Cargo features, workspace exclusions, or crate
  moves after soft isolation is stable.
