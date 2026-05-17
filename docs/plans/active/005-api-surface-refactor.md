# Plan 005: API Surface Refactor

## Status

Planned

## Goal

Make the Rust embedded API obvious and stable enough for 0.1 without first
breaking or deleting historical public functions.

## Scope

- Classify API surface as core, experimental, or maintenance.
- Improve docs around `Db::open`, `Db::open_paths`, snapshots, write
  transactions, graph persistence, traversal, and Mini-Cypher execution.
- Keep binding-facing wrappers out of the 0.1 story unless needed for build
  maintenance.
- Decide later whether experimental APIs should become feature-gated,
  `#[doc(hidden)]`, or moved.

## Not In Scope

- First-pass breaking removal.
- Stable Python, Node.js, or C API expansion.
- API additions that only serve vector, optimizer, or full-Cypher ambitions.

## Steps

1. Audit root facade docs and public examples.
2. Mark the 0.1 core API path in docs.
3. Classify maintenance/experimental APIs without removing them.
4. Add facade-level examples or tests where the core path is unclear.
5. Update README quick start if the public path changes.

## Validation

- `cargo doc -p nervusdb --no-deps` when API docs change.
- Focused facade tests or examples.
- `bash scripts/check.sh` before commit.

## Docs To Update

- `docs/architecture/api-surface.md`
- `docs/product/scope-0.1.md` if public scope changes.
- `README.md` and `README_CN.md` if quick start changes.

## Completion Evidence

Record generated-doc command, examples/tests, and any APIs left classified as
experimental.
