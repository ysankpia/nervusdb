# Testing Strategy

The 0.1 test strategy protects embedded database correctness, not feature
expansion.

## PR-Blocking Local Checks

Use:

```bash
bash scripts/check.sh
```

This runs formatting, clippy, and workspace quick tests.

## Required Test Bias

- Storage changes need persistence, reopen, and recovery-oriented tests.
- WAL changes need crash or replay coverage.
- Query changes need deterministic result tests for the Mini-Cypher surface.
- Public Rust API changes need facade-level tests or examples.
- Bug fixes need a regression guard before closeout.

## Non-Blocking Historical Gates

The repository still contains openCypher TCK, binding parity, vector, chaos,
soak, fuzz, and performance scripts. They are valuable manual or scheduled
signals, but they are not the default 0.1 development loop unless the touched
area specifically requires them.

## When To Run More

- Run TCK-related scripts only for query compatibility changes.
- Run binding smoke/parity scripts only for binding changes.
- Run perf scripts only for performance-sensitive changes.
- Run fuzz or chaos scripts for parser, executor, WAL, or IO fault work when the
  risk justifies it.
