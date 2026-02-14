# Task Tracking (v2.2 SQLite-Beta Board)

> **Focus**: SQLite-Beta 收敛（TCK≥95% → 7天稳定窗 → 性能封板）
> **Source of truth**: `docs/spec.md` + `docs/ROADMAP_2.0.md`

| ID            | Task                                                       | Risk   | Status | Branch                      | Notes                                                    |
| :------------ | :--------------------------------------------------------- | :----- | :----- | :-------------------------- | :------------------------------------------------------- |
| **Phase 1**   | **Core Architecture**                                      |        |        |                             |                                                          |
| T101          | [Storage] Implement `PageCursor` & B-Tree Page Layout      | High   | Done   | -                           | Slotted pages + ordered keys + cursor                    |
| T102          | [Storage] Implement `IndexCatalog` & B-Tree Logic          | High   | Done   | -                           | Insert/Search/Delete on Pager                            |
| T103          | [Storage] Compaction Integration (Merge to Index)          | High   | Done   | -                           | Prevent property loss on checkpoint                      |
| T104          | [Query] Implement `EXPLAIN` Clause                         | Low    | Done   | -                           | Show Plan visualization                                  |
| T105          | [Query] Implement `MERGE` Clause                           | Medium | Done   | -                           | Idempotent Create                                        |
| T106          | [Lifecycle] Implement Checkpoint-on-Close                  | Medium | Done   | -                           | Merge WAL to NDB on shutdown                             |
| T107          | [Query] Index Integration (Optimizer V1)                   | High   | Done   | feat/T107-index-integration | Connect Query to Storage IndexCatalog                    |
| T108          | [Query] Implement `SET` Clause (Updates)                   | High   | Done   | feat/T108-set-clause        | Enable property updates (WAL+Index)                      |
| **Phase 1.5** | **Production Hardening (Gap Filling)**                     |        |        |                             |                                                          |
| T151          | [Query] Implement `OPTIONAL MATCH` (Left Join)             | High   | Done   | feat/T151-optional-match    | Core graph pattern support                               |
| T152          | [Query] Implement Aggregation Functions (COLLECT/MIN)      | Medium | Done   | feat/T152-aggregation       | Extended executor capabilities                           |
| T153          | [Query] VarLen Optional Match (Chaining)                   | Medium | Done   | feat/T152-aggregation       | Handled in Gap Filling Phase                             |
| T154          | [Storage] Support Complex Types (Date/Map/List)            | High   | Done   | -                           | Extend PropertyValue & Serialization                     |
| T155          | [Storage] Implement Overflow Pages (Large Blobs)           | High   | Done   | -                           | Support properties > 8KB                                 |
| T156          | [Query] Optimizer V2 (Statistics & CBO Basics)             | High   | Done   | -                           | Histogram-based index selection                          |
| T157          | [Tool] Implement Offline Bulk Loader                       | High   | Done   | -                           | Direct SST/Page generation                               |
| T158          | [Lifecycle] Online Backup API                              | Medium | Done   | feat/T158-online-backup     | Hot snapshot capability                                  |
| **Phase 2**   | **v2.0.0 Stable Release Preparation**                      |        |        |                             |                                                          |
| T159          | [Release] v2.0.0 发布准备 (crates.io + 文档 + Binary)      | High   | Done   | -                           | crates.io 5 个 crate 已发布 + GitHub Release             |
| T160          | [Docs] 完善 README 和 User Guide                           | Medium | Done   | -                           | Slogan + 快速上手 + 特性表格                             |
| T161          | [Release] GitHub Releases 二进制分发                       | Medium | Done   | -                           | Linux binary 已发布                                      |
| T162          | [Benchmark] 性能基准测试和公布                             | Medium | Done   | feat/T162-benchmark         | 5 万/10 万节点测试结果已保存                             |
| T163          | [CI] 自动化 Release CI                                     | Medium | Done   | -                           | Tag 触发自动发布 Linux/macOS/Windows binary              |
| T201          | [Binding] UniFFI Setup & Python Binding                    | Medium | Done   | feat/T201-python-binding    | `pip install nervusdb`                                   |
| T202          | [Tool] Bulk Import Tool (CSV/JSONL)                        | Medium | Done   | feat/T202-T203-integration  | Bulk import end-to-end + rel type regression             |
| T203          | [AI] HNSW Index Prototype                                  | High   | Done   | feat/T202-T203-integration  | Persistent HNSW + vector cache; perf/GC TBD              |
| T204          | [Storage] BlobStore VACUUM (Orphan Reclaim)                | High   | Done   | feat/T202-T203-integration  | Implemented `vacuum_in_place` + CLI `v2 vacuum`          |
| T205          | [Storage] Pager Lock Granularity                           | High   | Done   | feat/T202-T203-integration  | Switched Pager to `Arc<RwLock<Pager>>` + offset IO reads |
| **Phase 3**   | **Tech Debt Resolution**                                   |        |        |                             |                                                          |
| T206          | [Storage] B-Tree Incremental Delete                        | Medium | Done   | feat/T202-T203-integration  | Replace `delete_exact_rebuild` with in-place delete      |
| T207          | [Query] Executor Optimization                              | Medium | Done   | feat/T202-T203-integration  | Enum-based iterator to reduce dynamic dispatch           |
| **Phase 4**   | **Cypher Full Support**                                    |        |        |                             |                                                          |
| T300          | [Query] Define “Full Cypher” Contract + TCK Gate           | High   | Done   | feat/T300-cypher-full       | Spec: `docs/specs/cypher_compatibility_v2.md`            |
| T301          | [Query] Implement Arithmetic Expressions (+,-,\*,/,%,^)    | Medium | Done   | feat/T301-arithmetic        | Support numeric calculations in queries                  |
| T302          | [Query] Implement String Operations (STARTS/ENDS/CONTAINS) | Medium | Done   | feat/T302-string-ops        | Enable text search and pattern matching                  |
| T303          | [Query] Implement IN Operator                              | Low    | Done   | feat/T303-in-operator       | Array membership testing                                 |
| T304          | [Query] Implement REMOVE Clause                            | Low    | Done   | feat/T304-remove-clause     | Delete properties from nodes/edges                       |
| T305          | [Query] Implement WITH Clause                              | High   | Done   | feat/T305-with-clause       | Multi-stage query pipeline                               |
| T306          | [Query] Implement UNWIND Clause                            | Medium | Done   | feat/T306-unwind-clause     | Array expansion and iteration                            |
| T307          | [Query] Implement UNION (ALL)                              | Medium | Done   | feat/T307-union             | Merge result sets from multiple queries                  |
| T308          | [Query] Implement CASE Expression                          | Medium | Done   | feat/T308-case-expr         | Conditional logic in SELECT                              |
| T309          | [Query] Implement EXISTS Subquery/Operator                 | Low    | Done   | feat/T309-exists            | Pattern existence testing (Parser+Evaluator)             |
| T310          | [Docs] Update cypher_support.md                            | High   | Done   | feat/T310-docs-update       | Updated based on `docs/specs/cypher_compatibility_v2.md` |
| T311          | [Query] Support RETURN/WITH Expressions (Projection)       | High   | Done   | feat/T311-projection-expr   | Allow computed columns, not only variables/aggregates    |
| T312          | [Query] Expression Precedence + Unary (NOT/Negate)         | High   | Done   | feat/T312-expr-precedence   | Full expression parser + evaluator semantics             |
| T313          | [Query] Built-in Functions (String/Math/List/Type)         | High   | Done   | feat/T313-functions         | toUpper/substring/size/coalesce/keys/type/id             |
| T314          | [Query] Generalize Patterns (multi-hop > 3 elements)       | High   | Done   | feat/T314-pattern-general   | Iterative compiler implemented                           |
| T315          | [Query] Support `<-` and Undirected `-` Patterns           | High   | Done   | feat/T315-direction         | Incoming/undirected expansion semantics                  |
| T316          | [Query] Relationship Type Alternation (`:A / B`)            | Medium | Done   | feat/T316-type-alternation  | Parser + planner + executor support                      |
| T317          | [Query] Multiple MATCH Parts & Join Semantics              | High   | Done   | feat/T317-joins             | Inner/left join + cartesian product rules                |
| T318          | [Query] Path Values + Path Functions                       | High   | Done   | feat/T318-path-values       | `p=...`, length(), nodes(), relationships()              |
| T319          | [Query] CALL { ... } Subquery (Apply)                      | High   | Done   | feat/T319-subquery          | Subquery scope + correlated apply                        |
| T320          | [Query] Procedure CALL/YIELD (NervusDB Extensions)         | High   | Done   | feat/T320-procedures        | e.g. `CALL vector.search(...) YIELD ...`                 |
| T321          | [Storage/API] Incoming Neighbors Support                   | High   | Done   | feat/T321-incoming          | 100% - All tests passing                                 |
| T322          | [Storage/API] Multi-Label Model + SET/REMOVE Labels        | High   | Done   | feat/T322-multi-label       | Storage + query semantics                                |
| T323          | [Query] MERGE Full Semantics (ON CREATE/ON MATCH)          | High   | Done   | feat/T323-merge-semantics   | Cypher-complete MERGE behavior                           |
| T324          | [Query] FOREACH Clause                                     | Medium | Done   | -                           | Iterative updates                                        |
| T325          | [Query] Pattern Properties Rewrite (Pattern → WHERE)       | Medium | Done   | -                           | 支持 `(n {k:v})` 语法，内联属性下沉为 WHERE 谓词         |
| T326          | [CI] Integrate openCypher TCK Harness                      | High   | Done   | feat/T326-tck               | Parse-only gate → Exec gate                              |
| T327          | [Tool] Cypher Fuzz (Parser/Planner/Executor)               | Medium | Done   | feat/T327-fuzz              | Implemented `tests/fuzz_cypher.rs` with proptest         |
| T328          | [Binding] Output Model Upgrade (Node/Rel/Path Values)      | High   | Done   | feat/T328-output-model      | Align CLI/Python with Cypher value semantics             |
| T329          | [Refactor] Evaluator Snapshot Access (Fix `keys()`)        | Medium | Done   | feat/T329-eval-snapshot     | Pass Snapshot to evaluator, un-ignore `keys()` tests     |
| T330          | [Refactor] Evaluator Schema Access (Fix `type()`)          | Medium | Done   | feat/T330-eval-schema       | Pass Schema/Txn to evaluator, un-ignore `type()` tests   |
| T331          | [Bug] Fix `id()` Lookup / Node Scan Consistency            | Medium | Done   | feat/T331-fix-id-lookup     | Resolved issue with `create_node` test arguments         |
| **M4**        | **Cypher Completeness (Tiered Gate Baseline)**             |        |        |                             |                                                          |
| M4-01         | [Query] Fix NotImplemented in query_api.rs                 | High   | Done   | feat/M4-01-query-api        | 16 occurrences resolved                                  |
| M4-02         | [Query] Fix NotImplemented in executor.rs                  | High   | Done   | feat/M4-02-executor         | 11 occurrences resolved                                  |
| M4-03         | [Query] Complete MERGE Semantics                           | High   | Done   | feat/M4-03-merge            | Chained MERGE, multi-label patterns                      |
| M4-04         | [Query] SET/DELETE with Complex Expressions                | High   | Done   | feat/M4-04-expressions      | Support list/var/expressions in writes                   |
| M4-05         | [Query] Simple CASE Expression                             | Medium | Done   | feat/M4-05-case             | Parser support implemented                               |
| M4-06         | [Query] Anonymous Node Handling                            | Medium | Done   | feat/M4-06-anon-nodes       | Auto-generated variable names                            |
| M4-07         | [CI] Expand TCK to clauses/\*                              | High   | Done   | feat/M4-07-tck-clauses      | 覆盖集: `scripts/tck_whitelist/tier1_clauses.txt`；Tier-1 白名单通过 |
| M4-08         | [CI] Expand TCK to expressions/\*                          | High   | Done   | feat/M4-08-tck-expressions  | 覆盖集: `scripts/tck_whitelist/tier2_expressions.txt`；Tier-2 白名单通过 |
| M4-09         | [Bug] Ongoing Unicode/String Edge Cases                    | Medium | Done   | -                           | Fixed UTF-8 panic in explain                             |
| M4-10         | [Query/CI] Merge Core Semantics + TCK Smoke Gate          | High   | Done   | feat/M4-10-merge-core       | Added binding conflict validation + varlen `<-/->/-` + CI smoke gate |
| M4-11         | [Query] MERGE Regression Hardening                         | High   | Done   | feat/M4-10-merge-core       | Fixed MERGE execution on wrapped plans, rel source indexing, ON CREATE/ON MATCH updates, correlated MATCH binding typing |
| **M5**        | **Bindings + Docs + Perf 基础设施**                        |        |        |                             |                                                          |
| M5-01         | [Binding] Python + Node.js 可用性收敛（PyO3 + N-API）      | High   | WIP    | feat/M5-01-bindings         | 本轮新增 Compatibility 错误语义与结构化 payload，需继续补跨语言契约覆盖 |
| M5-02         | [Docs] 用户文档与支持矩阵对齐                             | High   | WIP    | feat/M5-02-user-guide       | 已切换到 Beta 收敛口径；待补 95%/7天稳定窗发布说明与日报模板 |
| M5-03         | [Benchmark] NervusDB vs Neo4j/Memgraph 对标               | Medium | WIP    | feat/M5-03-benchmark        | 已有流程；待绑定 Beta 发布 SLO 阻断 |
| M5-04         | [Performance] 并发读热点优化                               | Medium | WIP    | feat/M5-04-concurrency      | 已有基线；待收敛到 Beta P99 门槛 |
| M5-05         | [AI] HNSW 参数调优与默认策略                              | Low    | WIP    | feat/M5-05-hnsw             | 已有参数面与报告；待收敛到 recall/latency 发布门槛 |
| **Industrial**| **Industrial Quality (Nightly Gates)**                    |        |        |                             |                                                          |
| I5-01         | [Quality] `cargo-fuzz` 分层接入                            | High   | WIP    | feat/I5-01-fuzz             | 已 nightly；待接入“7天稳定窗”统一统计 |
| I5-02         | [Quality] Chaos IO 故障注入门禁                            | High   | WIP    | feat/I5-02-chaos            | 已 nightly；待接入“7天稳定窗”统一统计 |
| I5-03         | [Quality] 24h Soak 稳定性流程                              | High   | WIP    | feat/I5-03-soak             | 已 nightly；待接入“7天稳定窗”统一统计 |

| **Beta Gate** | **SQLite-Beta 必达门槛**                                   |        |        |                             |                                                          |
| BETA-01       | [Storage] 强制 `storage_format_epoch` 校验                 | High   | Done   | feat/TB1-beta-gate          | `StorageFormatMismatch` + Compatibility 映射已落地 |
| BETA-02       | [CI] Tier-3 全量通过率统计与 95% 阈值阻断                  | High   | Done   | feat/TB1-beta-gate          | `scripts/tck_full_rate.sh` + `scripts/beta_gate.sh` + nightly/manual workflow |
| BETA-03       | [TCK] 官方全量通过率冲刺至 ≥95%                            | High   | Done   | feat/TB1-tck-95             | 2026-02-14 最新 Tier-3 全量复算：`3897/3897=100.00%`（skipped 0，failed 0）；见 `artifacts/tck/beta-04-error-step-bridge-tier3-full-2026-02-14.log`、`artifacts/tck/tier3-rate-2026-02-14.md`、`artifacts/tck/tier3-cluster-2026-02-14.md`。 |
| BETA-03R1     | [Refactor] 拆分 `query_api.rs`（解析/校验/Plan 组装模块化） | High   | Done   | codex/feat/phase1b1c-bigbang | 已由 Phase 1a (R1) 覆盖完成，query_api/ 目录已拆分为多文件模块；PR #131 全门禁通过 |
| BETA-03R2     | [Refactor] 拆分 `executor.rs`（读路径/写路径/排序投影）      | High   | Done   | codex/feat/phase1b1c-bigbang | 已由 Phase 1a (R2) 覆盖完成，executor/ 目录已拆分为 34 文件；PR #131 全门禁通过 |
| BETA-03R3     | [Refactor] 拆分 `evaluator.rs` Temporal/Duration 子模块     | High   | Done   | codex/feat/phase1b1c-bigbang | 已由 Phase 1a (R3) 覆盖完成，evaluator/ 目录已拆分为 25 文件；PR #131 全门禁通过 |
| BETA-03R4     | [TCK] 重构后恢复推进（Match4/Match9 失败簇三波次）           | High   | Done   | codex/feat/phase1b1c-bigbang | 2026-02-13 主干攻坚 + Follow-up 完成：W1/W2/W3 落地（varlen 关系变量统一列表语义、`[rs*]` 受绑定关系列表约束、parser+varlen 过滤收口、复合 CREATE 管线修复、trail 去重修复），并补齐 follow-up 收口（多标签 MATCH 过滤、`[:T|:T]` parser 去重、`length()` 参数类型校验、`null` 绑定类型冲突修复、TCK 标签顺序归一化）。`Match4`/`Match9` 非跳过场景全通过，扩展矩阵历史失败已清零。证据：`artifacts/tck/beta-03r4-match-cluster-2026-02-13.log`、`artifacts/tck/beta-03r4-followup-cluster-2026-02-13.log`、`artifacts/tck/beta-03r4-regression-matrix-2026-02-13.log`、`artifacts/tck/beta-03r4-baseline-gates-r4-2026-02-13.log`。 |
| BETA-03R5     | [TCK] 失败簇滚动清零（Temporal/Return/List/With/Map/Union） | High   | Done   | codex/feat/phase1b1c-bigbang | 2026-02-13 已清零 `Temporal2/5`、`Aggregation2`、`Return2`、`List11`、`With1/5`、`WithOrderBy1`、`Union1/2/3`、`Map1/2`，并补齐 UnknownFunction、WITH DISTINCT、UNION 校验等编译期语义。 |
| BETA-03R6     | [TCK] 失败簇滚动清零（Merge/With/Return/Graph/Skip-Limit）  | High   | Done   | codex/feat/phase1b1c-bigbang | 2026-02-13 已清零 `Merge1/2/3`、`Match8`、`Create1`、`With4`、`Return1/7`、`Graph3/4`、`ReturnSkipLimit1/2`、`Mathematical8`；见 `artifacts/tck/beta-03r6-*.log`。 |
| BETA-03R7     | [TCK] 主干攻坚（Temporal/Aggregation/Set/Remove/Create/Subquery） | High   | Done   | codex/feat/phase1b1c-bigbang | 2026-02-13 已清零 `Temporal4`、`Aggregation6`、`Remove1/3`、`Set2/4/5`、`Create3`，修复 correlated subquery 作用域回归，Tier-3 提升至 94.48%（3682/3897）。 |
| BETA-04       | [Stability] 连续 7 天主 CI + nightly 稳定窗                | High   | WIP    | feat/TB1-stability-window   | 已新增 `scripts/stability_window.sh`（按最近 N 天 `tier3-rate-YYYY-MM-DD.json` 校验 `pass_rate>=95 且 failed=0`）；2026-02-14 最新快照 `100.00%`（`3897/3897`，`failed=0`），当前累计天数不足 7 天，继续滚动积累。 |
| BETA-05       | [Perf] 大规模 SLO 封板（读120/写180/向量220 ms P99）       | High   | Plan   | feat/TB1-perf-slo           | 达标后方可发布 Beta |

### BETA-03R4 子进展（2026-02-13）
- W1：引入 `BindingKind::RelationshipList`，varlen 关系变量输出统一为 `List<Relationship>`，0-hop 命中输出 `[]`，OPTIONAL miss 保持 `null`。
- W2：支持 `[rs*]` 使用已绑定关系列表作为路径约束（方向敏感、精确序列匹配），消除 `Match9[6,7]` 的绑定冲突。
- W3：修复关系关键字解析与 varlen 属性谓词路径；补齐复合 `CREATE...WITH...UNWIND...CREATE` 写执行链；在 `MatchBoundRel` 增加路径重复边检查，清零 `Match4[4,7]`。
- W4（Follow-up 收口）：清零扩展矩阵历史失败簇：`Match1[3]`、`Match3[7,8,25]`、`Path1[1]`、`Path2[3]`、`Path3[2,3]`。
- 回归与门禁：`cargo fmt --check`、`cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`、`workspace_quick_test`、`tier0/1/2`、`binding_smoke`、`contract_smoke` 全通过（见 `artifacts/tck/beta-03r4-baseline-gates-r4-2026-02-13.log`）。

### BETA-03R5 子进展（2026-02-13）
- R5-W1：Temporal/聚合/投影链修复，清零 `Temporal2`、`Temporal5`、`Aggregation2`、`Return2`。
- R5-W2：列表与量词语义修复，清零 `List11`（`range()` 默认步长 + `sign()` + 量词聚合作用域）。
- R5-W3：WITH 与排序链修复，清零 `With5`、`With1`、`WithOrderBy1`。
- R5-W4：map/union 语义收口，清零 `Map1`、`Map2`、`Union3`。
- R5-W5：UNION 列名一致性校验补齐，清零 `Union1`、`Union2`（`DifferentColumnsInUnion`）。
- 证据日志：`artifacts/tck/beta-03r5-temporal2-2026-02-13.log`、`artifacts/tck/beta-03r5-temporal5-2026-02-13.log`、`artifacts/tck/beta-03r5-aggregation2-2026-02-13.log`、`artifacts/tck/beta-03r5-return2-2026-02-13.log`、`artifacts/tck/beta-03r5-list11-2026-02-13.log`、`artifacts/tck/beta-03r5-with1-2026-02-13.log`、`artifacts/tck/beta-03r5-with5-2026-02-13.log`、`artifacts/tck/beta-03r5-withorderby1-2026-02-13.log`、`artifacts/tck/beta-03r5-map1-2026-02-13.log`、`artifacts/tck/beta-03r5-map2-2026-02-13.log`、`artifacts/tck/beta-03r5-union3-2026-02-13.log`、`artifacts/tck/beta-03r6-union-columns-2026-02-13.log`。

### BETA-03R6 子进展（2026-02-13）
- R6-W1：失败簇刷新扫描（候选 16 个 feature），确认下一主簇为 `Merge1/2/3`（11 个非跳过失败）；次级簇为 `ReturnSkipLimit1/2`、`With4`、`Graph3/4`、`Literals8`、`Mathematical8`、`Match8`。
- R6-W2：写路径语义收口，清零 `Merge1`、`Merge2`、`Merge3`、`Match8`、`Create1` 非跳过失败；补齐 `MERGE`/`CREATE` 计划语义解耦、`ON CREATE/ON MATCH` label+property 回填、删除可见性（tombstone）过滤、写查询空结果行与 side effects 统计口径修复。
- R6-W3：编译期作用域与类型校验收口，清零 `With4`、`Return1`、`Return7`、`Literals8`、`Graph3`、`Graph4` 非跳过失败；补齐 `WITH` 非变量表达式强制别名、`RETURN *` 空作用域阻断、投影表达式变量绑定校验、`labels(path)`/`type(node)` 编译期拦截。
- R6-W4：`SKIP/LIMIT` 语义升级与列名渲染收口，清零 `ReturnSkipLimit1`、`ReturnSkipLimit2`、`Mathematical8` 非跳过失败；`SKIP/LIMIT` 从整数字面量升级为常量表达式（支持参数/函数，如 `toInteger(ceil(1.7))`），执行期统一求值并保留运行时参数错误语义；默认投影列名渲染补齐括号优先级保真。
- 证据日志：`artifacts/tck/beta-03r6-seed-cluster-2026-02-13.log`、`artifacts/tck/beta-03r6-candidate-scan-2026-02-13.log`、`artifacts/tck/beta-03r6-candidate-scan-2026-02-13.cluster.md`、`artifacts/tck/beta-03r6-precommit-merge-match8-create1-2026-02-13.log`、`artifacts/tck/beta-03r6-candidate-rescan-post-merge-2026-02-13.log`、`artifacts/tck/beta-03r6-compile-scope-cluster-2026-02-13.log`、`artifacts/tck/beta-03r6-skip-limit-cluster-2026-02-13.log`、`artifacts/tck/beta-03r6-candidate-rescan-r3-2026-02-13.log`。

### BETA-03R7 子进展（2026-02-13）
- R7-W1：定向主簇清零，`Temporal4`、`Aggregation6`、`Remove1/3`、`Set2/4/5`、`Create3` 全通过（见 `artifacts/tck/beta-03r7-w3-regression-bundle-2026-02-13.log`）。
- R7-W2：修复 correlated subquery 作用域回归：`CALL { WITH n/p ... }` 首子句注入 `subquery_seed_input`，并修复 `Plan::Apply` 输出绑定合并策略，避免外层别名被子查询 retain 覆盖。
- R7-W3：回归补强：`t301_expression_ops` 对齐 list-vs-null 比较预期；新增 `binding_analysis` 单测 `extract_output_var_kinds_apply_preserves_input_aliases` 防回归。
- Tier-3 全量复算：`3897 scenarios (3682 passed, 199 skipped, 16 failed)`，通过率 `94.48%`（见 `artifacts/tck/beta-03r7-w3-tier3-full-2026-02-13.log`、`artifacts/tck/tier3-rate-2026-02-13.md`、`artifacts/tck/tier3-cluster-2026-02-13.md`）。
- 基线门禁复跑：`fmt + clippy + workspace_quick_test + tier0/1/2 + binding_smoke + contract_smoke` 全绿（见 `artifacts/tck/beta-03r7-w3-baseline-gates-rerun-2026-02-13.log`）。

### BETA-03R8 子进展（2026-02-13）
- R8-W1：修复 lexer 标识符首字符规则，允许 `_` 开头变量（清零 `Create4` 的 `Unexpected character: _` 语法报错簇）。
- R8-W2：放宽 DELETE 实体表达式可达校验，补齐 `__getprop` 传递路径，支持 `DELETE rels.key.key[0]`（清零 `Delete5[6]`）。
- R8-W3：修复关联 MATCH 编译锚点：当模式属性表达式引用外层已绑定变量时，按相关子查询语义构建执行计划，消除 `UNWIND + MATCH + MERGE` 丢行（清零 `Unwind1[6]`）。
- R8-W4：TCK side-effects 标签口径修正：统计时排除 `UNLABELED` 哨兵标签，修复 `Create4[2]` `+labels` 与 `Delete3[1]` `-labels` 偏差。
- 定向回归：`Merge5/6/7`、`ReturnOrderBy4`、`Pattern2`、`Comparison2`、`Null3`、`Create4`、`Delete3`、`Delete5`、`Unwind1` 全通过（非跳过）。
- 证据日志：`artifacts/tck/beta-03r8-merge567-repro-2026-02-13.log`、`artifacts/tck/beta-03r8-merge567-fixed-2026-02-13.log`、`artifacts/tck/beta-03r8-next-cluster-repro-2026-02-13.log`、`artifacts/tck/beta-03r8-next-cluster-fixed-2026-02-13.log`、`artifacts/tck/beta-03r8-targeted-regression-clean-2026-02-13.log`。

### BETA-03R9 子进展（2026-02-14）
- R9-W1：修复 TCK harness 步骤正则过度转义（`\\(` → `\(`），恢复两类“忽略列表元素顺序”断言步骤：
  - `the result should be (ignoring element order for lists):`
  - `the result should be, in order (ignoring element order for lists):`
- R9-W2：定向回归验证从 skipped 转 pass：
  - `clauses/match/Match4.feature`：`9 passed, 1 skipped` → `10 passed`
  - `expressions/map/Map3.feature`：`2 passed, 9 skipped` → `11 passed`
  - `clauses/return-orderby/ReturnOrderBy2.feature`：场景 `[12]` 从 skipped 转 pass（全 14 passed）
- R9-W3：Tier-3 全量复算通过门槛：
  - `3897 scenarios (3719 passed, 178 skipped, 0 failed)`，通过率 `95.43%`（达到 `BETA-03` 目标）
- R9-W4：基线门禁复验通过：
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
  - `bash scripts/binding_smoke.sh`
  - `bash scripts/contract_smoke.sh`
- 证据日志：
  - `artifacts/tck/beta-03r9-step-regex-match4-before-2026-02-14.log`
  - `artifacts/tck/beta-03r9-step-regex-match4-after-2026-02-14.log`
  - `artifacts/tck/beta-03r9-step-regex-map3-before-2026-02-14.log`
  - `artifacts/tck/beta-03r9-step-regex-map3-after-2026-02-14.log`
  - `artifacts/tck/beta-03r9-step-regex-returnorderby2-after-2026-02-14.log`
  - `artifacts/tck/beta-03r9-tier3-full-2026-02-14.log`
  - `artifacts/tck/beta-03r9-baseline-gates-2026-02-14.log`
  - `artifacts/tck/tier3-rate-2026-02-14.md`
  - `artifacts/tck/tier3-cluster-2026-02-14.md`

### BETA-04 子进展（2026-02-14）
- 新增稳定窗门禁脚本：`scripts/stability_window.sh`
  - 输入：`artifacts/tck/tier3-rate-YYYY-MM-DD.json`
  - 默认门禁：最近 `7` 天均满足 `pass_rate >= 95` 且 `failed = 0`
  - 支持环境变量：`STABILITY_DAYS`、`TCK_MIN_PASS_RATE`、`TCK_REPORT_DIR`
- 与现有门禁衔接：
  - `scripts/tck_full_rate.sh` 负责产出每日 rate 快照
  - `scripts/beta_gate.sh` 负责单次阈值阻断
  - `scripts/stability_window.sh` 负责连续天数窗口阻断
- 当前状态：`2026-02-14` 快照已达标（`3790/3897=97.25%`，`failed=0`），稳定窗累计仍在进行中。

### BETA-03R10 子进展（2026-02-14）
- R10-W1：新增 TCK harness 图夹具步骤 `Given the <graph> graph`，支持从 `tests/opencypher_tck/tck/graphs/<name>/<name>.cypher` 自动加载图数据。
- R10-W2：定向回归 `useCases/triadicSelection/TriadicSelection1.feature`：
  - `19 skipped` → `19 passed`（零失败）
- R10-W3：Tier-3 全量复算进一步提升：
  - `3897 scenarios (3738 passed, 159 skipped, 0 failed)`，通过率 `95.92%`
- 证据日志：
  - `artifacts/tck/beta-04-triadic-before-2026-02-14.log`
  - `artifacts/tck/beta-04-triadic-after-2026-02-14.log`
  - `artifacts/tck/beta-04-tier3-rerun-2026-02-14.log`
  - `artifacts/tck/tier3-rate-2026-02-14.md`

### BETA-03R11 子进展（2026-02-14）
- R11-W1：补齐 `CALL` 簇的 TCK harness 步骤与夹具能力：
  - 新增 `And there exists a procedure ...` 步骤，支持签名（输入/输出类型）+ 表格数据注册。
  - 新增 `ProcedureError` / `ParameterMissing` 编译期断言步骤桥接。
- R11-W2：查询内核最小语义补齐（面向 openCypher CALL 簇）：
  - `procedure_registry` 增加 fixture 驱动的 `test.doNothing` / `test.labels` / `test.my.proc`。
  - `CALL` 支持无括号形式（隐式参数模式）与 `YIELD *`（仅 standalone 放行）。
  - 编译期补齐 `VariableAlreadyBound`（YIELD 覆盖冲突）与 `InvalidAggregation`（CALL 参数含聚合）。
  - `void` procedure 在 in-query 场景保持输入行基数（不吞行）。
  - TCK harness 串行执行（`max_concurrent_scenarios(1)`）避免 fixture 并发串台。
- R11-W3：Tier-3 全量复算（全绿）：
  - `3897 scenarios (3790 passed, 107 skipped, 0 failed)`，通过率 `97.25%`
  - 相比 R10：`passed +52`、`skipped -52`
- 证据日志：
  - `artifacts/tck/beta-04-callcluster-tier3-full-2026-02-14.log`
  - `artifacts/tck/beta-04-skipped-cluster-2026-02-14.txt`
  - `artifacts/tck/tier3-rate-2026-02-14.md`
  - `artifacts/tck/tier3-cluster-2026-02-14.md`

### BETA-03R12 子进展（2026-02-14）
- R12-W1：补齐 TCK harness 错误断言步骤并统一桥接入口：
  - 新增步骤：`TypeError`（runtime/any-time/compile-time）、`ArgumentError`（runtime）、`SyntaxError`（runtime）、`EntityNotFound`（runtime）、`SemanticError`（runtime）、`ConstraintVerificationFailed`（runtime）。
  - 在 `nervusdb/tests/tck_harness.rs` 引入统一 `assert_error_raised` helper，维持 `SyntaxError/ProcedureError/ParameterMissing` 编译期路径的严格断言。
- R12-W2：针对原 skipped 主簇做 21 个 feature 定向回归并全通过：
  - 覆盖 `Match4/Match9`、`List1/List11`、`TypeConversion1-4`、`Map1/2`、`Graph3/4/6`、`Aggregation6`、`Return2`、`ReturnSkipLimit1/2`、`Merge1/5`、`Set1`、`Delete1`。
- R12-W3：Tier-3 全量复算收口至 100%：
  - `3897 scenarios (3897 passed, 0 skipped, 0 failed)`，通过率 `100.00%`。
  - 相比 R11：`passed +107`、`skipped -107`、`failed 维持 0`。
- 证据日志：
  - `artifacts/tck/beta-04-error-step-bridge-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-error-step-bridge-tier3-full-2026-02-14.log`
  - `artifacts/tck/beta-04-skipped-cluster-2026-02-14.txt`
  - `artifacts/tck/tier3-rate-2026-02-14.json`
  - `artifacts/tck/tier3-rate-2026-02-14.md`
  - `artifacts/tck/tier3-cluster-2026-02-14.md`

## Archived (v1/Alpha)

_Previous tasks (T1-T67) are archived in `docs/memos/DONE.md`._
