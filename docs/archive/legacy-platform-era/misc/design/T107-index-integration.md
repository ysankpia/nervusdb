# T107: Index Integration (Optimizer V1)

## 1. Background

Currently, NervusDB v2's storage engine supports B-Tree indexes (`IndexCatalog`, `BTreeIndex`), but the query engine does not use them. All queries use `NodeScan` (O(N)), which is unacceptable for production use.
This task aims to bridge `nervusdb-storage` and `nervusdb-query` to enable O(logN) lookups.

## 2. Goals

- Expose Index capabilities from Storage to Query.
- Update `compile_m3_plan` to detect `MATCH (n:Label {prop: val})` patterns.
- Generate `IndexScan` physical plan when an index exists.
- **Performance**: Point lookups should be O(logN).

## 3. Architecture Changes

### 3.1 Storage Layer (`nervusdb-storage`)

- **Requirement**: The `Database` struct or a Facade must expose `lookup_index(label, prop, value) -> Option<NodeId>`.
- **Current State**: `IndexCatalog` exists but might be internal.
- **Change**: ensure `IndexCatalog` is accessible via `Storage` trait impl.

### 3.2 Query Layer (`nervusdb-query`)

- **Absctraction**: The `Storage` trait (in `facade.rs`) allows the query engine to talk to storage.
- **Change**: Add `get_index_entry(&self, label: &str, field: &str, value: &Value) -> Result<Option<NodeId>>` to the `Storage` trait.

### 3.3 Optimizer (`naive_planner`)

Modify `src/query_api.rs`:

```rust
// Pseudo-code for new logic
if let Some(prop_val) = pattern.properties.get(k) {
    if storage.has_index(label, k) {
        return Plan::IndexScan { ... }
    }
}
```

### 3.4 Physical Plan (`executor.rs`)

Add `IndexScan` variant:

```rust
pub enum Plan {
    // ...
    IndexScan {
        alias: String,
        label: String,
        field: String,
        value: Value,
    }
}
```

Execution Logic:

1. Call `storage.get_index_entry(label, field, value)`.
2. If match found (NodeId), fetch Node.
3. Yield 1 Row.
4. If no match, Yield 0 Rows.

## 4. Verification Plan

- **Unit Test**: `tests/t107_optimizer_test.rs` - verify `compile_m3_plan` output.
- **Integration**: `tests/t107_index_perf.rs` - measure generic `id` lookup time vs count.
