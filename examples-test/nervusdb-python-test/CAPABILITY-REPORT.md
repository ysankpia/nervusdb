# NervusDB Python Binding — Capability Test Report

> Updated: 2026-02-17
> Test entry: `examples-test/nervusdb-python-test/test_capabilities.py`

## Summary

| Metric       | Value |
|--------------|------:|
| Total tests  |   204 |
| Passed       |   204 |
| Failed       |     0 |
| Skipped      |     0 |
| Conclusion   | Python binding fully aligned with Rust baseline |

## Feature Matrix

| #  | Category | Tests | Status |
|----|----------|------:|--------|
|  1 | Basic CRUD (CREATE/MATCH/SET/DELETE/REMOVE) | 9 | Pass |
|  2 | Multi-label nodes [CORE-BUG: subset match] | 2 | Pass* |
|  3 | Data types (null/bool/int/float/string/list/map) | 9 | Pass |
|  4 | WHERE filters (equality/comparison/AND/OR/NOT/IN/STARTS WITH/CONTAINS/ENDS WITH/IS NULL) | 9 | Pass |
|  5 | Query clauses (ORDER BY/LIMIT/SKIP/WITH/UNWIND/UNION/OPTIONAL MATCH) | 10 | Pass |
|  6 | Aggregation (count/sum/avg/min/max/collect/DISTINCT/GROUP BY) | 7 | Pass |
|  7 | MERGE (node/ON CREATE/ON MATCH) [CORE-BUG: relationship] | 5 | Pass* |
|  8 | CASE expressions (simple/generic) | 2 | Pass |
|  9 | String functions (toString/toUpper/toLower/trim/size) [CORE-BUG: left/right] | 7 | Pass* |
| 10 | Math operations (arithmetic/modulo/abs/toInteger) | 4 | Pass |
| 11 | Variable-length paths [CORE-BUG: shortestPath] | 4 | Pass* |
| 12 | EXISTS subquery | 1 | Pass |
| 13 | FOREACH | 1 | Pass |
| 14 | Write transactions (begin_write/query/commit/rollback) | 4 | Pass |
| 15 | Error handling (syntax/execution/closed-db) | 5 | Pass |
| 16 | Relationship direction (outgoing/incoming/undirected) | 4 | Pass |
| 17 | Complex graph patterns (triangle/multi-hop/fan-out) | 3 | Pass |
| 18 | Bulk write performance (1000 nodes/UNWIND batch) | 3 | Pass |
| 19 | Persistence (close + reopen) | 1 | Pass |
| 20 | Edge cases (empty result/null/large string/self-loop) | 5 | Pass |
| 21 | `query_stream` behavior | 3 | Pass |
| 22 | Parameterized queries (query/execute_write with params) | 5 | Pass |
| 23 | Vector operations (set_vector/search_vector) | 4 | Pass |
| 24 | Typed objects (Node/Relationship/Path) | 4 | Pass |
| 25 | Exception hierarchy (NervusError/SyntaxError/StorageError) | 4 | Pass |
| 26 | `Db.path` + `open()` entry point | 3 | Pass |
| 27 | Python edge cases (large int/unicode/emoji/bad param type) | 5 | Pass |
| 28 | API alignment (open_paths/create_index/checkpoint/compact/backup/vacuum/bulkload) | 3 | Pass |
| 29 | WriteTxn low-level API (create/set/remove/tombstone) | 1 | Pass |
| 30 | UNWIND expanded (empty/aggregation/range/CREATE) | 5 | Pass |
| 31 | UNION/UNION ALL expanded (dedup/multi/with MATCH) | 4 | Pass |
| 32 | WITH pipeline (multi-stage/DISTINCT/aggregation) | 3 | Pass |
| 33 | ORDER BY + SKIP + LIMIT (pagination) | 3 | Pass |
| 34 | Null handling (COALESCE/propagation/IS NULL) | 5 | Pass |
| 35 | Type conversion (toInteger/toFloat/toString/toBoolean) | 7 | Pass |
| 36 | Math functions full (ceil/floor/round/sign/sqrt/log/e/pi) | 8 | Pass |
| 37 | String functions expanded (replace/lTrim/rTrim/split/reverse/substring) | 6 | Pass |
| 38 | List operations (range/index/size/comprehension/reduce) | 6 | Pass |
| 39 | Map operations (literal/access/nested/keys) | 4 | Pass |
| 40 | Multiple MATCH (cartesian/correlated/independent) | 3 | Pass |
| 41 | REMOVE clause (property/multiple) | 2 | Pass |
| 42 | Parameter queries expanded ($param in WHERE/CREATE/RETURN) | 3 | Pass |
| 43 | EXPLAIN | 1 | Pass |
| 44 | Index operations (create/update) | 2 | Pass |
| 45 | Error handling expanded (type/division/missing property) | 3 | Pass |
| 46 | Concurrent snapshot isolation | 1 | Pass |

\* Tests touching known core gaps handle errors gracefully and print diagnostics.

## Scope

- Categories 1-20: Mirrors Node.js shared capability surface.
- Categories 21-29: Python-specific capabilities (query_stream, typed objects, exception hierarchy, etc.).
- The Rust core engine is the sole authoritative baseline.
- Binding parity is enforced by `binding_parity_gate.sh`.

## Known Core Gaps (Engine-Level, Not Python Binding Issues)

These issues reproduce identically across Rust/Node/Python:

1. **Multi-label subset match** — `MATCH (n:Manager)` returns 0 rows for a node created as `:Person:Employee:Manager`
2. **`left()` / `right()` not implemented** — returns `UnknownFunction` error
3. **`shortestPath` incomplete** — may panic on valid shortest-path queries
4. **MERGE relationship instability** — `MERGE (a)-[:REL]->(b)` may fail in some scenarios

## Verification Commands

```bash
python examples-test/nervusdb-python-test/test_capabilities.py
bash examples-test/run_all.sh
bash scripts/binding_parity_gate.sh
```

## Related Documents

- `examples-test/nervusdb-rust-test/CAPABILITY-REPORT.md`
- `examples-test/nervusdb-node-test/CAPABILITY-REPORT.md`
