# Git Workflow

## Trunk

`main` is the trunk. All work targets `main`.

## Branch Strategy

Use short-lived branches when branch protection is enabled or when the user asks
for PR integration. If working locally on `main`, keep commits scoped and push
directly only after validation.

### Branch Naming

- `chore/...` — harness, docs, CI, or repository maintenance.
- `fix/...` — bug fixes.
- `feat/...` — scoped 0.1 feature work.
- `refactor/...` — behavior-preserving internal cleanup.

## Commit Discipline

- Keep commits scoped to one product or engineering change.
- Do not include unrelated local edits in any commit.
- Do not force push or rewrite shared history without explicit instruction.
- Use Conventional Commits for the subject line when it helps downstream
  tooling, but the primary requirement is a clear, scoped description.

## PR Body (for non-trivial PRs)

Every non-trivial PR should include:

- summary of the change
- validation commands run and their results
- docs updated
- scope impact (core, experimental, frozen)
- risks or follow-up work

## What Not To Commit

- Secrets, keys, or credentials.
- Runtime outputs, database files, or storage artifacts.
- `target/` or other build output.
- Caches, temporary files, or `.tmp/` / `.temp/` directories.
- Dist directories or generated bundle artifacts.
- `.DS_Store`, `Thumbs.db`, or OS noise files.
- Unrelated user changes (leave them unstaged and mention them).

## Pre-Commit Check

Run the smallest validation that proves the touched boundary (see
`docs/engineering/validation-policy.md` for the change-type-to-command mapping).

Minimum: `bash scripts/check.sh` passes, or any skipped part is explicitly
explained in the commit message.

## Merge Rules

- Rebase before merging to keep history linear on shared branches.
- Squash-merge is acceptable for single-developer feature branches.
- Do not merge with failing CI on `main`.

## References

- Branch naming convention: `docs/engineering/branching-pr.md`.
- Validation policy: `docs/engineering/validation-policy.md`.
- Definition of done: `docs/engineering/definition-of-done.md`.
