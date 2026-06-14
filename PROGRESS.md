# PROGRESS

## Current Objective

Codebase analysis: use CodeGraph to produce a comprehensive
`docs/reference/codebase-analysis.md` covering workspace structure, crate
boundaries, test/script/CI landscape, pain points, and recommended next steps.

## Active Plan

008-codebase-analysis

## Current Phase

Analysis — document created.

## Now

Review the analysis and decide which phase (A/B/C/D) to execute next.

## Done

- 001-harness-normalization — Agents.md, docs/index.md, product/architecture/engineering/runbook/reference/plan/ADR/bug docs rewritten for 0.1, legacy material archived.
- 002-core-0.1-slimdown — Core/experimental/frozen classification, CI focused on core, quick test separated from full fan-out.
- 003-storage-core-refactor — Storage invariant baseline, crash recovery tests, storage model documentation.
- 004-query-core-refactor — Mini-Cypher acceptance, query model documentation, advanced tests classified as compatibility evidence.
- 005-api-surface-refactor — Core API classification, Rust API reference, facade tests.
- 006-cli-examples-validation — CLI reference, ten runnable examples, smoke/crash/bench scripts.
- 007-harness-doc-backfill — All 11 missing harness docs created, 6 existing docs updated.
- 008-codebase-analysis — `docs/reference/codebase-analysis.md` created with CodeGraph exploration across all crates, tests, scripts, and CI.

## Next

Per analysis recommendation:
- Phase A: Feature isolation (HNSW gate, openCypher gate)
- Phase B: Test cleanup (core vs historical separation, missing tests)
- Phase C: Engineering cleanup (scripts, workspace, parser refactor)
- Phase D: 0.1 hardening (storage, crash recovery, release)

## Blockers

None.

## Validation Log

| Date | Check | Result |
|---|---|---|
| 2026-06-14 | `bash scripts/check.sh` | 9/9 core tests passed, fmt + clippy clean |
| 2026-06-14 | `git status --short` | 19 files staged, clean working tree after commit |
| 2026-06-14 | CodeGraph exploration | 10 calls across 200+ files, all data extracted |

## Last Checkpoint

2026-06-14: Codebase analysis document created at `docs/reference/codebase-analysis.md`.
