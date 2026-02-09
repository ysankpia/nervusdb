# Cypher Support in NervusDB v2

> **Status**: Execution-gated (v2.1 close-out)
> **Contract**: `docs/specs/cypher_compatibility_v2.md`

NervusDB follows a strict rule: **only gate-proven behavior is considered supported**.

## Gate Model

- **Tier-0**: Smoke regressions (core/extended)
- **Tier-1**: Clauses whitelist gate
- **Tier-2**: Expressions whitelist gate
- **Tier-3**: Full TCK nightly (non-blocking, report artifacts)

See:

- `scripts/tck_tier_gate.sh`
- `scripts/tck_whitelist/`
- `.github/workflows/tck-nightly.yml`
- Nightly artifacts: `artifacts/tck/tier3-full.log` + `artifacts/tck/tier3-cluster.md`

## Supported Capability Families (Current)

### Clauses

- `MATCH`, `OPTIONAL MATCH`, `RETURN`, `WITH`, `UNWIND`, `UNION`, `UNION ALL`
- `CREATE`, `MERGE`, `SET`, `REMOVE`, `DELETE`, `DETACH DELETE`, `FOREACH`
- `CALL { ... }`, `CALL ... YIELD ...`, `EXISTS { ... }`

### Patterns & Traversal

- Directed / Incoming / Undirected relationships
- Variable-length patterns (`*min..max`)
- Multi-hop generalized patterns
- Multi-label node model

### Expressions & Functions

- Literals, arithmetic, comparison, boolean logic
- String ops (`STARTS WITH`, `ENDS WITH`, `CONTAINS`)
- `IN`, list/map basics, `CASE`, `EXISTS`
- Built-ins: `id`, `type`, `labels`, `size`, `coalesce`, `head`, `last`
- Aggregates: `count`, `sum`, `avg`, `min`, `max`, `collect`

## Known Non-Goals / Limitations

- Regular expressions (`=~`) are not in current scope
- List/pattern comprehensions are not in current scope
- Error code taxonomy still evolving (message-based matching remains in use)

## Output Model

- CLI: JSON/NDJSON row output
- Python: `Node` / `Relationship` / `Path` typed objects
- Node (M5-01 N-API): JSON-compatible typed row values（含运行时 smoke / contract 验证）
