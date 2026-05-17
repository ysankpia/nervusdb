# NervusDB 0.1 Scope

This file is the working scope boundary for the 0.1 line. Anything outside this
scope is frozen unless this document changes first.

## 30-Day Stop Rule

Until the active slimdown plan is closed or superseded, do not add new full
Cypher features, non-Rust stable APIs, vector/HNSW default-path behavior,
optimizer breadth, or release/nightly gate pressure. Build fixes and security
fixes in frozen areas are allowed.

## In Scope Before 0.1

- Rust embedded API for opening a local database path.
- Local file storage with explicit storage format versioning.
- WAL-backed crash recovery and reopen correctness.
- Node, relationship, label, and property persistence.
- Single-writer write transactions and snapshot-style reads.
- Label scans and neighbor traversal by relationship type.
- One-hop and two-hop traversal examples.
- Basic property filtering needed by common local graph queries.
- A small Mini-Cypher subset for simple `MATCH`, `WHERE`, `RETURN`, `LIMIT`,
  and basic write statements already on the core path.
- CLI support for debugging, smoke testing, and import-style local workflows.
- Focused tests for persistence, recovery, API behavior, and query correctness.

## Frozen Before 0.1

- Full openCypher compatibility as a product goal.
- Procedures, subqueries, pattern comprehension, and complex clause interaction.
- Full openCypher TCK pass rate as a blocking requirement.
- Python, Node.js, or C API stabilization beyond compatibility maintenance.
- HNSW/vector search as a default product path.
- Advanced cost-based optimizer work not needed by the Mini-Cypher core path.
- Nightly chaos, soak, fuzz, TCK, and perf matrices as PR-blocking development
  gates.

## Allowed Maintenance On Frozen Areas

Frozen does not mean deleted. Existing code may receive build fixes, security
fixes, or compatibility patches when needed to keep the repository healthy. New
capability work in frozen areas requires a decision record.

## 0.1 Acceptance Criteria

- A Rust program can create and reopen a local graph database.
- Basic graph writes persist through process restart.
- Crash recovery tests cover the WAL path.
- One-hop and two-hop query examples are documented and tested.
- Query results for the Mini-Cypher surface are deterministic.
- A documented large smoke/benchmark can create 1,000,000 nodes and 5,000,000
  edges without corruption on documented hardware.
- One-hop and two-hop benchmark output includes reproducible P50/P95/P99
  numbers.
- Rust API docs are clear enough for a user to open a local database without a
  server or non-Rust SDK.
- Ten realistic examples are documented and runnable.
- `bash scripts/check.sh` passes locally and in CI-equivalent environments
  without running historical full-test fan-out by default.
