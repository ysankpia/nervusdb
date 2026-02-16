# nervusdb

An embeddable property graph database written in Rust with Python bindings.

## Installation

```bash
pip install nervusdb
```

## Usage

```python
import nervusdb

# Open or create a database
db = nervusdb.open("my_graph.ndb")

# Create nodes (write statements must use execute_write or transaction APIs)
db.execute_write("CREATE (n:Person {name: 'Alice', age: 30})")

# Query the graph
result = db.query("MATCH (n:Person) RETURN n")
for row in result:
    print(row)

# Close the database
db.close()
```

If you run a write statement through `db.query(...)`, NervusDB raises
`ExecutionError` (for example: `CREATE must be executed via execute_write`).

Local runnable smoke example:

- `examples/py-local/smoke.py`

## License

MIT
