# Plan 001: Harness Normalization

## Status

In progress

## Goal

Rebuild the repository harness so future work starts from the SQLite-for-graphs
0.1 line instead of the old platform-era scope.

## Scope

- Keep work on local `main`.
- Commit the prior transition state before this harness rewrite.
- Rewrite `AGENTS.md` as a short navigational guide.
- Make `docs/index.md` the only current documentation map.
- Build current product, architecture, engineering, runbook, reference, plan,
  ADR, and bug docs around 0.1.
- Move old platform-era material under `docs/archive/legacy-platform-era/`.
- Keep archive indexing short and require an ADR to revive archived material.

## Not In Scope

- Rust implementation changes.
- Physical deletion of advanced query, binding, vector, perf, fuzz, chaos, soak,
  or TCK code.
- Running full historical tests by reflex.
- Creating a new branch.

## Steps

1. Preserve the previous dirty state in a transition commit.
2. Archive old docs into coarse legacy groups.
3. Write current harness docs for product, architecture, engineering, runbooks,
   reference, decisions, bugs, and active plans.
4. Remove stale links to archived docs from README and current docs.
5. Validate scripts, documentation paths, and the fast core test path.
6. Commit and push `main`.

## Validation

- `bash -n` for default and core validation scripts.
- `make -n check quick-test full-test pre-commit`.
- `find docs -maxdepth 3 -type f | sort`.
- Current-doc grep for stale platform-era links and old success claims.
- `bash scripts/workspace_quick_test.sh`.

## Docs To Update

- `AGENTS.md`
- `README.md`
- `README_CN.md`
- `docs/index.md`
- all current harness docs listed in `docs/index.md`
- `docs/archive/legacy-platform-era/INDEX.md`

## Completion Evidence

Record the validation commands and commit hash in the final report.
