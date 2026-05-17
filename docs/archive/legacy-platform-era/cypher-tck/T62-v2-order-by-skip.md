# T62: v2 ORDER BY / SKIP / LIMIT

## 1. Context

NervusDB v2 M3 currently has limited LIMIT support (embedded in MatchOut). This task implements full ORDER BY, SKIP, and LIMIT support as separate plan nodes.

## 2. Goals

- Add `OrderBy`, `Skip`, `Limit`, and `Distinct` nodes to `Plan` enum
- Implement execution logic for sorting, skipping, and limiting results
- Support `ORDER BY n.prop ASC/DESC`
- Support `SKIP n` for pagination
- Support `LIMIT n` for result count restriction
- Support `RETURN DISTINCT` for deduplication

## 3. Implementation Plan

### 3.1 Add Plan Nodes

```rust
// executor.rs

/// ORDER BY clause - sorts results by expression
OrderBy {
    input: Box<Plan>,
    items: Vec<(String, Direction)>, // (column_name, ASC|DESC)
}

/// SKIP clause - skips first n rows
Skip {
    input: Box<Plan>,
    skip: u32,
}

/// LIMIT clause - limits result count (replaces embedded limit)
Limit {
    input: Box<Plan>,
    limit: u32,
}

/// DISTINCT clause - removes duplicate rows
Distinct {
    input: Box<Plan>,
}
```

### 3.2 Implement Execution Logic

- `OrderBy`: Collect all rows, sort in-memory, then stream
- `Skip`: Skip first N rows from iterator
- `Limit`: Take first N rows from iterator
- `Distinct`: Track seen rows using HashSet

### 3.3 Update Query API

- Parse ORDER BY, SKIP, LIMIT from RETURN clause
- Build appropriate plan nodes

## 4. Testing

- `test_order_by_asc`
- `test_order_by_desc`
- `test_skip`
- `test_limit`
- `test_distinct`
- `test_order_by_skip_limit_combined`
