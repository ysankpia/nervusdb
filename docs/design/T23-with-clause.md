# T23: Cypher WITH Clause

## 1. Context

Without WITH, multi-stage pipelines and intermediate projections were not
possible.

## 2. Goals

- Support WITH projections with optional WHERE.
- Allow DISTINCT, ORDER BY, SKIP, LIMIT inside WITH.
- Define a new variable scope for downstream clauses.

Non-Goals

- Complex subquery import/export semantics.
- Advanced optimization across WITH boundaries.

## 3. Solution

### 3.1 AST

Reuse WithClause and WithItem definitions.

### 3.2 Parser

Parse WITH items, optional WHERE, DISTINCT, ORDER BY, SKIP, LIMIT.

### 3.3 Planner

Treat WITH as a pipeline:

1) Apply pending WHERE.
2) Project WITH items.
3) Apply WITH's own WHERE.
4) Apply DISTINCT, ORDER BY, SKIP, LIMIT.

## 4. Testing Strategy

- cypher_query_test::test_with_clause

## 5. Risks

- Alias inference and shadowing can cause confusing results if not consistent.
- Order-of-operations must match Cypher semantics.
