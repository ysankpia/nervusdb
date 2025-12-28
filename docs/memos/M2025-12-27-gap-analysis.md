# Gap Analysis & Roadmap: Towards the "SQLite of Graph Databases"

**Date:** 2025-12-27
**Status:** Updated - v2.0.0 Released
**Target:** Aligning NervusDB v2 development with the "SQLite" vision.

> 注意：本文件是历史 gap analysis 备忘录。仓库已进入 **Scope Frozen** 收尾模式，当前单一真相来源是 `docs/spec.md` 与 `docs/memos/DONE.md`；本文件里关于“单文件/多语言绑定”等判断可能已过时。

## 1. Context: "SQLite of Graph DBs"

The vision is to be the default embedded choice for graph data. "SQLite" implies:
1.  **Zero Config**: No daemon, open a local path. (✅ Achieved; v2 is `.ndb + .wal`)
2.  **Universal**: Bindings for every language. (⚠️ Not in MVP; bindings are archived)
3.  **Reliable**: ACID, crash-safe. (✅ Achieved)
4.  **Feature Rich**: Enough Cypher to build real apps. (⚠️ MVP only, Missing advanced features)

## 2. Gap Analysis (Vs. Mature Graph DBs like Kùzu/Neo4j)

| Feature Area | Mature Product | NervusDB v2 (Current) | GAP | Status |
|:-------------|:---------------|:----------------------|:---:|:------:|
| **Data Model** |
| Property Storage | Rich Property Graph | Basic (String/i64/Bool) | CRITICAL | ✅ Done |
| Nested Types | Lists/Maps/Date | Not supported | CRITICAL | 🔴 RED |
| **Usability** |
| Label Interning | Auto String↔ID | Manual `LabelId=u32` | HIGH | 🔴 RED |
| External ID | String/Snowflake | `u64` only | HIGH | ⚠️ Plan |
| **Query Cypher** |
| Variable Length | `MATCH (n)-[*1..5]->(m)` | Fixed single-hop only | HIGH | 🔴 RED |
| OPTIONAL MATCH | Left Join semantics | Inner Join only | MEDIUM | 🔴 RED |
| Aggregation | COUNT/SUM/AVG/MIN/MAX | Not implemented | MEDIUM | 🔴 RED |
| WITH clause | Pipeline/chaining | Linear pipeline only | MEDIUM | 🔴 RED |
| UNWIND | Batch import | Not implemented | MEDIUM | 🔴 RED |
| ORDER BY | Sort results | Not implemented | MEDIUM | 🔴 RED |
| SKIP/LIMIT | Pagination | LIMIT only | MEDIUM | ⚠️ Partial |
| **Indexing** |
| Label Index | `:Label` lookup | Full scan | MEDIUM | 🔴 RED |
| Property Index | B-Tree/Hash | Not implemented | MEDIUM | 🔴 RED |
| **Ecosystem** |
| Python | `pip install` | Not available | HIGH | 🔴 RED |
| Node.js | npm package | Available | HIGH | ✅ Done |
| UniFFI | C/Java/Kotlin/Swift | Available | HIGH | ✅ Done |

## 3. v2.0.0 Completed Features

Per `docs/spec.md` 6.3:

| Feature | Cypher | Tests | Status |
|:--------|:-------|:-----:|:------:|
| Return constant | `RETURN 1` | 1 | ✅ |
| Single-hop match | `MATCH (n)-[:1]->(m)` | 9 | ✅ |
| WHERE filter | `WHERE n.prop = 'value'` | 1 | ✅ |
| CREATE node | `CREATE (n)` | 4 | ✅ |
| CREATE edge | `CREATE (a)-[:1]->(b)` | 4 | ✅ |
| DELETE node | `MATCH (n) DELETE n` | 2 | ✅ |
| DETACH DELETE | `DETACH DELETE n` | 3 | ✅ |
| LIMIT | `RETURN n LIMIT k` | 9 | ✅ |

**Total Tests**: 33+ integration tests + 8 tombstone tests + 13 storage tests

## 4. v2.1 Roadmap (Next Milestone)

### Priority 1: Usability - Label Interning
**Goal**: `MATCH (n:User)` instead of `MATCH (n)` with manual filtering
- [ ] String↔u32 interner (LSM-based, persistent)
- [ ] Automatic label creation
- [ ] Label-based scan optimization

### Priority 2: Query Power - Variable Length Paths
**Goal**: `MATCH (a)-[:KNOWS*1..3]->(b)`
- [ ] DFS/BFS operator for variable hops
- [ ] Cycle detection
- [ ] Path result construction

### Priority 3: Query Power - Aggregation
**Goal**: `RETURN count(n), sum(n.age)`
- [ ] Aggregate operator (hash-based)
- [ ] GROUP BY support
- [ ] Functions: COUNT, SUM, AVG, MIN, MAX

### Priority 4: Query Quality - ORDER BY
**Goal**: `RETURN n ORDER BY n.name SKIP 10 LIMIT 20`
- [ ] Sort operator
- [ ] SKIP clause support
- [ ] Stable sorting

### Priority 5: Ecosystem - Python Bindings
**Goal**: `pip install nervusdb`
- [ ] PyO3 integration
- [ ] pip package setup
- [ ] CI/CD pipeline

## 5. Feature Dependencies

```
Property Storage (✅ Done)
    │
    ├── Label Interning ──> Variable Length Paths ──> Aggregation
    │         │
    │         └── ORDER BY (independent)
    │
    └── Python Bindings (independent)
```

## 6. Testing Requirements

### Current Coverage (v2.0.0)
- ✅ CREATE/DELETE: 11 tests
- ✅ LIMIT boundary: 9 tests
- ✅ WHERE filter: 1 test
- ✅ Tombstone semantics: 8 tests
- ✅ Crash recovery: 3 tests
- ✅ Compaction: 1 test

### v2.1 Required Coverage
- [ ] Label-based queries: 5 tests
- [ ] Variable length paths: 8 tests
- [ ] Aggregations: 10 tests
- [ ] ORDER BY/SKIP: 6 tests
- [ ] Property indexing: 8 tests

## 7. Performance Targets

| Metric | v2.0.0 | v2.1 Target |
|:-------|:------:|:-----------:|
| Insert (edges/sec) | 212K | 200K |
| Neighbors hot (M2) | 17M | 20M |
| Neighbors cold (M2) | 13.5M | 15M |
| Label scan (10M nodes) | O(n) | O(1) with index |
| Property filter (100K results) | O(n) | O(log n) with index |

## 8. Summary

### v2.0.0 Achieved ✅
- Core kernel stable (Pager/WAL/Transactions)
- Basic Cypher CRUD (MATCH/CREATE/DELETE/WHERE/LIMIT)
- Crash-safe (tombstone/compaction/recovery)
- Performance baseline established

### v2.1 Goals 🔄
- **Usability**: Label interning, string IDs
- **Query Power**: Variable length, aggregation, ORDER BY
- **Ecosystem**: Python bindings

### Long Term Vision
- [ ] Neo4j/Kùzu feature parity (core Cypher)
- [ ] Full-text search integration
- [ ] Vector search integration
- [ ] Multi-language bindings (Python/Java/Go)
