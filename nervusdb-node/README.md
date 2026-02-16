# nervusdb-node

Node.js (N-API) bindings for [NervusDB](../README.md), a Rust-native embedded property graph database.

Provides typed return values (`NodeValue`, `RelationshipValue`, `PathValue`) and
structured error payloads (`{ code, category, message }`).

## Build

```bash
cargo build --manifest-path nervusdb-node/Cargo.toml --release
```

## Usage

```typescript
const { Db } = require("./nervusdb-node");

const db = Db.open("/tmp/mydb");

// Write (must use executeWrite or a write transaction)
db.executeWrite("CREATE (n:Person {name: 'Alice', age: 30})");

// Read
const rows = db.query("MATCH (n:Person) RETURN n.name, n.age");
console.log(rows);

// Parameterized queries
const result = db.query(
  "MATCH (n:Person) WHERE n.name = $name RETURN n",
  { name: "Alice" }
);

// Transactions
const txn = db.beginWrite();
txn.query("CREATE (a:Person {name: 'Bob'})");
txn.commit();

// Maintenance
db.createIndex("Person", "name");
db.compact();
db.checkpoint();

db.close();
```

## API Parity

All APIs are aligned with the Rust baseline. See [Binding Parity](../docs/binding-parity.md).

| Tests | Status |
|-------|--------|
| Capability tests | 109 all green |

## License

[AGPL-3.0](../LICENSE)
