# NervusDB Agent Guide

NervusDB is being refactored into SQLite for property graphs: a Rust-first
embedded graph database with local files, WAL recovery, persistent graph data,
and a small query surface.

## Read Order

1. `docs/index.md`
2. `docs/product/scope-0.1.md`
3. `docs/architecture/overview.md`
4. `docs/engineering/validation-policy.md`
5. The relevant active plan under `docs/plans/active/`

Archived platform-era documents are evidence only. Do not use them to infer
current scope unless a current ADR promotes that material.

## Core Rule

Every change must either move the 0.1 embedded graph core forward or cleanly
isolate non-core work.

0.1 core means Rust embedded API, local file storage, WAL/crash recovery,
node/edge/label/property persistence, label scans, neighbor traversal,
Mini-Cypher, and CLI smoke/debug/import workflows.

## Frozen Before 0.1

Do not expand full openCypher, procedures, subqueries, pattern comprehension,
stable Python/Node/C APIs, vector/HNSW defaults, advanced optimizer work, or
TCK/perf/fuzz/chaos/soak/release gates as default requirements.

## Refactor Workflow

For non-trivial refactors:

1. Read the relevant active plan.
2. Identify the touched layer: storage, query, API, CLI, docs, or CI.
3. Make the smallest coherent change.
4. Update architecture, reference, engineering, or plan docs in the same change.
5. Run the smallest validation that proves the touched boundary.

## Validation

Default:

```bash
bash scripts/check.sh
```

Docs-only changes do not need Rust tests unless code examples changed. Full
workspace verification is manual:

```bash
bash scripts/workspace_full_test.sh
```

Never hide full test fan-out behind `quick`, `check`, `pre-commit`, or
`pre-push`.

## Done

- The change matches `docs/product/scope-0.1.md`.
- The relevant focused validation passed or the skip is documented.
- Public behavior, API, storage format, validation, or architecture docs were
  updated when affected.
- No archived platform-era material was promoted without a new ADR.
- `git status --short` contains only intentional changes before commit.
