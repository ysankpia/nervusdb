# Plan 004: Query Core Refactor

## Status

In progress

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

## Current Audit

| Mini-Cypher form | Current evidence | Gap | 0.1 status |
|---|---|---|---|
| `RETURN 1` | `core_0_1_return_one` | None | Proven |
| `MATCH (n)` | Older node-scan tests and planner docs | Add explicit core acceptance test | Implemented but weakly proven |
| `MATCH (n:Label)` | `core_0_1_label_scan_property_filter_and_limit` | None | Proven |
| `MATCH (a)-[:TYPE]->(b)` | `core_0_1_one_hop_and_two_hop_traversal` | Keep direction and relationship type in core assertion | Proven |
| `MATCH (a)-[:TYPE]->(b)-[:TYPE]->(c)` | `core_0_1_one_hop_and_two_hop_traversal` | Happy path only; enough for 0.1 examples | Proven |
| String property equality | `core_0_1_label_scan_property_filter_and_limit` | None | Proven |
| Integer property equality | Historical filter tests | Add explicit core acceptance test | Implemented but weakly proven |
| Boolean/null equality | Historical expression support may exist | Do not promote before 0.1 | Out of 0.1 / Frozen |
| `RETURN` variables and simple properties | Existing `RETURN n` and `RETURN b.name` core tests | None | Proven |
| `LIMIT` | Existing core tests use `LIMIT` | Add `LIMIT 0` and cap evidence | Implemented but weakly proven |
| Basic `CREATE` node | `core_0_1_basic_create_set_delete_and_explain` | None | Proven |
| Basic `CREATE` edge | Historical create/storage tests | Add explicit core acceptance test | Implemented but weakly proven |
| Basic `SET` | `core_0_1_basic_create_set_delete_and_explain` | None | Proven |
| Basic `DELETE` | `core_0_1_basic_create_set_delete_and_explain` | Only basic node delete; enough for 0.1 | Proven |
| `EXPLAIN` | `core_0_1_basic_create_set_delete_and_explain` | None | Proven |
| `OPTIONAL MATCH`, `WITH`, `UNION`, `UNWIND`, aggregation, procedures, subqueries, pattern comprehension | Historical tests and openCypher material | Compatibility evidence only | Out of 0.1 / Frozen |

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

- `docs/reference/mini-cypher.md` is the Mini-Cypher 0.1 contract.
- `nervusdb/tests/core_0_1_mini_cypher.rs` maps accepted query forms to tests.
- Advanced query tests and openCypher TCK material are documented as
  compatibility evidence, not default 0.1 acceptance.
- `bash scripts/check.sh` passes without adding full test fan-out.
