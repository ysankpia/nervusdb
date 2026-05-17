# T59: v2 Label Interning (String ↔ u32 Mapping)

## 1. Context

### 1.1 Current State

Currently, labels are `LabelId = u32`, but users must manage the mapping themselves:

```rust
// Current API - user must manage string->u32 mapping
const USER_LABEL: LabelId = 1;
let node = txn.create_node(external_id, USER_LABEL)?;
```

### 1.2 Problem

This is not user-friendly. Users expect Cypher-style labels:

```cypher
MATCH (n:User) WHERE n.age > 25 RETURN n
```

### 1.3 Goal

Implement a **Label Interner** that:
1. Maps label names (String) to `LabelId` (u32) automatically
2. Persists the mapping across restarts
3. Is efficient (O(1) lookup for hot paths)
4. Supports concurrent read transactions

## 2. Design

### 2.1 Architecture

```
┌─────────────────────────────────────────────────────┐
│                    LabelInterner                     │
├─────────────────────────────────────────────────────┤
│  s2i: HashMap<String, LabelId>  (memory, hot)      │
│  i2s: Vec<String>              (id -> name)        │
│  wal: WAL log                   (persistent)       │
│  manifest: String table         (recovery)          │
└─────────────────────────────────────────────────────┘
```

### 2.2 Operations

| Operation | Description | Consistency |
|-----------|-------------|-------------|
| `get_id(name: &str)` | Lookup label ID | Snapshot isolation |
| `get_name(id: LabelId)` | Lookup label name | Snapshot isolation |
| `get_or_create(name: &str)` | Get existing or create new | Write transaction |
| `scan_by_label(label: LabelId)` | Iterate nodes with label | Read transaction |

### 2.3 Persistence Strategy

1. **WAL Events**:
   - `CreateLabel { name: String }` → returns `LabelId`
   - Mapping stored in WAL during commit

2. **Manifest Checkpoint**:
   - Periodically dump `s2i` map to manifest
   - On recovery, rebuild `s2i` from manifest

3. **In-Memory Caching**:
   - `s2i`: HashMap for O(1) lookup
   - `i2s`: Vec for O(1) reverse lookup

### 2.4 Concurrency Model

- **Readers**: Access snapshot of `s2i` via `Arc<HashMap>`
- **Writers**: Mutate `s2i` in WAL order, copy-on-write for snapshots

## 3. API Design

### 3.1 LabelInterner Struct

```rust
pub struct LabelInterner {
    s2i: HashMap<String, LabelId>,
    i2s: Vec<String>,
    // ... persistence handles
}

impl LabelInterner {
    /// Get label ID, returns None if not found
    pub fn get_id(&self, name: &str) -> Option<LabelId>;

    /// Get label name, returns None if not found
    pub fn get_name(&self, id: LabelId) -> Option<&str>;

    /// Get or create label, returns ID
    /// Only valid within a write transaction
    pub fn get_or_create(&mut self, name: &str) -> LabelId;

    /// Number of registered labels
    pub fn len(&self) -> usize;
}
```

### 3.2 GraphEngine Integration

```rust
impl GraphEngine {
    /// Get or create a label, returns LabelId
    pub fn get_or_create_label(&mut self, name: &str) -> Result<LabelId> {
        self.label_interner.get_or_create(name)
    }

    /// Iterate nodes with specific label
    pub fn scan_by_label(&self, label: LabelId) -> impl Iterator<Item = InternalNodeId> + '_ {
        // Filter nodes by label using I2E records
    }
}
```

### 3.3 Cypher Integration

In the executor, add label filtering:

```rust
pub struct Scan {
    pub label: Option<LabelId>,  // None = all labels
    // ...
}

impl Executor for Scan {
    fn execute<S: GraphSnapshot>(self, snapshot: &S) -> impl Iterator<Item = Result<Row>> {
        let label_id = self.label.map(|name| {
            snapshot.get_label_id(&name).expect("label must exist")
        });

        snapshot.nodes()
            .filter(move |node| {
                label_id.map_or(true, |lid| node.label == lid)
            })
            .map(|node| Row::with("n", Value::Node(node.id)))
    }
}
```

## 4. Implementation Plan

### Phase 1: Core Interner (2d)

1. Create `label_interner.rs` in `nervusdb-storage`
2. Implement `LabelInterner` struct with HashMap + Vec
3. Add `get_id()`, `get_name()`, `get_or_create()`
4. Add WAL event serialization

### Phase 2: Persistence (1d)

1. Add label creation to WAL
2. Add label table to manifest
3. Implement recovery (rebuild from manifest)
4. Add `scan_by_label()` to `GraphEngine`

### Phase 3: Query Integration (1d)

1. Update `CreateNode` to auto-create labels
2. Update AST to accept label strings
3. Update planner to resolve label names
4. Add executor filter for labels

### Phase 4: Testing (1d)

1. Unit tests for `LabelInterner`
2. Integration tests for `MATCH (n:Label)`
3. Persistence tests (crash/recovery)

## 5. Testing Strategy

### 5.1 Unit Tests

```rust
#[test]
fn test_label_interner_basic() {
    let mut interner = LabelInterner::new();
    let id1 = interner.get_or_create("User");
    let id2 = interner.get_or_create("User");  // Same
    assert_eq!(id1, id2);
    assert_eq!(interner.get_name(id1), Some("User"));
}

#[test]
fn test_label_interner_multiple() {
    let mut interner = LabelInterner::new();
    let user = interner.get_or_create("User");
    let post = interner.get_or_create("Post");
    assert_ne!(user, post);
}
```

### 5.2 Integration Tests

```rust
#[test]
fn test_match_by_label() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create nodes with different labels
    let mut txn = db.begin_write();
    let user_id = txn.create_node(1, "User").unwrap();
    let post_id = txn.create_node(2, "Post").unwrap();
    txn.commit().unwrap();

    // Query only User nodes
    let rows: Vec<_> = query_collect(
        &db.snapshot(),
        "MATCH (n:User) RETURN n",
        &Default::default(),
    ).unwrap();

    assert_eq!(rows.len(), 1);
}
```

## 6. File References

| File | Changes |
|------|---------|
| `nervusdb-storage/src/label_interner.rs` | New file |
| `nervusdb-storage/src/engine.rs` | Add `LabelInterner` field, `get_or_create_label()`, `scan_by_label()` |
| `nervusdb-storage/src/wal.rs` | Add `CreateLabel` WAL event |
| `nervusdb-storage/src/manifest.rs` | Add label table serialization |
| `nervusdb-api/src/lib.rs` | Update `GraphSnapshot` trait |
| `nervusdb-query/src/planner.rs` | Resolve label strings to IDs |
| `nervusdb-query/src/executor.rs` | Add label filter in Scan |

## 7. Risks

| Risk | Mitigation |
|------|------------|
| Label name collision | Validate on creation, use `HashMap` |
| Memory growth | Labels are bounded (usually < 1000) |
| Recovery correctness | Test WAL replay carefully |
| Concurrency | Use `Arc<HashMap>` for snapshots |

## 8. Success Criteria

- [ ] `LabelInterner` implemented with HashMap + Vec
- [ ] WAL persistence for label creation
- [ ] `GraphEngine::get_or_create_label()` works
- [ ] `GraphEngine::scan_by_label()` returns nodes with label
- [ ] Cypher `MATCH (n:Label)` works
- [ ] Crash recovery preserves labels
- [ ] All tests pass

## 9. Dependencies

- None (self-contained)

## 10. References

- `nervusdb-storage/src/idmap.rs` (similar persistence pattern)
- `nervusdb-storage/src/wal.rs` (WAL event format)
- `nervusdb-query/src/ast.rs` (Label syntax)
