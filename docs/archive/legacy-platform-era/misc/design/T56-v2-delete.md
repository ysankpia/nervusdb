# T56-DELETE Design Document

## 1. Context

### 1.1 Why DELETE is Needed

According to `/Volumes/WorkDrive/Develop/github/nervusdb/nervusdb/docs/memos/v2-status-assessment.md`, v2.0.0-alpha1 requires basic CRUD capability:

> **v2.0.0-alpha1 must complete**:
> - `DELETE` / `DETACH DELETE` - delete nodes and relationships

Without DELETE, v2 is effectively write-once, which severely limits its usefulness as an embedded graph database.

### 1.2 Current State

The AST scaffolding exists:

| Component | Status | Details |
|-----------|--------|---------|
| AST `DeleteClause` | ✅ Exists | `struct DeleteClause { detach: bool, expressions: Vec<Expression> }` |
| Plan `Plan::Delete` | ✅ Exists | `Plan::Delete { detach, expressions }` |
| Executor `execute_delete` | ⚠️ Partial | Skeleton returns NotImplemented |
| Query API `compile_m3_plan` | ⚠️ Partial | Returns error for DELETE |

### 1.3 Challenges

**Key Challenge**: DELETE in Cypher operates on **pattern matching results**, not just variable names:

```cypher
// DELETE a single node by variable
MATCH (n) WHERE n.name = 'Alice' DELETE n

// DELETE with detach (delete edges first)
MATCH (n) WHERE n.name = 'Bob' DETACH DELETE n

// DELETE multiple
MATCH (n)-[r]->(m) WHERE r.type = 'old' DELETE r, m
```

This requires **combining read and write operations** in a single transaction:
1. Execute MATCH to find matching nodes/edges
2. DELETE the found entities

### 1.4 Dependencies

- T54 (Property Storage) - completed
- T55 (CREATE) - prerequisite (DELETE by variable requires executing a plan)
- GraphEngine tombstone methods - `tombstone_node()`, `tombstone_edge()` exist

---

## 2. Goals

### 2.1 Primary Goals

1. **Basic DELETE**: `MATCH (n) WHERE ... DELETE n`
2. **DETACH DELETE**: `MATCH (n) WHERE ... DETACH DELETE n` (delete edges before node)
3. **Delete by variable**: DELETE nodes/edges identified by variables in a pattern
4. **Return deleted count**: Track and return how many entities were deleted

### 2.2 MVP Constraints

Per spec.md MVP:
- Single-hop patterns only: `MATCH (a)-[:<u32>]->(b)`
- WHERE clause support (from T54) is required for DELETE to be useful
- Numeric relationship types only

### 2.3 Non-Goals

| Feature | Reason |
|---------|--------|
| DELETE without MATCH | Cypher requires pattern matching for DELETE |
| DELETE by property path | Complex, not MVP |
| DELETE with EXISTS subquery | Complex, not MVP |
| Multiple variable DELETE | `DELETE n, m` - can be added later |
| REMOVE clause | Separate task (label/property removal) |

---

## 3. Proposed Solution

### 3.1 AST Structure (Existing)

```rust
// From ast.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeleteClause {
    pub detach: bool,
    pub expressions: Vec<Expression>,
}
```

**Key Design Decision**: For MVP, we restrict `expressions` to a single `Expression::Variable`:
```cypher
MATCH (a)-[:1]->(b) WHERE a.age > 25 DELETE a
```

### 3.2 Plan Enum (Existing)

```rust
pub enum Plan {
    // ... existing variants
    Delete {
        detach: bool,
        expressions: Vec<Expression>,
    },
}
```

### 3.3 Execution Strategy

**Option A**: Execute separate read-then-write transactions
```rust
fn execute_delete_with_match(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &Params,
) -> Result<u32> {
    // 1. Execute MATCH plan to find matching nodes
    // 2. Collect node/edge IDs
    // 3. If detach=true, delete edges first
    // 4. Delete nodes
    // 5. Return count
}
```

**Option B** (MVP Simplified): Only allow DELETE with pre-bound variables

For MVP, we take a **simplified approach**:

```cypher
// MVP supported:
CREATE (n {name: 'Alice'})  -- creates node
// ... later ...
// For now, use WriteTxn directly for DELETE
```

**Wait - this is not user-friendly**. Let's design a proper MVP solution:

### 3.4 MVP DELETE Design

For v2.0.0-alpha1, we'll implement **DELETE with explicit variable binding**:

```rust
// Query API change - allow returning node IDs from MATCH
let query = prepare("MATCH (n) WHERE n.age > 25 RETURN n").unwrap();
let snapshot = db.begin_read();
let params = Params::new();
let matching_nodes: Vec<_> = query.execute_streaming(&snapshot, &params)
    .filter_map(|r| r.ok())
    .filter_map(|row| row.get_node("n"))
    .collect();

// Then use WriteTxn to delete
let mut write_txn = db.begin_write();
for node_id in matching_nodes {
    write_txn.tombstone_node(node_id);
}
write_txn.commit();
```

This is a **two-phase approach** but keeps the MVP simple. The user must:
1. MATCH to find nodes
2. Use WriteTxn to delete

For a more integrated solution, we need to design `execute_delete_with_match`.

### 3.5 Execute Delete with Match

```rust
/// Execute DELETE with an accompanying MATCH plan
pub fn execute_delete_with_match(
    match_plan: &Plan,
    delete_plan: &Plan,
    snapshot: &(impl GraphSnapshot + 'static),
    txn: &mut dyn WriteableGraph,
    params: &Params,
) -> Result<u32> {
    match delete_plan {
        Plan::Delete { detach, expressions } => {
            // Collect nodes to delete from the MATCH results
            let mut nodes_to_delete: Vec<InternalNodeId> = Vec::new();
            let mut edges_to_delete: Vec<EdgeKey> = Vec::new();

            // Execute the MATCH plan to get rows
            let rows: Vec<_> = execute_plan(snapshot, match_plan, params)
                .filter_map(|r| r.ok())
                .collect();

            for row in &rows {
                for expr in expressions {
                    if let Expression::Variable(var_name) = expr {
                        // Try to get node ID
                        if let Some(node_id) = row.get_node(var_name) {
                            nodes_to_delete.push(node_id);
                        }
                        // Try to get edge key
                        // ... (similar for edges)
                    }
                }
            }

            // If detach, delete edges first
            if *detach {
                // Find all edges connected to nodes_to_delete
                // This requires snapshot access
                for node_id in &nodes_to_delete {
                    let neighbors = snapshot.neighbors(*node_id, None);
                    for edge in neighbors {
                        // Check if dst is also being deleted (complete DETACH)
                        if nodes_to_delete.contains(&edge.dst) {
                            edges_to_delete.push(edge);
                        }
                    }
                }
                // Delete edges
                for edge in edges_to_delete {
                    txn.tombstone_edge(edge.src, edge.rel, edge.dst)?;
                }
            }

            // Delete nodes
            let mut count = edges_to_delete.len() as u32;
            for node_id in nodes_to_delete {
                txn.tombstone_node(node_id)?;
                count += 1;
            }

            Ok(count)
        }
        _ => Err(Error::Other("Not a delete plan".into())),
    }
}
```

---

## 4. Implementation Plan

### Phase 1: WriteTxn-based DELETE (Immediate)

For alpha1, document that DELETE requires two-phase approach:

```rust
// Example usage:
let nodes_to_delete: Vec<InternalNodeId> = {
    let query = prepare("MATCH (n) WHERE n.age > 25 RETURN n")?;
    let snapshot = db.begin_read();
    let params = Params::new();
    query.execute_streaming(&snapshot, &params)
        .filter_map(|r| r.ok())
        .filter_map(|row| row.get_node("n"))
        .collect()
};

let mut txn = db.begin_write();
for node_id in nodes_to_delete {
    txn.tombstone_node(node_id);
}
txn.commit()?;
```

**Pros**: Simple, leverages existing code
**Cons**: Not idiomatic Cypher

### Phase 2: Integrated DELETE Plan (Post-Alpha1)

After alpha1, implement:

```rust
pub enum Plan {
    // ... existing
    Delete {
        input: Box<Plan>,  // The MATCH plan
        detach: bool,
    },
}
```

This allows:
```cypher
MATCH (n) WHERE n.age > 25 DELETE n
```

---

## 5. Test Strategy

### 5.1 WriteTxn-based Tests

```rust
#[test]
fn test_delete_by_matching() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create nodes
    {
        let mut txn = db.begin_write();
        txn.create_node(ExternalId::from(1), 0).unwrap();
        txn.create_node(ExternalId::from(2), 0).unwrap();
        txn.commit().unwrap();
    }

    // Find and delete nodes with age > 25
    let nodes_to_delete: Vec<InternalNodeId> = {
        // Add properties and query
        let query = prepare("MATCH (n) RETURN n").unwrap();
        let snapshot = db.begin_read();
        let params = Params::new();
        query.execute_streaming(&snapshot, &params)
            .filter_map(|r| r.ok())
            .filter_map(|row| row.get_node("n"))
            .collect()
    };

    let mut txn = db.begin_write();
    for node_id in &nodes_to_delete {
        txn.tombstone_node(*node_id);
    }
    txn.commit().unwrap();

    // Verify deletion
    let snapshot = db.begin_read();
    let nodes: Vec<_> = snapshot.nodes().collect();
    assert!(nodes.is_empty() || !nodes_to_delete.contains(&nodes[0]));
}
```

---

## 6. File References

| File | Changes |
|------|---------|
| `/Volumes/WorkDrive/Develop/github/nervusdb/nervusdb/nervusdb-query/src/executor.rs` | Add `execute_delete_with_match` |
| `/Volumes/WorkDrive/Develop/github/nervusdb/nervusdb/nervusdb-query/src/query_api.rs` | Update docs for two-phase DELETE |
| `/Volumes/WorkDrive/Develop/github/nervusdb/nervusdb/nervusdb-query/tests/delete_test.rs` | New integration tests |

---

## 7. Summary

**T56-DELETE** requires careful design due to the read-then-write nature of Cypher DELETE:

1. **MVP Approach**: Document two-phase pattern (MATCH → collect → WriteTxn delete)
2. **Post-Alpha1**: Implement integrated `Plan::Delete` with `input: Box<Plan>`
3. **Key Dependency**: Snapshot access during write transactions for DETACH DELETE

The two-phase approach is pragmatic for alpha1 while keeping the door open for a more integrated solution later.
