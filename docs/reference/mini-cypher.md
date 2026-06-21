# Mini-Cypher 0.1 Reference

Mini-Cypher is the only query language surface that counts toward NervusDB 0.1.
Existing parser or executor support outside this document is historical or
experimental until this file changes.

If current code accepts syntax outside this document, that behavior is
compatibility residue. It is not a 0.1 product promise and must not drive default
development work.

## Supported Surface

Read queries:

- `RETURN 1`
- `MATCH (n)`
- `MATCH (n:Label)`
- `MATCH (a)-[:TYPE]->(b)` for directed one-hop traversal
- `MATCH (a)-[:TYPE]->(b)-[:TYPE]->(c)` for directed two-hop examples
- `MATCH (n:Label) WHERE n.key = 'value'`
- `MATCH (n:Label) WHERE n.key = 30`
- `MATCH (a:Label)-[:TYPE]->(b) WHERE a.key = 'value'`
- `RETURN` of bound variables and simple properties
- `LIMIT`
- `EXPLAIN` for supported plans

Storage expectation:

- `MATCH (n:Label)` resolves the label ID and uses
  `GraphSnapshot::nodes_with_label(label_id)`. It is not allowed to rely only
  on full node scans as the 0.1 storage contract.

Write queries:

- `CREATE (n)`
- `CREATE (n:Label)`
- `CREATE (n:Label {key: 'value'})`
- `CREATE (a:Label {key: 'value'})-[:TYPE]->(b:Label {key: 'value'})`
- basic `DELETE` for supported node/edge paths
- basic `SET` for supported node property assignments

Filters:

- simple equality against string and integer literals
- simple parameter equality where already supported by the query API
- boolean and null equality are not part of the 0.1 contract
- conjunctions are not part of the 0.1 contract unless a future plan promotes
  them with tests

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
- named paths
- variable-length paths
- broad temporal/duration semantics
- boolean/null equality semantics
- full expression compatibility
- full openCypher edge semantics
- openCypher TCK pass rate as a success metric

## Acceptance Mapping

The core acceptance tests should map directly to this document:

- constant return
- node create and scan
- label match
- string and integer property equality filter
- one-hop traversal
- two-hop traversal
- write then reopen
- supported delete
- supported set
- supported explain

Old openCypher TCK results are compatibility evidence only. They are not the
definition of completion for this query surface.
