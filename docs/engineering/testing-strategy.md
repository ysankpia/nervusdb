# Testing Strategy

The 0.1 test strategy protects embedded database correctness, not feature
expansion.

## Default Local Checks

Use:

```bash
bash scripts/check.sh
```

This runs formatting, clippy for the public library crate, CLI, and local
wrapper crates, plus the core 0.1 quick test. The quick test is deliberately
small:

```bash
cargo test -p nervusdb --test core_0_1_mini_cypher
```

The default clippy scope is also deliberate. It checks the Rust-first embedded
path plus the `publish = false` wrapper crates that re-export the public
`nervusdb` modules:

```bash
cargo clippy \
  -p nervusdb-api \
  -p nervusdb-storage \
  -p nervusdb-query \
  -p nervusdb \
  -p nervusdb-cli \
  --lib --bins \
  -- -W warnings
```

## Required Test Bias

- Storage changes need persistence, reopen, and recovery-oriented tests.
- WAL changes need crash or replay coverage.
- Query changes need deterministic result tests for the Mini-Cypher surface.
- Public Rust API changes need facade-level tests or examples.
- Bug fixes need a regression guard before closeout.

## Test Cost Rule

Run the smallest test that proves the touched boundary:

- Docs-only changes: script syntax, link/grep checks, and no Rust test unless
  examples or API docs changed.
- CI/script changes: shell syntax plus a small representative command.
- Mini-Cypher changes: `cargo test -p nervusdb --test core_0_1_mini_cypher`
  plus targeted query tests for the changed operator.
- Storage/WAL changes: targeted storage tests plus `scripts/core_crash_recovery.sh`.
The goal is evidence, not ritual.
