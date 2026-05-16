# Plan: Harness Scope Reset

## Status

Ready for review

## Goal

Reset NervusDB repository guidance to a 0.1 Rust-first embedded database line:
SQLite-style local graph storage, crash safety, and a small Mini-Cypher surface.

## Scope

- Replace the root `AGENTS.md` symlink with a short tracked guide.
- Add the harness documentation map and current product/engineering docs.
- Add the default local validation script.
- Freeze full Cypher, SDK expansion, vector defaults, and industrial gate work as
  non-0.1 scope.
- Leave `docs/spec.md` unchanged until explicit user confirmation.

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
- `bash scripts/workspace_quick_test.sh`
- `bash -n scripts/check.sh`
