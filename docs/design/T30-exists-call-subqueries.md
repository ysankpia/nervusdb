# T30: EXISTS / CALL Subqueries

## 1. Context

EXISTS predicates and subquery execution improve expressiveness without adding
full procedure support.

## 2. Goals

- Support EXISTS pattern predicates.
- Support EXISTS { <subquery> }.
- Support standalone CALL { <subquery> }.

Non-Goals

- CALL procedure/YIELD.
- Mixing CALL with other clauses.

## 3. Solution

### 3.1 Parser

Parse EXISTS(pattern) and EXISTS { subquery } into ExistsExpression.
Parse CALL { subquery } as CallClause.

### 3.2 Execution

- EXISTS(pattern): evaluate by attempting to match the pattern.
- EXISTS { subquery }: execute a restricted subquery (single MATCH only).
- CALL { subquery }: only allowed as a standalone query; dispatch to subquery.

## 4. Testing Strategy

- cypher_query_test::test_exists_pattern_predicate_filters_rows
- cypher_query_test::test_exists_subquery_filters_rows
- cypher_query_test::test_call_subquery_standalone_returns_results

## 5. Risks

- Subquery restrictions must be explicit to avoid silent misbehavior.
- EXISTS with OPTIONAL MATCH is intentionally rejected.
