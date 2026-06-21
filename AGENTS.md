# NervusDB Agent Guide

NervusDB is being refactored into SQLite for property graphs: a Rust-first
embedded graph database with local directory storage, Fjall-backed crash-safe
persistence, durable graph data, and a small query surface.

## Read Order

1. `docs/index.md`
2. `docs/product/direction-contract.md`
3. `docs/product/scope-0.1.md`
4. `docs/roadmap.md`
5. `PROGRESS.md`
6. `docs/architecture/overview.md`
7. `docs/engineering/validation-policy.md`
8. `docs/plans/active/010-fjall-storage-refactor.md`

Legacy platform-era documents have been removed from the working tree. Use git
history only when historical evidence is explicitly needed; do not infer current
scope from deleted platform-era material.

## Core Rule

Every change must either move the 0.1 embedded graph core forward or cleanly
isolate non-core work.

0.1 core means Rust embedded API, local database directory storage,
Fjall-backed committed persistence, node/edge/label/property persistence, label
scans, neighbor traversal, Mini-Cypher, and CLI smoke/debug/import workflows.

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

- The change matches `docs/product/direction-contract.md` and `docs/product/scope-0.1.md`.
- The relevant focused validation passed or the skip is documented.
- Public behavior, API, storage format, validation, or architecture docs were
  updated when affected.
- Quality score, technical debt, or architecture invariants were updated when the
  change reveals a quality gap, accepted debt, or boundary violation.
- No deleted platform-era material was promoted without a new ADR.
- `git status --short` contains only intentional changes before commit.
