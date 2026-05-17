# Mini-Cypher 0.1 Reference

Mini-Cypher is the only query language surface that counts toward NervusDB 0.1.
Existing parser or executor support outside this document is historical or
experimental until this file changes.

## Supported Surface

Read queries:

- `RETURN 1`
- `MATCH (n)`
- `MATCH (n:Label)`
- `MATCH (a)-[:TYPE]->(b)`
- `MATCH (a)-[:TYPE]->(b)-[:TYPE]->(c)` for documented two-hop examples
- `MATCH (a)-[:TYPE]->(b) WHERE a.key = 'value'`
- `RETURN` of bound variables and simple properties
- `LIMIT`
- `EXPLAIN` for supported plans

Write queries:

- `CREATE (n)`
- `CREATE (n:Label)`
- `CREATE (n {key: 'value'})`
- `CREATE (a)-[:TYPE]->(b)`
- basic `DELETE` for supported node/edge paths
- basic `SET` where the current implementation is already stable

Filters:

- simple equality against string, integer, boolean, and null literals
- simple parameter equality where already supported by the query API
- conjunctions only when existing tests prove deterministic behavior

## Frozen Surface

Do not add or promote these before 0.1:

- `OPTIONAL MATCH`
- `WITH`
- `UNION`
- `UNWIND`
- aggregation
- subqueries
- procedures
- pattern comprehension
- broad temporal/duration semantics
- full expression compatibility
- full openCypher edge semantics

## Acceptance Mapping

The core acceptance tests should map directly to this document:

- constant return
- node create and scan
- label match
- property equality filter
- one-hop traversal
- two-hop traversal
- write then reopen
- supported delete
- supported set
- supported explain

Old openCypher TCK results are compatibility evidence only. They are not the
definition of completion for this query surface.
