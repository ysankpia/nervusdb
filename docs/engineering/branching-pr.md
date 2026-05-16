# Branching And PR Rules

- `main` is the protected trunk.
- Development happens on short-lived branches.
- Use PRs for integration and squash merge by default.
- Keep commits scoped to one product or engineering change.
- Do not include unrelated local edits.
- Do not force push or rewrite shared history without explicit instruction.

## Branch Names

- `chore/...` for harness, docs, CI, or repository maintenance.
- `fix/...` for bug fixes.
- `feat/...` for scoped 0.1 feature work.
- `refactor/...` for behavior-preserving internal cleanup.

## PR Body

Every non-trivial PR should include:

- summary
- validation commands
- docs updated
- scope impact
- risks or follow-up work
