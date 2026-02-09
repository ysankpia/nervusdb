# nervusdb-node (scaffold)

N-API scaffold for NervusDB bindings (M5-01).

Current exported APIs:

- `Db.open(path)`
- `db.query(cypher)`
- `db.execute_write(cypher)`
- `db.begin_write()`
- `db.close()`
- `WriteTxn.query(cypher)`
- `WriteTxn.commit()`
- `WriteTxn.rollback()`

This scaffold is intentionally minimal and will be hardened with contract tests.
