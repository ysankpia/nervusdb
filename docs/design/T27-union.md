# T27: Cypher UNION / UNION ALL

## 1. Context

Combining multiple read queries is needed for practical query composition.

## 2. Goals

- Support UNION (distinct) and UNION ALL.
- Require matching return columns across unioned queries.

Non-Goals

- UNION with write clauses.
- UNION without explicit RETURN.

## 3. Solution

### 3.1 Parser

Parse UNION and UNION ALL as clause boundaries.

### 3.2 Execution

Handle UNION at the query orchestration layer:

- Execute each subquery independently.
- Validate identical return columns.
- UNION ALL concatenates; UNION deduplicates by row key.

## 4. Testing Strategy

- cypher_query_test::test_union_all_keeps_duplicates
- cypher_query_test::test_union_distinct_dedup

## 5. Risks

- Deduplication materializes results and increases memory usage.
- Schema mismatch errors must be clear and deterministic.
