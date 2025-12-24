# T21: Cypher ORDER BY + SKIP

## 1. Context

ORDER BY and SKIP were parsed as NotImplemented, leaving queries without sorting or pagination.

## 2. Goals

- Support ORDER BY <expr> [ASC|DESC], ...
- Support SKIP <n>
- Keep behavior consistent for RETURN and WITH.

Non-Goals

- Window functions or cursor-based pagination.
- Streaming sort without materialization.

## 3. Solution

### 3.1 AST

Reuse OrderByClause/OrderByItem and the skip field on ReturnClause/WithClause.

### 3.2 Parser

Parse ORDER BY items and optional SKIP in RETURN/WITH.

### 3.3 Planner

Insert SortNode before SkipNode/LimitNode in the pipeline.

### 3.4 Executor

- SortNode materializes rows, evaluates sort keys, and sorts by key + direction.
- SkipNode drops the first N rows and yields the rest.

## 4. Testing Strategy

- query::parser ORDER BY tests
- cypher_query_test::test_order_by_and_skip

## 5. Risks

- Sorting requires materialization and may increase memory use.
- Comparison semantics for Null and mixed types must be deterministic.
