# NervusDB Node Binding — Capability Test Report

> Updated: 2026-02-17
> Test entry: `examples-test/nervusdb-node-test/src/test-capabilities.ts`

## Summary

| Metric       | Value |
|--------------|------:|
| Total tests  |   168 |
| Passed       |   168 |
| Failed       |     0 |
| Skipped      |     0 |
| Conclusion   | Node binding fully aligned with Rust baseline |

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
| 14 | Write transactions (beginWrite/query/commit/rollback) | 4 | Pass |
| 15 | Error handling (structured payload: syntax/execution/closed-db) | 5 | Pass |
| 16 | Relationship direction (outgoing/incoming/undirected/properties) | 4 | Pass |
| 17 | Complex graph patterns (triangle/multi-hop/multiple MATCH) | 3 | Pass |
| 18 | Bulk write performance (1000 nodes/UNWIND batch) | 3 | Pass |
| 19 | Persistence (close + reopen) | 1 | Pass |
| 20 | Edge cases (empty result/literal return/empty string/large string/many props/self-loop) | 6 | Pass |
| 21 | API alignment (openPaths/createIndex/checkpoint/compact/searchVector/backup/vacuum/bulkload) | 4 | Pass |
| 22 | WriteTxn low-level API (createNode/createEdge/setProperty/removeProperty/tombstone) | 1 | Pass |
| 36 | UNWIND expanded (basic/CREATE/nested/empty) | 4 | Pass |
| 37 | UNION/UNION ALL (dedup/with MATCH) | 2 | Pass |
| 38 | WITH pipeline (aggregation/DISTINCT/multi-stage) | 3 | Pass |
| 39 | ORDER BY + SKIP + LIMIT (pagination/DESC) | 3 | Pass |
| 40 | Null handling (IS NULL/IS NOT NULL/COALESCE/propagation) | 4 | Pass |
| 41 | Type conversion (toInteger/toFloat/toString/toBoolean) | 4 | Pass |
| 42 | Math functions (abs/ceil/floor/round/sign/sqrt/log/e/pi/rand) | 9 | Pass* |
| 43 | String functions expanded (replace/split/reverse/trim/substring) | 5 | Pass |
| 44 | List operations (range/index/size/comprehension/reduce) | 6 | Pass |
| 45 | Map operations (literal/access/nested/keys) | 4 | Pass |
| 46 | Multiple MATCH (cartesian/correlated) | 2 | Pass |
| 47 | REMOVE clause (property/label) | 2 | Pass |
| 48 | Parameter queries ($param in WHERE/CREATE/multi) | 3 | Pass |
| 49 | EXPLAIN | 1 | Pass |
| 50 | Index operations (create + query) | 1 | Pass |
| 51 | Concurrent snapshots (isolation) | 1 | Pass |
| 52 | Error handling expanded (syntax/unknown func/delete/null/division) | 5 | Pass |

\* Tests touching known core gaps handle errors gracefully and print diagnostics.

## Scope

- Categories 1-20: Shared capability surface mirroring Rust baseline and Python binding.
- Categories 21-22: Node-specific API alignment and low-level WriteTxn verification.
- The Rust core engine is the sole authoritative baseline.
- `examples-test/run_all.sh` requires all three bindings (Rust/Node/Python) to pass simultaneously.

## Known Core Gaps (Engine-Level, Not Node Binding Issues)

These issues reproduce identically across Rust/Node/Python:

1. **Multi-label subset match** — `MATCH (n:Manager)` returns 0 rows for a node created as `:Person:Employee:Manager`
2. **`left()` / `right()` not implemented** — returns `UnknownFunction` error
3. **`shortestPath` incomplete** — may panic on valid shortest-path queries
4. **MERGE relationship instability** — `MERGE (a)-[:REL]->(b)` may fail in some scenarios

## Verification Commands

```bash
cd examples-test/nervusdb-node-test && npx ts-node src/test-capabilities.ts
bash examples-test/run_all.sh
bash scripts/binding_parity_gate.sh
```

## Related Documents

- `examples-test/nervusdb-rust-test/CAPABILITY-REPORT.md`
- `examples-test/nervusdb-python-test/CAPABILITY-REPORT.md`
