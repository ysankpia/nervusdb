# PROGRESS

## Current Objective

Complete the repository harness layer: backfill missing
direction-contract/roadmap/PROGRESS, engineering maintenance ledgers, runbooks,
and reference docs; update existing docs to reference them.

## Active Plan

007-harness-doc-backfill

## Current Phase

Harness documentation — Phase 1 of 5 (direction layer).

## Now

Creating `docs/product/direction-contract.md`, `docs/roadmap.md`, and
`PROGRESS.md`.

## Done

- 001-harness-normalization — Agents.md, docs/index.md, product/architecture/engineering/runbook/reference/plan/ADR/bug docs rewritten for 0.1, legacy material archived.
- 002-core-0.1-slimdown — Core/experimental/frozen classification, CI focused on core, quick test separated from full fan-out.
- 003-storage-core-refactor — Storage invariant baseline, crash recovery tests, storage model documentation.
- 004-query-core-refactor — Mini-Cypher acceptance, query model documentation, advanced tests classified as compatibility evidence.
- 005-api-surface-refactor — Core API classification, Rust API reference, facade tests.
- 006-cli-examples-validation — CLI reference, ten runnable examples, smoke/crash/bench scripts.

## Next (in order)

1. `docs/engineering/quality-score.md` — 0-5 assessment with CodeGraph evidence.
2. `docs/engineering/architecture-invariants.md` — always-true rules from crate boundaries and code analysis.
3. `docs/engineering/git-workflow.md` — merge branching-pr.md + AGENTS.md into one workflow doc.
4. `docs/engineering/dependency-policy.md` — based on Cargo workspace structure.
5. `docs/runbooks/doc-gardening.md` — cleanup strategy, lifecycle, quality-score recheck.
6. `docs/runbooks/local-setup.md` — environment setup, separate from local-validation.md.
7. `docs/plans/tech-debt.md` — debt ledger from plans, FIXMEs, known issues.
8. `docs/reference/generated-artifacts.md` — build artifacts, code generation policy.
9. Update existing docs: AGENTS.md, docs/index.md, definition-of-done.md, documentation-policy.md, plan-001 status, glossary.md.
10. Create 007-harness-doc-backfill plan, validate, commit.

## Blockers

None.

## Validation Log

| Date | Check | Result |
|---|---|---|
| 2026-06-14 | `bash scripts/check.sh` | 9/9 core tests passed, fmt + clippy clean |
| 2026-06-14 | `git status --short` | 19 files staged, clean working tree after commit |

## Last Checkpoint

2026-06-14: Harness doc backfill completed and committed. All 11 missing docs created, 6 existing docs updated. Working tree clean.
