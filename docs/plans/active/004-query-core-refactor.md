# Plan 004: Query Core Refactor

## Status

Planned

## Goal

Make Mini-Cypher the only query target for 0.1 and stop full-Cypher drift from
driving parser, planner, executor, and test work.

## Scope

- Keep `docs/reference/mini-cypher.md` as the supported query contract.
- Map core acceptance tests to that reference.
- Improve deterministic behavior for one-hop and two-hop traversal, simple
  filters, writes, deletes, sets, limits, and explain.
- Reclassify advanced query tests as historical or compatibility evidence when
  they are not part of the 0.1 surface.

## Not In Scope

- New procedures, subqueries, pattern comprehension, optional match, union,
  unwind, broad aggregation, or full openCypher edge semantics.
- Chasing openCypher TCK pass rate as a product success metric.
- Advanced optimizer expansion before correctness is boring.

## Steps

1. Audit `nervusdb-query` entry points against the Mini-Cypher reference.
2. Keep or add focused core acceptance tests.
3. Isolate advanced compatibility tests from the default development loop.
4. Refactor parser/planner/executor only where the core contract needs it.
5. Update query model and reference docs in the same change.

## Validation

- `bash scripts/workspace_quick_test.sh`.
- Targeted query tests for changed behavior.
- `bash scripts/check.sh` before commit.

## Docs To Update

- `docs/architecture/query-model.md`
- `docs/reference/mini-cypher.md`
- `docs/engineering/testing-strategy.md` if default query validation changes.

## Completion Evidence

Record accepted query forms, test names, and any frozen advanced behavior.
