# PROGRESS

## Current Objective

Slimming complete. Ship 0.1 or drive the next blocker.

## Active Plan

009-slim-to-0.1 ✅

## Current Phase

Done — slimming committed on branch `chore/slim-to-0.1`.

## Now

Ship reviews, merge to `main`, or tackle the next 0.1 P0.

## Done

- 001 to 008 — Harness normalization, core classification, refactors, examples,
  doc backfill, codebase analysis.
- Direction contract updated with explicit "Deleted" section.
- Slimming plan written with per-file `git rm` commands for every file to delete.
- **HNSW/vector deleted** — 5 files, 824 lines, 2 external deps removed
- **Bindings deleted** — pyo3, capi, node, examples-test
- **Historical tests deleted** — ~50 files across nervusdb + storage
- **CI workflows deleted** — 10 kept ci.yml
- **Scripts deleted** — 31 kept 6 core
- **Non-Mini-Cypher query code stripped** — 15 executor files, evaluator_temporal_parse
- **Facade exports cleaned** — backup, bulkload, vacuum removed
- **Examples +杂物 cleaned** — py-local, ts-local, fuzz, Makefile, lefthook
- **Tests fixed** — 9 Mini-Cypher, 1 Rust API, 6 storage all pass
- **`bash scripts/check.sh` passes** — fmt, clippy, tests all green
- **Committed** — `c55b81e9`, 218 files, 41,386 lines deleted

## Next

Merge `chore/slim-to-0.1` to `main`, or start the next active plan.

## Blockers

None.

## Validation Log

| Date | Check | Result |
|---|---|---|
| 2026-06-14 | `bash scripts/check.sh` | 9/9 core tests passed, fmt + clippy clean |
| 2026-06-14 | `git status --short` | Clean working tree, all 218 files staged |
| 2026-06-14 | CodeGraph exploration | 10 calls across 200+ files, all data extracted |
| 2026-06-14 | Slimming plan created | `docs/plans/slimming-plan.md` |
| 2026-06-14 | Slimming committed | `c55b81e9`, 218 files, 41,386 lines deleted |

## Last Checkpoint

2026-06-14: Slimming committed on `chore/slim-to-0.1`. 41,386 lines removed.
Workspace: 5 crates, 2 test files, 1 CI workflow, 6 scripts.
