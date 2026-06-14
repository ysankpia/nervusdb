# Plan 007: Harness Doc Backfill

## Status

Completed

## Goal

Backfill all missing project-harness documentation files and update existing docs
to create a complete, self-recoverable repository contract.

## Scope

- Create 11 new files: `docs/product/direction-contract.md`, `docs/roadmap.md`,
  `PROGRESS.md`, `docs/engineering/quality-score.md`,
  `docs/engineering/architecture-invariants.md`,
  `docs/engineering/git-workflow.md`,
  `docs/engineering/dependency-policy.md`, `docs/runbooks/doc-gardening.md`,
  `docs/runbooks/local-setup.md`, `docs/plans/tech-debt.md`,
  `docs/reference/generated-artifacts.md`.
- Update 6 existing files: `AGENTS.md`, `docs/index.md`,
  `docs/engineering/definition-of-done.md`,
  `docs/engineering/documentation-policy.md`,
  `docs/plans/active/001-harness-normalization.md`,
  `docs/reference/glossary.md`.

## Not In Scope

- Rust implementation changes.
- Script changes.
- CI workflow changes.
- Physical deletion of experimental/frozen code.

## CodeGraph Usage

- Explored crate boundaries, internal dependency graph, and workspace layers for
  `architecture-invariants.md` and `dependency-policy.md`.
- Explored validation scripts (`check.sh`, `workspace_quick_test.sh`,
  `workspace_full_test.sh`, `core_smoke.sh`) for `quality-score.md`.
- Explored CI workflows for `quality-score.md` and `tech-debt.md`.
- Explored bug ledger for completeness assessment.
- Explored `Cargo.toml` workspace members for `dependency-policy.md`.

## Steps

1. CodeGraph deep exploration (crate boundaries, scripts, CI, bug ledger, workspace structure).
2. Create `docs/product/direction-contract.md` from vision + scope + non-goals + ADRs.
3. Create `docs/roadmap.md` synthesizing active plans and milestones.
4. Create `PROGRESS.md` as live execution ledger.
5. Create `docs/engineering/quality-score.md` with CodeGraph evidence.
6. Create `docs/engineering/architecture-invariants.md` from crate boundaries and code analysis.
7. Create `docs/engineering/git-workflow.md` merging branching-pr.md + AGENTS.md.
8. Create `docs/engineering/dependency-policy.md` from Cargo workspace analysis.
9. Create `docs/runbooks/doc-gardening.md` with lifecycle rules.
10. Create `docs/runbooks/local-setup.md` separated from local-validation.md.
11. Create `docs/plans/tech-debt.md` from plans, FIXMEs, and known issues.
12. Create `docs/reference/generated-artifacts.md` with build/test artifact policy.
13. Update AGENTS.md — Read Order and Done section.
14. Update docs/index.md — add all new files, restructure sections.
15. Update docs/engineering/definition-of-done.md — add quality-score/tech-debt/architecture-invariants triggers.
16. Update docs/engineering/documentation-policy.md — add new doc types.
17. Mark plan-001 as superseded.
18. Update docs/reference/glossary.md with harness terms.
19. Create this plan.
20. Run `bash scripts/check.sh`.
21. Commit.

## Validation

- `bash scripts/check.sh` — ensure no Rust regression from doc-only changes.
- Manual doc link check via `rg` on new docs for cross-references.
- `git status --short` confirms only intended files changed.

## Docs Created Or Updated

### Created

| File | Description |
|---|---|
| `docs/product/direction-contract.md` | Product definition, scope, acceptance |
| `docs/roadmap.md` | Phase, now/next/later, milestones |
| `PROGRESS.md` | Live execution ledger |
| `docs/engineering/quality-score.md` | 0-5 assessment with evidence |
| `docs/engineering/architecture-invariants.md` | Always-true system rules |
| `docs/engineering/git-workflow.md` | Branch, commit, PR discipline |
| `docs/engineering/dependency-policy.md` | Internal/external dep rules |
| `docs/runbooks/doc-gardening.md` | Stale doc cleanup lifecycle |
| `docs/runbooks/local-setup.md` | Environment setup guide |
| `docs/plans/tech-debt.md` | Active/deferred/accepted debt |
| `docs/reference/generated-artifacts.md` | Build/test artifact policy |

### Updated

| File | Change |
|---|---|
| `AGENTS.md` | Read Order adds direction-contract, roadmap, PROGRESS; Done adds quality-score/tech-debt/architecture-invariants |
| `docs/index.md` | Adds all new files, restructures sections |
| `docs/engineering/definition-of-done.md` | Adds quality-score/tech-debt/architecture-invariants triggers |
| `docs/engineering/documentation-policy.md` | Adds new doc types and doc-gardening trigger |
| `docs/plans/active/001-harness-normalization.md` | Status: superseded by 007 |
| `docs/reference/glossary.md` | Adds harness terms |

## Completion Evidence

- `bash scripts/check.sh` passes.
- All 11 new files exist and are referenced from `docs/index.md`.
- 6 updated files contain correct cross-references.
- `git status --short` shows only these 17 files changed.
