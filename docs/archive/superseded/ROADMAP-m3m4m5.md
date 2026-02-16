# NervusDB v2 Roadmap

> **Current Version**: v0.1.0-alpha (M3 - Milestone 3)  
> **Status**: ⚠️ Experimental - Core Cypher features incomplete, TCK coverage ~5%

## 🚨 Honest Assessment

This project is **NOT production-ready**. The following critical gaps exist:

- 31+ `NotImplemented` errors in query engine
- Only 1/220 TCK feature files running in CI
- Python binding incomplete
- Many Cypher clauses partially implemented

---

## Milestone Definitions

### M3 (Current) - Core Foundation ✅

**Goal**: Basic graph operations work  
**Status**: Complete but with gaps

| Category                  | Status  |
| ------------------------- | ------- |
| Storage (WAL, crash-safe) | ✅ Done |
| Basic MATCH/CREATE/DELETE | ✅ Done |
| Single-hop patterns       | ✅ Done |
| Aggregations (basic)      | ✅ Done |
| CLI working               | ✅ Done |

**Known Gaps** (blocking production use):

- Chained MERGE not supported
- Complex expressions in SET/DELETE
- Simple CASE expression
- Multiple labels in MERGE
- Anonymous nodes in patterns

---

### M4 - Cypher Completeness 🎯

**Goal**: TCK pass rate ≥ 70%, remove most `NotImplemented`  
**Target**: 2026-Q1

| ID    | Task                                            | Priority | Est.    |
| ----- | ----------------------------------------------- | -------- | ------- |
| M4-01 | Fix all `NotImplemented` in `query_api.rs`      | P0       | 2w      |
| M4-02 | Fix all `NotImplemented` in `executor.rs`       | P0       | 2w      |
| M4-03 | Complete MERGE semantics (chained, multi-label) | P0       | 1w      |
| M4-04 | Complete SET/DELETE with expressions            | P0       | 1w      |
| M4-05 | Simple CASE expression support                  | P1       | 3d      |
| M4-06 | Anonymous node handling                         | P1       | 3d      |
| M4-07 | Expand TCK harness to clauses/\*                | P0       | 1w      |
| M4-08 | Expand TCK harness to expressions/\*            | P0       | 1w      |
| M4-09 | Fix Unicode/string edge cases (ongoing)         | P1       | ongoing |

**Exit Criteria**:

- TCK pass rate ≥ 70%
- Zero P0 `NotImplemented` remaining
- All core Cypher clauses functional

---

### M5 - Polish & Performance

**Goal**: TCK pass rate ≥ 90%, Python binding stable, docs complete  
**Target**: 2026-Q2

| ID    | Task                                     | Priority | Est. |
| ----- | ---------------------------------------- | -------- | ---- |
| M5-01 | Complete Python binding with examples    | P0       | 2w   |
| M5-02 | Write comprehensive User Guide           | P0       | 1w   |
| M5-03 | Performance benchmarks vs Neo4j/Memgraph | P1       | 1w   |
| M5-04 | Concurrent read optimization             | P1       | 2w   |
| M5-05 | HNSW index tuning                        | P2       | 1w   |
| M5-06 | Expand fuzz testing (Planner/Executor)   | P1       | 1w   |
| M5-07 | GitHub Actions: auto-release binaries    | P1       | 3d   |

**Exit Criteria**:

- TCK pass rate ≥ 90%
- Python examples and docs complete
- Performance baseline published

---

### v1.0 - Production Ready

**Goal**: TCK pass rate ≥ 95%, battle-tested, community adoption  
**Target**: 2026-Q4

| ID     | Task                    | Priority | Est.    |
| ------ | ----------------------- | -------- | ------- |
| 1.0-01 | TCK pass rate ≥ 95%     | P0       | ongoing |
| 1.0-02 | Security audit          | P0       | TBD     |
| 1.0-03 | Swift/iOS binding       | P2       | TBD     |
| 1.0-04 | WebAssembly target      | P2       | TBD     |
| 1.0-05 | Real-world case studies | P1       | TBD     |

---

## Current TCK Status

| Category                 | Files | Passing | Coverage  |
| ------------------------ | ----- | ------- | --------- |
| expressions/literals     | 8     | 1       | 12.5%     |
| expressions/mathematical | 17    | 0       | 0%        |
| expressions/comparison   | 4     | 0       | 0%        |
| clauses/\*               | 50+   | 0       | 0%        |
| **Total**                | 220   | 1       | **~0.5%** |

---

## NotImplemented Inventory

From `nervusdb-query`:

| File           | Count | Key Gaps                                    |
| -------------- | ----- | ------------------------------------------- |
| `query_api.rs` | 16    | Chained MERGE, multi-label, anonymous nodes |
| `executor.rs`  | 11    | Property expressions, node/rel as values    |
| `parser.rs`    | 2     | Simple CASE, complex expressions            |

---

## How to Contribute

1. Pick a task from M4
2. Create branch `feat/M4-XX-description`
3. Implement with tests
4. Run TCK: `cargo test --test tck_harness`
5. Submit PR

See [CONTRIBUTING.md](CONTRIBUTING.md) for details.
