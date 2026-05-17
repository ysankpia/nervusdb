# NervusDB Agent Guide

NervusDB is a Rust-first embedded property graph database: SQLite-style local
files, crash-safe storage, and a deliberately small graph query surface.

## Read Order

1. `docs/index.md`
2. `docs/product/vision.md`
3. `docs/product/scope-0.1.md`
4. `docs/architecture/overview.md`
5. The relevant engineering rule under `docs/engineering/`
6. The active plan or design note for the task, if one exists

Treat old Beta, TCK, binding, vector, and perf documents as historical unless
`docs/index.md` marks them as current.

## Task Routing

- Product scope, roadmap, or public behavior changes require a plan under
  `docs/plans/active/` or a design note under `docs/design/`.
- Storage, WAL, recovery, file format, public API, and query semantics changes
  require tests before implementation and explicit validation evidence.
- Bug fixes require a regression guard. If a deterministic test is not possible,
  document the guard and the reason in `docs/bugs/`.
- Pure docs updates may skip code tests, but must keep links and status language
  consistent with `docs/index.md`.

## Implementation Constraints

- Keep the 0.1 line focused on Rust embedded usage, local storage, crash safety,
  basic graph persistence, label scans, neighbor traversal, and Mini-Cypher.
- Do not expand full openCypher compatibility, procedures, subqueries, pattern
  comprehension, vector search defaults, or stable non-Rust SDK APIs unless the
  scope document is changed first.
- Prefer small modules and direct code over speculative abstraction.
- Treat `docs/spec.md` as the repository constitution. Change it only when the
  user explicitly authorizes a scope reset or constitutional update.

## Git And Integration Rules

- Prefer short-lived branches and PRs when branch protection is enabled.
- If the user explicitly requests local `main` work, do not create extra local
  branches. Keep the commit scoped, validate it, and push `main` directly.
- Do not include unrelated local edits in a commit.
- Never rewrite user changes unless the user explicitly asks for that exact
  cleanup.

## Validation

Use `scripts/check.sh` for the normal 0.1 development loop. It must stay a
short core gate, not a hidden full-suite runner:

```bash
bash scripts/check.sh
```

Default validation covers only the 0.1 core path:

- `cargo fmt --all -- --check`
- clippy for core crates (`nervusdb-api`, `nervusdb-storage`, `nervusdb-query`,
  `nervusdb`, `nervusdb-cli`) on lib/bin targets
- `bash scripts/workspace_quick_test.sh`

Do not run full workspace tests by reflex. Pick the smallest check that proves
the touched boundary:

- Docs-only changes: use shell syntax checks, link/grep checks, and no Rust test
  unless code examples changed.
- CI/script changes: use `bash -n` plus targeted grep or dry-run checks.
- Mini-Cypher changes: run `bash scripts/workspace_quick_test.sh` plus the
  narrow affected query tests.
- Storage/WAL changes: run targeted storage tests and
  `bash scripts/core_crash_recovery.sh`.
- Broad refactors or release preparation: run `bash scripts/workspace_full_test.sh`
  manually and expect it to be slow.

Never hide full test fan-out behind names like `quick`, `check`, `pre-commit`,
or `pre-push`. If a command is slow, name it as full/manual. See
`docs/engineering/testing-strategy.md` and `docs/runbooks/local-validation.md`.

## Definition Of Done

- Code or docs match the 0.1 scope.
- Tests or explicit regression guards cover behavior changes.
- Public API, storage format, build, release, or operational changes update docs.
- `git status --short` contains only intentional changes.
- Validation commands and results are recorded in the final report.
