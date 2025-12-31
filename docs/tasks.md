# Task Tracking (v2.0 Roadmap)

> **Focus**: Architecture Parity (Indexes), Cypher Completeness, and Ecosystem.
> **Source**: `docs/ROADMAP_2.0.md`

| ID            | Task                                                  | Risk   | Status | Branch                      | Notes                                                    |
| :------------ | :---------------------------------------------------- | :----- | :----- | :-------------------------- | :------------------------------------------------------- |
| **Phase 1**   | **Core Architecture**                                 |        |        |                             |                                                          |
| T101          | [Storage] Implement `PageCursor` & B-Tree Page Layout | High   | Done   | -                           | Slotted pages + ordered keys + cursor                    |
| T102          | [Storage] Implement `IndexCatalog` & B-Tree Logic     | High   | Done   | -                           | Insert/Search/Delete on Pager                            |
| T103          | [Storage] Compaction Integration (Merge to Index)     | High   | Done   | -                           | Prevent property loss on checkpoint                      |
| T104          | [Query] Implement `EXPLAIN` Clause                    | Low    | Done   | -                           | Show Plan visualization                                  |
| T105          | [Query] Implement `MERGE` Clause                      | Medium | Done   | -                           | Idempotent Create                                        |
| T106          | [Lifecycle] Implement Checkpoint-on-Close             | Medium | Done   | -                           | Merge WAL to NDB on shutdown                             |
| T107          | [Query] Index Integration (Optimizer V1)              | High   | Done   | feat/T107-index-integration | Connect Query to Storage IndexCatalog                    |
| T108          | [Query] Implement `SET` Clause (Updates)              | High   | Done   | feat/T108-set-clause        | Enable property updates (WAL+Index)                      |
| **Phase 1.5** | **Production Hardening (Gap Filling)**                |        |        |                             |                                                          |
| T151          | [Query] Implement `OPTIONAL MATCH` (Left Join)        | High   | Done   | feat/T151-optional-match    | Core graph pattern support                               |
| T152          | [Query] Implement Aggregation Functions (COLLECT/MIN) | Medium | Done   | feat/T152-aggregation       | Extended executor capabilities                           |
| T153          | [Query] VarLen Optional Match (Chaining)              | Medium | Done   | feat/T152-aggregation       | Handled in Gap Filling Phase                             |
| T154          | [Storage] Support Complex Types (Date/Map/List)       | High   | Done   | -                           | Extend PropertyValue & Serialization                     |
| T155          | [Storage] Implement Overflow Pages (Large Blobs)      | High   | Done   | -                           | Support properties > 8KB                                 |
| T156          | [Query] Optimizer V2 (Statistics & CBO Basics)        | High   | Done   | -                           | Histogram-based index selection                          |
| T157          | [Tool] Implement Offline Bulk Loader                  | High   | Done   | -                           | Direct SST/Page generation                               |
| T158          | [Lifecycle] Online Backup API                         | Medium | Done   | feat/T158-online-backup     | Hot snapshot capability                                  |
| **Phase 2**   | **v2.0.0 Stable Release Preparation**                 |        |        |                             |                                                          |
| T159          | [Release] v2.0.0 发布准备 (crates.io + 文档 + Binary) | High   | Done   | -                           | crates.io 5 个 crate 已发布 + GitHub Release             |
| T160          | [Docs] 完善 README 和 User Guide                      | Medium | Done   | -                           | Slogan + 快速上手 + 特性表格                             |
| T161          | [Release] GitHub Releases 二进制分发                  | Medium | Done   | -                           | Linux binary 已发布                                      |
| T162          | [Benchmark] 性能基准测试和公布                        | Medium | Done   | feat/T162-benchmark         | 5 万/10 万节点测试结果已保存                             |
| T163          | [CI] 自动化 Release CI                                | Medium | Done   | -                           | Tag 触发自动发布 Linux/macOS/Windows binary              |
| T201          | [Binding] UniFFI Setup & Python Binding               | Medium | Done   | feat/T201-python-binding    | `pip install nervusdb`                                   |
| T202          | [Tool] Bulk Import Tool (CSV/JSONL)                   | Medium | Done   | feat/T202-T203-integration  | Bulk import end-to-end + rel type regression             |
| T203          | [AI] HNSW Index Prototype                             | High   | Done    | feat/T202-T203-integration  | Persistent HNSW + vector cache; perf/GC TBD              |
| T204          | [Storage] BlobStore VACUUM (Orphan Reclaim)           | High   | Done   | feat/T202-T203-integration  | Implemented `vacuum_in_place` + CLI `v2 vacuum`          |
| T205          | [Storage] Pager Lock Granularity                      | High   | Done   | feat/T202-T203-integration  | Switched Pager to `Arc<RwLock<Pager>>` + offset IO reads |
| **Phase 3**   | **Tech Debt Resolution**                              |        |        |                             |                                                          |
| T206          | [Storage] B-Tree Incremental Delete                   | Medium | Done   | feat/T202-T203-integration  | Replace `delete_exact_rebuild` with in-place delete      |
| T207          | [Query] Executor Optimization                         | Medium | Done   | feat/T202-T203-integration  | Enum-based iterator to reduce dynamic dispatch           |
| **Phase 4**   | **Cypher Full Support**                              |        |        |                             |                                                          |
| T300          | [Query] Define “Full Cypher” Contract + TCK Gate      | High   | Plan   | feat/T300-cypher-full       | Design doc + CI gate (parse-only → exec)                 |
| T301          | [Query] Implement Arithmetic Expressions (+,-,*,/,%,^) | Medium | Plan   | feat/T301-arithmetic        | Support numeric calculations in queries                   |
| T302          | [Query] Implement String Operations (STARTS/ENDS/CONTAINS) | Medium | Plan   | feat/T302-string-ops        | Enable text search and pattern matching                   |
| T303          | [Query] Implement IN Operator                        | Low    | Plan   | feat/T303-in-operator       | Array membership testing                                  |
| T304          | [Query] Implement REMOVE Clause                      | Low    | Plan   | feat/T304-remove-clause     | Delete properties from nodes/edges                        |
| T305          | [Query] Implement WITH Clause                        | High   | Plan   | feat/T305-with-clause       | Multi-stage query pipeline                               |
| T306          | [Query] Implement UNWIND Clause                      | Medium | Plan   | feat/T306-unwind-clause     | Array expansion and iteration                            |
| T307          | [Query] Implement UNION (ALL)                        | Medium | Plan   | feat/T307-union             | Merge result sets from multiple queries                  |
| T308          | [Query] Implement CASE Expression                     | Medium | Plan   | feat/T308-case-expr         | Conditional logic in SELECT                              |
| T309          | [Query] Implement EXISTS Subquery/Operator           | Low    | Plan   | feat/T309-exists            | Pattern existence testing                                 |
| T310          | [Docs] Update cypher_support.md                      | High   | Plan   | feat/T310-docs-update       | Fix OPTIONAL MATCH and aggregation docs                   |
| T311          | [Query] Support RETURN/WITH Expressions (Projection)  | High   | Plan   | feat/T311-projection-expr   | Allow computed columns, not only variables/aggregates     |
| T312          | [Query] Expression Precedence + Unary (NOT/Negate)    | High   | Plan   | feat/T312-expr-precedence   | Full expression parser + evaluator semantics              |
| T313          | [Query] Built-in Functions (String/Math/List/Type)    | High   | Plan   | feat/T313-functions         | toUpper/substring/size/coalesce/...                       |
| T314          | [Query] Generalize Patterns (multi-hop > 3 elements)  | High   | Plan   | feat/T314-pattern-general   | Multi-hop patterns + multiple relationships               |
| T315          | [Query] Support `<-` and Undirected `-` Patterns      | High   | Plan   | feat/T315-direction         | Incoming/undirected expansion semantics                   |
| T316          | [Query] Relationship Type Alternation (`:A|B`)        | Medium | Plan   | feat/T316-type-alternation  | Parser+planner+executor support                           |
| T317          | [Query] Multiple MATCH Parts & Join Semantics         | High   | Plan   | feat/T317-joins             | Inner/left join + cartesian product rules                |
| T318          | [Query] Path Values + Path Functions                  | High   | Plan   | feat/T318-path-values       | `p=...`, length(), nodes(), relationships()               |
| T319          | [Query] CALL { ... } Subquery (Apply)                 | High   | Plan   | feat/T319-subquery          | Subquery scope + correlated apply                         |
| T320          | [Query] Procedure CALL/YIELD (NervusDB Extensions)    | High   | Plan   | feat/T320-procedures        | e.g. `CALL vector.search(...) YIELD ...`                  |
| T321          | [Storage/API] Incoming Neighbors Support              | High   | Plan   | feat/T321-incoming          | Disk format + snapshot API extension                      |
| T322          | [Storage/API] Multi-Label Model + SET/REMOVE Labels   | High   | Plan   | feat/T322-multi-label       | Storage + query semantics                                 |
| T323          | [Query] MERGE Full Semantics (ON CREATE/ON MATCH)     | High   | Plan   | feat/T323-merge-semantics   | Cypher-complete MERGE behavior                             |
| T324          | [Query] FOREACH Clause                                | Medium | Plan   | feat/T324-foreach           | Iterative updates                                         |
| T325          | [Query] Pattern Properties Rewrite (Pattern → WHERE)  | Medium | Plan   | feat/T325-pattern-props     | Accept `(n {k:v})` by lowering into predicates            |
| T326          | [CI] Integrate openCypher TCK Harness                 | High   | Plan   | feat/T326-tck               | Parse-only gate → Exec gate                               |
| T327          | [Tool] Cypher Fuzz (Parser/Planner/Executor)          | Medium | Plan   | feat/T327-fuzz              | Find panics + semantic mismatches                          |
| T328          | [Binding] Output Model Upgrade (Node/Rel/Path Values) | High   | Plan   | feat/T328-output-model      | Align CLI/Python with Cypher value semantics              |

## Archived (v1/Alpha)

_Previous tasks (T1-T67) are archived in `docs/memos/DONE.md`._
