# T25: Cypher MERGE

## 1. Context

MERGE is needed for idempotent creation without duplicating nodes or edges.

## 2. Goals

- Support MERGE for node and relationship patterns.
- Ensure the pattern is created only when no match exists.

Non-Goals

- ON CREATE / ON MATCH sub-clauses.
- MERGE mixed with other clauses in the same query.

## 3. Solution

### 3.1 AST

Reuse MergeClause with Pattern.

### 3.2 Parser

Parse MERGE <pattern>.

### 3.3 Execution

For MVP, execute MERGE as a standalone query:

1) Attempt to match the pattern.
2) If no match, create the pattern.

## 4. Testing Strategy

- cypher_query_test::test_merge_single_node
- cypher_query_test::test_merge_relationship_idempotent

## 5. Risks

- Incomplete patterns can lead to unexpected matches.
- Standalone-only behavior is a limitation for complex queries.
