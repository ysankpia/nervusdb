# nervusdb (Python)

Python bindings for [NervusDB](../README.md), a Rust-native embedded property graph database.

Built with PyO3. Provides typed graph objects (`Node`, `Relationship`, `Path`) and
typed exceptions (`SyntaxError`, `ExecutionError`, `StorageError`, `CompatibilityError`).

## Installation

```bash
pip install maturin
maturin develop -m nervusdb-pyo3/Cargo.toml
```

## Usage

```python
import nervusdb

db = nervusdb.open("/tmp/mydb")

# Write (must use execute_write or a write transaction)
db.execute_write("CREATE (n:Person {name: 'Alice', age: 30})")

# Read
for row in db.query("MATCH (n:Person) RETURN n.name, n.age"):
    print(row)

# Streaming
for row in db.query_stream("MATCH (n) RETURN n LIMIT 100"):
    print(row)

# Transactions
txn = db.begin_write()
txn.query("CREATE (a:Person {name: 'Bob'})")
txn.commit()

# Maintenance
db.create_index("Person", "name")
db.compact()
db.checkpoint()

db.close()
```

## API Parity

All APIs are aligned with the Rust baseline. See [Binding Parity](../docs/binding-parity.md).

| Tests | Status |
|-------|--------|
| Capability tests | 138 all green |

## License

[AGPL-3.0](../LICENSE)
