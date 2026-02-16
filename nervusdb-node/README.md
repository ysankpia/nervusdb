# nervusdb-node

N-API binding for NervusDB local embedding.

## Current exported APIs

- `Db.open(path)`
- `db.query(cypher)`
- `db.executeWrite(cypher)`
- `db.beginWrite()`
- `db.close()`
- `WriteTxn.query(cypher)`
- `WriteTxn.commit()`
- `WriteTxn.rollback()`

## Minimal local usage

```js
const addon = require('./native/nervusdb_node.node')
const db = addon.Db.open('/tmp/nervusdb-node-demo.ndb')

db.executeWrite("CREATE (n:Person {name:'Node'})")
const rows = db.query("MATCH (n:Person) RETURN n LIMIT 1")
console.log(rows)

const txn = db.beginWrite()
txn.query("CREATE (:Person {name:'TxnNode'})")
txn.commit()

db.close()
```

For a runnable TypeScript project template, see:

- `examples/ts-local/`
