# T22: Cypher Aggregate Functions

## 1. Context

Cypher aggregates were missing, preventing basic grouping queries.

## 2. Goals

- Support COUNT, SUM, AVG, MIN, MAX.
- Allow aggregation in RETURN and WITH.
- Group by non-aggregate projection items.

Non-Goals

- DISTINCT aggregates.
- Advanced numeric typing beyond existing Value types.

## 3. Solution

### 3.1 AST

Use existing FunctionCall expressions (no new AST nodes).

### 3.2 Planner

Detect aggregate expressions and insert an AggregateNode. Group keys are the
non-aggregate projection expressions.

### 3.3 Executor

Maintain per-group accumulators for each aggregate. AVG uses sum + count.
Null inputs yield Null results where appropriate.

## 4. Testing Strategy

- cypher_query_test::test_aggregate_functions

## 5. Risks

- Large group sets increase memory usage.
- Type coercion and Null handling must match expected Cypher semantics.
