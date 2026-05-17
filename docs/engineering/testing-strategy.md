# Testing Strategy

The 0.1 test strategy protects embedded database correctness, not feature
expansion.

## Default Local Checks

Use:

```bash
bash scripts/check.sh
```

This runs formatting, clippy for the 0.1 core crates, and the core 0.1 quick
test. The quick test is deliberately small:

```bash
cargo test -p nervusdb --test core_0_1_mini_cypher
```

The default clippy scope is also deliberate. It checks the Rust-first embedded
path only:

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

Do not hide full test fan-out behind a "quick" name. Full historical tests are
manual and explicit:

```bash
bash scripts/workspace_full_test.sh
```

That command is allowed to be slow. It is for release preparation, broad
refactors, or changes that intentionally touch old integration surfaces.

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
- Large refactors: `scripts/workspace_full_test.sh` only when the blast radius
  justifies the cost.

The goal is evidence, not ritual.

## Non-Blocking Historical Gates

The repository still contains TCK, binding parity, vector, chaos, soak, fuzz,
and performance scripts. They are valuable manual signals, but they are not
scheduled pressure and are not the default 0.1 development loop unless the
touched area specifically requires them.

Advanced query tests, including optional match, `WITH`, `UNION`, `UNWIND`,
aggregation, procedures, subqueries, pattern comprehension, and openCypher TCK
material, are compatibility evidence. They are not the Mini-Cypher 0.1
acceptance suite and must not be added to `scripts/check.sh` or
`scripts/workspace_quick_test.sh`.

## When To Run More

- Run TCK-related scripts only for query compatibility changes.
- Run binding smoke/parity scripts only for binding changes.
- Run perf scripts only for performance-sensitive changes.
- Run fuzz or chaos scripts for parser, executor, WAL, or IO fault work when the
  risk justifies it.
- Run crash recovery scripts for storage/WAL changes even though they are not
  scheduled by default.
