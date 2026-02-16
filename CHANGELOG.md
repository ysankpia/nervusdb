# Changelog (v2)

This file records verifiable changes for **v2 (`.ndb` + `.wal`)** only.

For v1/redb and legacy binding history, see `_legacy_v1_archive/CHANGELOG_v1.md`.

## Unreleased

### Milestones

- **TCK 100%**: openCypher TCK full pass rate reached 100% (3 897 / 3 897 scenarios).
- **Three-platform binding alignment**: Rust (153 tests), Python (138 tests), Node.js (109 tests) — all green, API parity verified.
- **SQLite-Beta convergence**: entered 7-day stability window (Phase B).
- **Documentation restructure**: all docs rewritten in English, public-facing quality.

### Features

- Query engine refactoring: split `query_api` / `executor` / `evaluator` for maintainability.
- Python binding: typed objects (`Node` / `Relationship` / `Path`), typed exception hierarchy (`SyntaxError` / `ExecutionError` / `StorageError` / `CompatibilityError`).
- Node.js binding: structured error payloads (`code` / `category` / `message`), full API alignment with Rust baseline.
- Storage format epoch enforcement with `StorageFormatMismatch` error on mismatch.
- Binding parity gate (`scripts/binding_parity_gate.sh`) for CI enforcement.

### Fixes

- Variable-length path execution: `MATCH (a)-[*min..max]->(b)` correctly dispatches as `MatchOutVarLen` with default hop limit.
- Clippy warnings resolved (`collapsible_if`, `type_complexity`, `too_many_arguments`).

### Maintenance

- v1 fully retired: removed from workspace/CI, archived to `_legacy_v1_archive/`.
- Documentation de-lied: all docs aligned with actual v2 capabilities and boundaries.
- Outdated docs archived to `docs/archive/superseded/`.
