# Query Model

NervusDB 0.1 targets Mini-Cypher only. The query layer may contain historical
support for broader Cypher work, but that code is not the product target before
0.1.

## Responsibilities

- Parser: accept the documented Mini-Cypher surface.
- Planner: produce simple plans for label scan, neighbor traversal, filters,
  projection, limit, write operations, and explain.
- Executor: return deterministic rows and apply supported writes against the
  storage boundary.

## Before 0.1

Do not add new procedures, subqueries, pattern comprehension, broad aggregation,
or full openCypher edge semantics. Query work should either improve correctness
inside `docs/reference/mini-cypher.md` or isolate historical behavior.

