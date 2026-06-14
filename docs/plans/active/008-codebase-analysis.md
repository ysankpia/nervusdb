# Plan 008: Codebase Analysis

## Status

Completed

## Goal

Produce a comprehensive, evidence-based codebase analysis document using
CodeGraph, covering workspace structure, crate boundaries, test/script/CI
landscape, pain points with locations, and prioritized next steps.

## Scope

- Use `codegraph explore` and `codegraph node` across all 7 workspace crates.
- Document crate-by-crate structure, module responsibilities, and key types.
- Catalog all test files (core vs historical), scripts (default vs manual), and
  CI workflows (default vs nightly).
- Identify and rank pain points by priority.
- Produce actionable next steps organized into phases.

## Not In Scope

- Rust implementation changes.
- Creating execution plans for the recommended phases.
- Running validation scripts beyond what the analysis covers.

## CodeGraph Usage

10 exploration calls covering:

1. Workspace dependency graph
2. Storage layer (engine, pager, wal, recovery, snapshot)
3. Query layer (parser, planner, executor, Mini-Cypher)
4. Facade public API
5. CLI structure
6. Integration test landscape
7. Script landscape
8. `nervusdb-api` traits
9. `nervusdb/src/lib.rs` (full node)
10. `nervusdb-cli/src/main.rs` (full node)

## Steps

1. Explore workspace structure and crate dependency graph.
2. Explore storage layer architecture and key files.
3. Explore query layer parser/planner/executor.
4. Explore facade public API surface.
5. Explore CLI structure and subcommands.
6. Catalog all integration tests (core vs historical).
7. Catalog all validation scripts (default vs manual).
8. Catalog all CI workflows (default vs nightly).
9. Synthesize findings into pain points with code locations.
10. Organize recommended next steps into phases A/B/C/D.
11. Write `docs/reference/codebase-analysis.md`.
12. Update `docs/index.md` and `PROGRESS.md`.
13. Commit.

## Validation

- Document cross-references resolve (codebase-analysis.md linked from index.md).
- `git status --short` shows only intended files.

## Docs Created

- `docs/reference/codebase-analysis.md` — full analysis with 11 sections.

## Docs Updated

- `docs/index.md` — added codebase-analysis.md entry.
- `PROGRESS.md` — updated current objective, done, next, checkpoint.

## Completion Evidence

- `docs/reference/codebase-analysis.md` exists with all 8 sections populated.
- All CodeGraph exploration output verified against actual source files.
- Working tree clean.
