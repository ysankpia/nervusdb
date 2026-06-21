# Non-Goals Before 0.1

These are explicitly not product goals for the 0.1 refactor:

- Replacing Neo4j.
- Reimplementing a general-purpose storage engine.
- Preserving old `.ndb/.wal` storage compatibility.
- Exposing `.ndb + .wal` as the current public file contract.
- Adding independent edge IDs.
- Supporting parallel edges.
- Adding property range indexes.
- Hashing property keys as logical identity.
- Passing full openCypher TCK.
- Implementing full Cypher semantics.
- Adding procedures, subqueries, pattern comprehension, `OPTIONAL MATCH`, broad
  aggregation, `ORDER BY/SKIP`, `WITH`, `UNION`, or `UNWIND` as core gates.
- Stabilizing Python, Node.js, or C APIs.
- Making vector/HNSW a default feature.
- Building a server or distributed database.
- Expanding the optimizer for workloads not covered by Mini-Cypher.
- Requiring fuzz, chaos, soak, perf, TCK, or release windows in the default
  development loop.

Archived work in these areas can be used as reference only. Promotion back into
core requires a new ADR and updates to product, architecture, validation, and
active plan docs.
