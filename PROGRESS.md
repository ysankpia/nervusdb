# PROGRESS

## Current Objective

Decisive slimming: delete all non-0.1-core code — HNSW, bindings, full openCypher,
historical tests, CI noise, scripts noise, experimental APIs — to make the
project finishable.

## Active Plan

009-slim-to-0.1

## Current Phase

Planning — slimming plan created, direction contract updated.

## Now

Execute the slimming plan (see `docs/plans/slimming-plan.md`).

## Done

- 001 to 008 — Harness normalization, core classification, refactors, examples,
  doc backfill, codebase analysis.
- Direction contract updated with explicit "Deleted" section.
- Slimming plan written with `git rm` commands for every file to delete.

## Next

1. Delete HNSW/vector (`nervusdb-storage/src/index/hnsw/`, engine.rs cleanup)
2. Delete bindings (nervusdb-pyo3, nervusdb-capi, nervusdb-node, examples-test)
3. Delete historical tests (~35 files)
4. Delete CI workflows (10 files)
5. Delete historical scripts (31 files)
6. Strip non-Mini-Cypher query code (AST variants, executor files)
7. Clean facade exports (backup/bulkload/vacuum)
8. Clean examples (py-local, ts-local)
9. Clean杂物 (fuzz, Makefile, lefthook)
10. Validate + commit

## Blockers

None. Decision is made — execute.

## Validation Log

| Date | Check | Result |
|---|---|---|
| 2026-06-14 | `bash scripts/check.sh` | 9/9 core tests passed, fmt + clippy clean |
| 2026-06-14 | `git status --short` | 19 files staged, clean working tree after commit |
| 2026-06-14 | CodeGraph exploration | 10 calls across 200+ files, all data extracted |
| 2026-06-14 | Slimming plan created | `docs/plans/slimming-plan.md` |

## Last Checkpoint

2026-06-14: Direction contract updated with "Explicitly Deleted" section.
Slimming plan written with per-file `git rm` commands.
