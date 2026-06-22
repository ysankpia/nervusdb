# Mini-Cypher 0.1 Reference

Mini-Cypher is the only query language surface that counts toward NervusDB 0.1.
The main parser, planner, and executor path must reject syntax outside this
document. Future expansion requires an ADR, tests, and documentation updates.

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
- `MATCH (n:Label) WHERE n.key = scalar_literal` and
  `MATCH (n:Label {key: scalar_literal})` may use
  `GraphSnapshot::nodes_with_label_and_property(label_id, key, value)` as an
  exact-match anchor. Remaining predicates still run through the normal filter
  path.

Write queries:

- `CREATE (n)`
- `CREATE (n:Label)`
- `CREATE (n:Label {key: 'value'})`
- `CREATE (a:Label {key: 'value'})-[:TYPE]->(b:Label {key: 'value'})`
- basic `DELETE` for supported node/edge paths
- plain `DELETE n` rejects connected nodes; use `DETACH DELETE n` when deleting
  a node should also remove its relationships
- basic `SET n.key = value` for supported node or edge property assignments

Filters:

- simple equality against string and integer literals
- simple parameter equality where already supported by the query API
- scalar label-qualified property equality may be index-backed
- boolean and null equality are not part of the 0.1 contract
- conjunctions are not part of the 0.1 contract unless a future plan promotes
  them with tests

## Frozen Surface

Do not add or promote these before 0.1:

- `OPTIONAL MATCH`
- `WITH`
- `UNION`
- `UNWIND`
- `MERGE`
- `FOREACH`
- `CALL`
- `REMOVE`
- `RETURN DISTINCT`
- `ORDER BY`
- `SKIP`
- aggregation
- `EXISTS`
- subqueries
- procedures
- list comprehension
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

Unsupported syntax should fail fast with an `outside Mini-Cypher 0.1` error
instead of producing an executable plan.
