# Roadmap

## Current Phase

Refactoring toward SQLite-for-graphs 0.1 — cutting the repository back from
platform-era breadth to a finishable embedded Rust graph database.

## Now

- Storage core baseline: local files, WAL recovery, persistence invariants.
- Query core baseline: Mini-Cypher acceptance, deterministic results.
- API surface classification: core vs experimental vs maintenance.
- CLI/examples/validation: runnable examples, smoke, crash recovery, benchmark.

## Next

- Comprehensive crash recovery and reopen test suite.
- Mini-Cypher edge-case hardening (limit 0, empty label, error paths).
- Facade API documentation pass (rustdoc + reference).
- Large manual acceptance smoke (1M nodes / 5M edges).

## Later

- Cargo feature isolation for experimental and frozen code.
- Benchmark regression detection for the core path.
- Release mechanics and publish documentation.
- Community contribution guide.

## Milestones

| Milestone | Target | Evidence |
|---|---|---|
| Storage boring | Q2 2026 | Format epoch fail-fast, reopen tests, crash recovery script passes |
| Query boring | Q2 2026 | All Mini-Cypher forms in core test, advanced tests isolated |
| API obvious | Q2 2026 | Core Rust path documented, experimental APIs classified |
| 0.1 credible | Q2 2026 | Ten examples runnable, recovery proven, large smoke passes |
| 0.1 release | Q2 2026 | Published to crates.io, docs complete, validation repeatable |

## Open Questions

- Whether to feature-gate experimental code before 0.1 or keep soft isolation.
- Whether `nervusdb-node`, `nervusdb-pyo3`, and `nervusdb-capi` should stay workspace members or move out.
- Whether backup/vacuum/bulkload become 0.1 core or stay maintenance-only.
