# T60: v2 Variable Length Paths (多跳路径查询)

## 1. Context

### 1.1 Current State

Currently, v2 query only supports single-hop patterns:

```cypher
MATCH (a)-[:1]->(b) RETURN a, b
```

The AST supports `VariableLength` but planner and executor don't implement it.

### 1.2 Goal

Support variable-length path patterns:

```cypher
MATCH (a)-[:KNOWS*1..3]->(b) RETURN a, b
MATCH (a)-[:FRIEND*2..5]->(b) WHERE b.age > 25 RETURN a, b
```

## 2. Design

### 2.1 AST Structure (Already Exists)

```rust
pub struct RelationshipPattern {
    pub variable: Option<String>,
    pub types: Vec<String>,
    pub direction: RelationshipDirection,
    pub properties: Option<PropertyMap>,
    pub variable_length: Option<VariableLength>,  // Already exists!
}

pub struct VariableLength {
    pub min: Option<u32>,  // None = 0
    pub max: Option<u32>,  // None = unbounded
}
```

### 2.2 Physical Plan Nodes

```rust
// Existing single-hop expand
pub struct ExpandNode {
    pub input: Box<PhysicalPlan>,
    pub start_node_alias: String,
    pub rel_alias: String,
    pub end_node_alias: String,
    pub direction: RelationshipDirection,
    pub rel_type: Option<String>,
}

// New variable-length expand (DFS-based)
pub struct ExpandVariableNode {
    pub input: Box<PhysicalPlan>,
    pub start_node_alias: String,
    pub rel_alias: String,
    pub end_node_alias: String,
    pub direction: RelationshipDirection,
    pub rel_type: Option<String>,
    pub min_hops: u32,
    pub max_hops: Option<u32>,  // None = unbounded
}
```

### 2.3 Execution Strategy

**DFS-based approach** for variable-length paths:

```rust
impl Executor for ExpandVariableNode {
    fn execute<S: GraphSnapshot>(self, snapshot: &S) -> impl Iterator<Item = Result<Row>> {
        let start_nodes: Vec<_> = self.input.execute(snapshot).collect();
        
        start_nodes.into_iter().flat_map(move |start_row| {
            if let Some(start_id) = start_row.get_node(&self.start_node_alias) {
                self.dfs_expand(snapshot, start_id, 0)
                    .map(move |(path, end_id)| {
                        let mut row = start_row.clone();
                        // Set relationship variable to path
                        // Set end_node_alias to end_id
                        row
                    })
            } else {
                Box::new(std::iter::empty())
            }
        })
    }
    
    fn dfs_expand<S: GraphSnapshot>(
        &self,
        snapshot: &S,
        current_node: InternalNodeId,
        depth: u32,
    ) -> Box<dyn Iterator<Item = (Path, InternalNodeId)> {
        if let Some(max) = self.max_hops {
            if depth >= max {
                return Box::new(std::iter::empty());
            }
        }
        
        let neighbors: Vec<_> = snapshot.neighbors(current_node, self.rel_type.clone()).collect();
        
        neighbors.into_iter().flat_map(move |edge| {
            let next_node = edge.dst;
            
            // Yield this path at current depth
            let path = Path { edges: vec![edge] };
            let yield_item = (path.clone(), next_node);
            
            // Recurse
            let mut deeper = self.dfs_expand(snapshot, next_node, depth + 1);
            // Prepend current edge to all deeper paths
            
            std::iter::once(yield_item).chain(deeper)
        })
    }
}
```

### 2.4 Path Construction

For `MATCH (a)-[:KNOWS*1..3]->(b)`:

1. Scan all nodes as starting points
2. For each start node, do DFS up to 3 hops
3. Each path yields a row with:
   - `a` = start node
   - `:KNOWS*1..3` = path (list of edges) or intermediate nodes
   - `b` = end node

## 3. Implementation Plan

### Phase 1: Single-Hop Expand (1d)

1. Update `plan_pattern` to handle single-hop relationships
2. Use `ExpandNode` for `(a)-[:TYPE]->(b)` patterns
3. Test: `MATCH (a)-[:1]->(b) RETURN a, b`

### Phase 2: Variable-Length Expand (2d)

1. Add `ExpandVariableNode` to physical plan
2. Implement DFS-based executor
3. Handle `*`, `*1..3`, `*..5`, etc.
4. Test: `MATCH (a)-[:1*1..2]->(b) RETURN a, b`

### Phase 3: Path Variables (1d)

1. Support path variable: `p = (a)-[:1*]->(b)`
2. Return path as list of edges
3. Support `relationships(p)` function

### Phase 4: Testing (1d)

1. Unit tests for DFS logic
2. Integration tests for Cypher patterns
3. Performance benchmarks

## 4. Testing Strategy

### 4.1 Unit Tests

```rust
#[test]
fn test_single_hop_expand() {
    // Create: (a)-[:1]->(b)-[:1]->(c)
    // Query: MATCH (a)-[:1]->(b) RETURN a, b
    // Expect: a->b edges only
}

#[test]
fn test_variable_length_two_hops() {
    // Create: (a)-[:1]->(b)-[:1]->(c)
    // Query: MATCH (a)-[:1*1..2]->(c) RETURN a, c
    // Expect: a->b, b->c (paths of length 1 and 2)
}
```

### 4.2 Integration Tests

```rust
#[test]
fn test_match_path_variable() {
    // Query: MATCH p = (a)-[:1*]->(b) RETURN p
    // Verify path variable contains edges
}
```

## 5. File References

| File | Changes |
|------|---------|
| `nervusdb-query/src/planner.rs` | Update `plan_pattern()`, add `ExpandVariableNode` |
| `nervusdb-query/src/executor.rs` | Implement DFS for variable-length expand |
| `nervusdb-query/tests/var_length_test.rs` | New integration tests |

## 6. Risks

| Risk | Mitigation |
|------|------------|
| Unbounded paths (`*`) | Set reasonable default max (e.g., 100) or require explicit max |
| Performance degradation | Add max hop limit, consider iterative vs recursive |
| Path deduplication | Handle cycles with visited set |

## 7. Success Criteria

- [ ] Single-hop patterns use `ExpandNode`
- [ ] Variable-length patterns (`*1..3`) use `ExpandVariableNode`
- [ ] DFS algorithm correctly traverses up to max hops
- [ ] Path variables work (`p = (a)-[:*]->(b)`)
- [ ] All tests pass
- [ ] Performance acceptable (< 100ms for 10-hop queries on 10K nodes)

## 8. Dependencies

- None (self-contained)

## 9. References

- `nervusdb-query/src/ast.rs` (VariableLength struct)
- `nervusdb-query/src/planner.rs` (plan_pattern function)
- `nervusdb-query/src/executor.rs` (ExpandNode executor)
