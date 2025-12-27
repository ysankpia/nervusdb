# T61: v2 Aggregation (COUNT/SUM/AVG)

## Overview

Implement Cypher aggregation functions for v2 query engine:
- `COUNT(expr)` or `COUNT(*)`
- `SUM(expr)`
- `AVG(expr)`

## Scope

### MVP Support
- `RETURN COUNT(n)`, `RETURN COUNT(*)`
- `RETURN SUM(n.prop)`
- `RETURN AVG(n.prop)`
- `MATCH ... RETURN COUNT(*)` with grouping

### Not in Scope
- `DISTINCT` in aggregates (follow-up)
- `MIN`/`MAX` (requires value comparison)
- `COLLECT` (requires list type)
- `WITH` aggregation (follow-up)

## Design

### Physical Plan Node

```rust
// In planner.rs
pub enum PhysicalPlan {
    // ... existing nodes
    Aggregate(AggregateNode),
}

pub struct AggregateNode {
    pub input: Box<PhysicalPlan>,
    pub group_by: Vec<String>,          // Variables to group by
    pub aggregates: Vec<AggregateItem>, // What to compute
}

pub struct AggregateItem {
    pub alias: String,
    pub function: AggregateFunction,
}

pub enum AggregateFunction {
    Count(Option<Expression>), // None for COUNT(*)
    Sum(Expression),
    Avg(Expression),
}
```

### Executor

```rust
// In executor.rs
struct AggregateIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    input: Box<dyn Iterator<Item = Result<Row>> + 'a>,
    group_by: Vec<String>,
    aggregates: Vec<AggregateItem>,
    // HashMap for grouping: key = (group_values), value = (count, sum, etc.)
    groups: HashMap<Vec<Value>, GroupAccumulator>,
}

struct GroupAccumulator {
    count: usize,
    sum: f64,
    avg_count: usize,
}

impl<'a, S: GraphSnapshot + 'a> Iterator for AggregateIter<'a, S> {
    type Item = Result<Row>;
    
    fn next(&mut self) -> Option<Self::Item> {
        // Phase 1: Collect all input rows and group them
        // Phase 2: Emit one row per group with aggregate values
    }
}
```

### Query Parsing

Extend v2 parser to support:
```cypher
MATCH (n) RETURN COUNT(n)
MATCH (n) RETURN SUM(n.age), AVG(n.age)
MATCH (n)-[:1]->(m) RETURN n.name, COUNT(m)
```

### Implementation Phases

1. **Parser**: Add `parse_aggregate_function()` to handle `COUNT`, `SUM`, `AVG`
2. **Planner**: Add `AggregateNode` to physical plan
3. **Executor**: Implement grouping and aggregation computation
4. **Tests**: Add comprehensive test cases

## API Changes

None. Aggregation is query-only.

## Testing

```rust
// Test cases
test_count_star()
test_count_nodes()
test_sum_properties()
test_avg_properties()
test_aggregation_with_group_by()
test_aggregation_no_matches()
```

## References

- v1 implementation: `nervusdb-core/src/query/planner.rs` (AggregateFunction, AggregateNode)
- v1 executor: `nervusdb-core/src/query/executor.rs` (compute_aggregate)
