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
- Do not change `docs/spec.md` without explicit user confirmation, because it is
  the repository constitution.

## Git And PR Rules

- Work on a short-lived branch. Do not commit directly to `main`.
- Use PRs, keep CI green, and squash merge by default.
- Do not include unrelated local edits in a commit.
- Never rewrite user changes unless the user explicitly asks for that exact
  cleanup.

## Validation

Use `scripts/check.sh` for the normal 0.1 development loop:

```bash
bash scripts/check.sh
```

Run narrower tests first when iterating, but the final result must include the
checks relevant to the touched subsystem. See `docs/engineering/testing-strategy.md`
and `docs/runbooks/local-validation.md`.

## Definition Of Done

- Code or docs match the 0.1 scope.
- Tests or explicit regression guards cover behavior changes.
- Public API, storage format, build, release, or operational changes update docs.
- `git status --short` contains only intentional changes.
- Validation commands and results are recorded in the final report.
