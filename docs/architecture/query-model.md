# Query Model

NervusDB 0.1 targets Mini-Cypher only. The query layer may contain historical
support for broader Cypher work, but that code is not the product target before
0.1.

The current product contract is `docs/reference/mini-cypher.md`. Syntax accepted
outside that reference is compatibility residue unless a future ADR promotes it
with product scope, architecture notes, tests, and validation policy.

## Responsibilities

- Parser: accept the documented Mini-Cypher surface.
- Planner: produce simple plans for label scan, neighbor traversal, filters,
  projection, limit, write operations, and explain.
- Executor: return deterministic rows and apply supported writes against the
  storage boundary.

## 0.1 Core Path

The core query path is:

```text
query string
  -> parser
  -> simple Mini-Cypher plan
  -> executor
  -> GraphSnapshot or write transaction
```

Before 0.1, this path is optimized for correctness and predictable behavior, not
for openCypher breadth. The default acceptance suite is
`nervusdb/tests/core_0_1_mini_cypher.rs`.

## Before 0.1

Do not add new procedures, subqueries, pattern comprehension, broad aggregation,
or full openCypher edge semantics. Query work should either improve correctness
inside `docs/reference/mini-cypher.md` or isolate historical behavior.

Advanced tests for optional match, `WITH`, `UNION`, `UNWIND`, aggregation,
procedures, subqueries, pattern comprehension, and openCypher TCK material are
compatibility evidence only. They are not the 0.1 acceptance suite and are not
required in the default development loop.
