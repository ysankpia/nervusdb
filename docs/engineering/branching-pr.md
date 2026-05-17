# Branching And Integration Rules

- `main` is the trunk.
- Use short-lived branches and PRs when branch protection is enabled or when the
  user asks for PR integration.
- If the user explicitly requests local `main` work, keep commits scoped and
  push `main` directly only after validation.
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
