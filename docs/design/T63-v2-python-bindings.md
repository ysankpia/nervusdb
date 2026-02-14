# T63: v2 Python Bindings

## 1. Context

NervusDB v2.0.0 introduces a new query engine (`nervusdb-query`) with:
- Modern parser, planner, and executor
- Streaming execution model
- Property graph storage

The existing Python bindings (`bindings/python/nervusdb-py`) use v1 API. This task creates Python bindings for v2.

## 2. Goals

- Expose v2 query API to Python via UniFFI
- Provide Pythonic API similar to `nervusdb_query::facade`
- Support streaming row iteration
- Reuse existing UniFFI infrastructure in `bindings/uniffi/nervusdb-uniffi`

## 3. Non-Goals

- Don't modify v1 Python bindings (maintain backward compatibility)
- Don't implement new query features (only expose existing v2 functionality)
- Don't create async/await API (keep synchronous for MVP)

## 4. Solution

### 4.1 Architecture

```
bindings/python/nervusdb-py/
├── python/nervusdb/
│   ├── __init__.py          # Package entry
│   ├── v2.py                # v2 Query API wrapper
│   └── v1.py                # v1 API (existing)
├── tests/
│   ├── test_v2_query.py     # v2 query tests
│   └── test_v2_facade.py    # v2 facade tests

bindings/uniffi/nervusdb-uniffi/
├── src/
│   ├── lib.rs               # v1 bindings (existing)
│   └── v2_lib.rs            # NEW: v2 bindings module
└── Cargo.toml               # Add v2 feature
```

### 4.2 v2 UniFFI API

```rust
// bindings/uniffi/nervusdb-uniffi/src/v2_lib.rs

uniffi::include_scaffolding!("nervusdb");

// Re-export types from v2-query
pub use nervusdb_query as v2_query;

// v2 Database wrapper for Python
pub struct V2Database {
    path: String,
    // Will hold actual DB instance
}

// v2 Query Result (streaming)
pub struct V2QueryResult {
    // Will hold iterator state
}
```

### 4.3 Python API (v2)

```python
# bindings/python/nervusdb-py/python/nervusdb/v2.py

from nervusdb import Database as V1Database
from nervusdb import V2Database

# Example usage:
# db = V2Database("/path/to/db.ndb")
# rows = db.query("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 10")
# for row in rows:
#     print(row)
```

## 5. v2 Query API Surface

### 5.1 Database Operations

- `V2Database(path: str)` - Open v2 database
- `db.query(cypher: str, params: Optional[dict]) -> List[Row]` - Execute query
- `db.close()` - Close database

### 5.2 Row Interface

- `row.columns()` - Get column names
- `row[i]` - Get value by index
- `row['name']` - Get value by column name

### 5.3 Value Types

- `None` (NULL)
- `str` (TEXT)
- `float` (FLOAT)
- `bool` (BOOL)
- `int` (NODE_ID, LABEL_ID, REL_TYPE_ID)

## 6. Implementation Plan

### Phase 1: UniFFI v2 Module
1. Create `bindings/uniffi/nervusdb-uniffi/src/v2_lib.rs`
2. Add v2 types: `V2Database`, `V2Row`, `V2Result`
3. Implement `prepare_v2()` and `query()` methods
4. Update `Cargo.toml` with v2 feature

### Phase 2: Python Wrapper
1. Create `bindings/python/nervusdb-py/python/nervusdb/v2.py`
2. Implement Pythonic API wrapper
3. Add `__init__.py` exports

### Phase 3: Tests
1. Create `test_v2_query.py`
2. Test basic MATCH/RETURN
3. Test CREATE/DELETE
4. Test LIMIT/WHERE

## 7. Testing Strategy

- Unit tests for Python wrapper
- Integration tests with v2 storage
- Smoke tests for basic CRUD operations

## 8. Files to Modify

- `bindings/uniffi/nervusdb-uniffi/src/v2_lib.rs` (new)
- `bindings/uniffi/nervusdb-uniffi/Cargo.toml`
- `bindings/python/nervusdb-py/python/nervusdb/v2.py` (new)
- `bindings/python/nervusdb-py/python/nervusdb/__init__.py`
- `bindings/python/nervusdb-py/tests/test_v2_query.py` (new)
