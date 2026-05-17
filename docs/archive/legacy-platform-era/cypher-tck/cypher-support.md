# Cypher Support in NervusDB

> **Version**: v1.0 — Full Compliance
> **TCK Status**: 100% pass rate (3 897 / 3 897 scenarios)
> **Standard**: openCypher v9

NervusDB implements the openCypher query language with full TCK compliance.
Only gate-proven behavior is considered "supported."

## Compliance Summary

| Metric | Value |
|--------|-------|
| TCK scenarios | 3 897 |
| Pass rate | 100% |
| Tier-0 (smoke) | PR-blocking |
| Tier-1 (clauses) | PR-blocking |
| Tier-2 (expressions) | PR-blocking |
| Tier-3 (full) | Nightly |

## Supported Clauses

### Read Clauses

| Clause | Status | Notes |
|--------|--------|-------|
| `MATCH` | Supported | Directed, incoming, undirected patterns |
| `OPTIONAL MATCH` | Supported | Null-padded when no match |
| `RETURN` | Supported | Projection, aliasing, `DISTINCT` |
| `WITH` | Supported | Pipeline, `DISTINCT`, aggregation |
| `WHERE` | Supported | Inline and standalone filtering |
| `ORDER BY` | Supported | `ASC` / `DESC`, multi-key |
| `SKIP` / `LIMIT` | Supported | Pagination |
| `UNWIND` | Supported | Array expansion |
| `UNION` / `UNION ALL` | Supported | Result set merging |
| `CALL { ... }` | Supported | Correlated subqueries |
| `EXISTS { ... }` | Supported | Existence subqueries |
| `EXPLAIN` | Supported | Query plan output |

### Write Clauses

| Clause | Status | Notes |
|--------|--------|-------|
| `CREATE` | Supported | Nodes, relationships, properties |
| `MERGE` | Supported | `ON CREATE SET`, `ON MATCH SET` |
| `SET` | Supported | Properties and labels |
| `REMOVE` | Supported | Properties and labels |
| `DELETE` | Supported | Node and relationship deletion |
| `DETACH DELETE` | Supported | Removes relationships first |
| `FOREACH` | Supported | Iterative mutations |

### Patterns and Traversal

| Feature | Status | Notes |
|---------|--------|-------|
| Directed relationships | Supported | `(a)-[:R]->(b)` |
| Incoming relationships | Supported | `(a)<-[:R]-(b)` |
| Undirected relationships | Supported | `(a)-[:R]-(b)` |
| Variable-length paths | Supported | `*min..max` with default hop limit |
| Multi-label nodes | Supported | `(n:A:B)` |
| Named paths | Supported | `p = (a)-[*]->(b)` |

## Supported Expressions

### Literals

Integer, Float, String, Boolean, Null, List `[]`, Map `{}`.

### Operators

| Category | Operators |
|----------|-----------|
| Arithmetic | `+`, `-`, `*`, `/`, `%`, `^` |
| Comparison | `=`, `<>`, `<`, `>`, `<=`, `>=` |
| Boolean | `AND`, `OR`, `NOT`, `XOR` |
| String | `STARTS WITH`, `ENDS WITH`, `CONTAINS` |
| List | `IN`, `[]` (index), `[..]` (slice) |
| Null | `IS NULL`, `IS NOT NULL` |
| Control | `CASE WHEN ... THEN ... ELSE ... END` |

### Functions

| Category | Functions |
|----------|-----------|
| Scalar | `id()`, `type()`, `labels()`, `head()`, `last()`, `size()`, `length()`, `coalesce()`, `properties()`, `keys()` |
| String | `toString()`, `toUpper()`, `toLower()`, `trim()`, `replace()`, `split()`, `reverse()`, `substring()`, `left()`, `right()` |
| Math | `abs()`, `ceil()`, `floor()`, `round()`, `sign()`, `sqrt()`, `log()`, `rand()`, `e()`, `pi()`, `toInteger()`, `toFloat()` |
| Aggregation | `count()`, `collect()`, `min()`, `max()`, `sum()`, `avg()` |
| List | `range()`, `reduce()`, `tail()`, `nodes()`, `relationships()` |
| Path | `nodes()`, `relationships()`, `length()` |
| Type | `toInteger()`, `toFloat()`, `toString()`, `toBoolean()` |

## Known Limitations

No open engine-level core gaps are currently tracked for `left()/right()` and
`MATCH p = shortestPath((...)-[*]->(...))` parsing/execution in this document scope.

## Gate Model

NervusDB uses a tiered TCK gate system:

| Tier | Scope | Enforcement |
|------|-------|-------------|
| Tier-0 | Core/extended smoke | PR-blocking |
| Tier-1 | Clauses whitelist | PR-blocking |
| Tier-2 | Expressions whitelist | PR-blocking |
| Tier-3 | Full TCK (3 897 scenarios) | Nightly |

Scripts:
- `scripts/tck_tier_gate.sh tier0|tier1|tier2`
- `scripts/tck_full_rate.sh` — pass-rate report
- `scripts/beta_gate.sh` — threshold gate (default 95%)

## Output Model

| Platform | Format |
|----------|--------|
| CLI | NDJSON (one JSON object per line) |
| Rust | `Row` with typed `Value` enum |
| Python | Typed objects: `Node`, `Relationship`, `Path` |
| Node.js | JSON-compatible typed row values |
