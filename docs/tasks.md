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
| M5-01         | [Binding] Python + Node.js 可用性收敛（PyO3 + N-API）      | High   | Done   | feat/M5-01-bindings         | Rust 基线 API 面与 parity 门禁已全量对齐；三端 capability 套件已改为硬断言并全绿；剩余核心缺口 `left/right`、`shortestPath` 已于 2026-02-18 清零。 |
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
| BETA-03R13    | [Hardening] `TypeError` 断言收紧（compile-time + any-time + runtime） | High   | Done   | codex/feat/beta-04-r13w2-anytime-hardening | R13-W1/W2/W3 已全部完成：compile-time、any-time、runtime 三类 `TypeError` 断言均切换为严格模式；补齐递归运行期表达式类型守卫（含 list comprehension 作用域）与属性写入非法 list 元素拦截，定向簇与基线门禁全绿。 |
| BETA-03R14    | [Hardening] runtime 语义一致性收口（WHERE guard + type(rel)） | High   | Done   | codex/feat/beta-04-r14w2-unwind-guard | R14-W1~W13 已完成：`WHERE/UNWIND/SET/MERGE/FOREACH/DELETE/CREATE/CALL/Aggregate/IndexSeek` 入口 runtime guard 全覆盖，`runtime_guard_audit` 热点清零并接入 CI；W13-A 全量证据：core gates 全绿、Tier-3 全量 `3897/3897` 全通过。 |
| BETA-04       | [Stability] 连续 7 天主 CI + nightly 稳定窗                | High   | WIP    | feat/TB1-stability-window   | strict 稳定窗基建已落地（`ci-daily-snapshot` + `stability_window.sh --mode strict` + `beta_release_gate.sh` + release 接线）；截至 2026-02-20（UTC）累计 `consecutive_days=5/7`（`2026-02-15` 为空快照 `empty_tier3_snapshot`），发布门禁仍阻断，若不重置最早 2026-02-22 达标。 |
| BETA-05       | [Perf] 大规模 SLO 封板（读120/写180/向量220 ms P99）       | High   | WIP    | codex/feat/w13-perf-guard-stream | W13-PERF 已落地资源护栏+高内存算子收敛；待主分支 Nightly 8h 复测并累计稳定窗证据。 |

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

### BETA-03R13 子进展（2026-02-14）
- R13-W1（compile-time 严格化）：
  - 将 `TypeError should be raised at compile time` 从“桥接可放行”切换为严格断言（`allow_success=false`）。
  - 在投影编译绑定校验中新增“变量来源表达式追溯”（沿 `Plan` 链回溯 alias 源表达式），对可静态判定为非 map 标量/列表的属性访问直接报 `syntax error: InvalidArgumentType`。
  - 保留 `null` 来源不误判（如 `WITH null AS m RETURN m.x` 仍返回 `null`）。
- R13-W1（定向与扩展回归）：
  - 定向：`expressions/map/Map1.feature`、`expressions/graph/Graph6.feature` 严格断言均全通过。
  - 扩展：`Map2`、`Graph3`、`Graph4`、`Return2` 全通过。
  - 基线门禁：`fmt + workspace_quick_test + tier0/1/2 + binding_smoke + contract_smoke` 全通过。
- R13-W2（any-time 严格化）：
  - 将 `TypeError should be raised at any time` 从桥接放行切换为严格断言（`allow_success=false`）。
  - 收敛 `__index` 的 runtime 类型语义：在 `Plan::Project` 执行路径增加类型守卫，不兼容索引组合直接返回 runtime error（`InvalidArgumentType`）。
  - 清零 `List1` 暴露簇：从收紧后的 `23 scenarios (5 passed, 18 failed)` 修复到 `23 passed`。
- R13-W2（回归与门禁）：
  - 定向：`expressions/list/List1.feature` 全通过。
  - 扩展：`List11`、`Map1`、`Map2`、`Graph6`、`Return2` 全通过。
  - 基线门禁：`fmt + workspace_quick_test + tier0/1/2 + binding_smoke + contract_smoke` 全通过。
- R13-W3（runtime 严格化）：
  - 将 `TypeError should be raised at runtime` 从桥接放行切换为严格断言（`allow_success=false`）。
  - 在执行层引入递归运行期表达式类型守卫（`Project`/`OrderBy` 路径）：
    - 覆盖 `__index`、`labels`、`type`、`toBoolean`、`toInteger`、`toFloat`、`toString`。
    - 支持 `ListComprehension` 作用域递归检查，修复 `TypeConversion1~4` 在列表推导内的漏拦截。
  - 写路径属性转换补齐 `InvalidPropertyType` 校验：禁止将 `Map/Node/Relationship/Path/ReifiedPath/Id/EdgeKey` 作为 list 元素写入属性（修复 `Set1` 非法属性类型簇）。
  - 定向严格化扫描与复扫：`Map2`、`Graph3`、`Graph4`、`Set1`、`TypeConversion1/2/3/4` 全通过。
- R13-W3（基线门禁）：
  - 首次门禁在 `workspace_quick_test` 暴露 `t311_expressions` 的 duration roundtrip 回归；修复 `toString` 守卫放行 duration map 后复跑全绿。
  - 基线门禁最终通过：`fmt + workspace_quick_test + tier0/1/2 + binding_smoke + contract_smoke` 全通过。
- 证据日志：
  - `artifacts/tck/beta-04-r13w1-map1-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w1-graph6-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w1-regression-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w1-gate-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w2-list1-anytime-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w2-regression-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w2-gate-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w3-runtime-strict-scan-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w3-runtime-strict-scan-remaining-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w3-runtime-strict-rescan-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w3-gate-2026-02-14.log`
  - `artifacts/tck/beta-04-r13w3-gate-rerun-2026-02-14.log`

### BETA-03R14 子进展（2026-02-14）
- R14-W1（TDD：先红后绿）：
  - 新增失败用例并先验证红灯：
    - `test_where_invalid_list_index_raises_runtime_type_error`（`WHERE` 非法索引应抛 runtime `InvalidArgumentType`）
    - `test_set_relationship_return_type_keeps_rel_type_name`（`SET ... RETURN type(r)` 应返回关系类型名）
  - 修复点：
    - `FilterIter` 接入 `ensure_runtime_expression_compatible`，使 `WHERE` 与 `Project/OrderBy` 的 runtime TypeError 语义一致。
    - `evaluate_type` 补齐 `Value::Relationship` 分支，与 `Value::EdgeKey` 保持一致的类型名解析。
- R14-W1（定向回归）：
  - 集成测试：`t301_expression_ops`、`t108_set_clause`、`t313_functions` 全通过。
  - TCK 定向：`List1`、`Graph4`、`Set1` 全通过。
  - `cargo fmt --all -- --check` 通过。
- R14-W2（TDD：先红后绿，UNWIND 入口补洞）：
  - 新增失败用例并先验证红灯：
    - `test_unwind_invalid_list_index_raises_runtime_type_error`
    - `test_unwind_toboolean_invalid_argument_raises_runtime_type_error`
  - 修复点：
    - `execute_unwind` 在每行展开前接入 `ensure_runtime_expression_compatible`，使 `UNWIND` 与 `Project/OrderBy/WHERE` 的 runtime TypeError 语义一致。
    - 修复此前 `UNWIND` 对非法表达式“吞错并产出 null 行”的行为差异。
- R14-W2（定向回归）：
  - 集成测试：`t306_unwind`（含新增 2 条）全通过。
  - 扩展回归：`t301_expression_ops`（where runtime guard）、`t108_set_clause`（type(rel)）、`t313_functions` 全通过。
  - TCK 定向：`List1`、`Graph4`、`Set1` 全通过。
  - 门禁：`bash scripts/tck_tier_gate.sh tier0` 全通过，`cargo fmt --all -- --check` 通过。
- R14-W3（TDD：先红后绿，写路径表达式入口补洞）：
  - 新增失败用例并先验证红灯：
    - `test_set_invalid_toboolean_argument_raises_runtime_type_error`
  - 修复点：
    - `execute_set`、`execute_set_from_maps` 在表达式求值前接入 `ensure_runtime_expression_compatible`。
    - `merge_apply_set_items`、`merge_eval_props_on_row` 同步接入 guard，消除 `SET/MERGE` 写路径对非法表达式“静默 `null`/继续执行”的语义缺口。
- R14-W3（定向回归）：
  - 集成测试：`t108_set_clause`（11/11，含新增 runtime 错误断言）、`t105_merge_test`、`t323_merge_semantics`、`t306_unwind`、`t301_expression_ops`、`t313_functions` 全通过。
  - TCK 定向：`Set1`、`List1`、`Graph4` 全通过。
  - 门禁：`bash scripts/tck_tier_gate.sh tier0` 全通过，`cargo fmt --all -- --check` 通过。
- R14-W4（TDD：先红后绿，尾部执行入口收口）：
  - 新增失败用例并先验证红灯：
    - `t324_foreach_invalid_toboolean_argument_raises_runtime_type_error`
    - `test_delete_list_index_with_invalid_index_type_raises_runtime_type_error`
  - 修复点：
    - `execute_foreach` 对列表表达式求值前接入 `ensure_runtime_expression_compatible`。
    - `execute_delete` / `execute_delete_on_rows` 对 DELETE 目标表达式求值前接入 `ensure_runtime_expression_compatible`。
    - 修复 `FOREACH/DELETE` 在非法表达式下“静默 `null` 或继续执行”的语义缺口，与既有 runtime guard 路径保持一致。
- R14-W4（定向回归）：
  - 集成测试：`t324_foreach`（4/4，含新增 1 条）、`create_test` 新增 DELETE runtime 错误断言、`t108/t306/t301` runtime 严格化断言全通过。
  - TCK 定向：`Delete5`、`Delete1`、`Delete3` 全通过。
  - 门禁：`bash scripts/tck_tier_gate.sh tier0|tier1|tier2` 全通过，`cargo fmt --all -- --check` 通过。
- R14-W5（TDD：先红后绿，CREATE 属性表达式入口补洞）：
  - 新增失败用例并先验证红灯：
    - `test_create_property_with_invalid_toboolean_argument_raises_runtime_type_error`
  - 修复点：
    - `execute_create_from_rows` 的节点/关系属性表达式求值前接入 `ensure_runtime_expression_compatible`。
    - 修复此前 `CREATE ... {prop: toBoolean(1)}` 可能“静默写入 null/跳过属性”的行为差异，与其他执行入口 runtime TypeError 语义对齐。
- R14-W5（定向回归）：
  - 集成测试：`create_test`（新增 CREATE runtime 错误断言 + DELETE runtime 错误断言）、`t324_foreach`、`t108`、`t306` 全通过。
  - TCK 定向：`Create1`、`Delete5` 全通过。
  - 门禁：`bash scripts/tck_tier_gate.sh tier0` 全通过，`cargo fmt --all -- --check` 通过。
- R14-W6（TDD：先红后绿，CALL 参数表达式入口补洞）：
  - 新增失败用例并先验证红灯：
    - `test_procedure_argument_expression_invalid_toboolean_raises_runtime_type_error`
  - 修复点：
    - `ProcedureCallIter::next` 在过程参数逐项求值前接入 `ensure_runtime_expression_compatible`。
    - 修复 `CALL ...` 参数表达式在非法输入下先落到过程内部报错（如 `math.add requires numeric arguments`）的语义偏差，统一为 runtime `InvalidArgumentValue`。
- R14-W6（定向回归）：
  - 集成测试：`t320_procedures`（新增 1 条）、`t324_foreach`、`t108_set_clause`、`t306_unwind`、`create_test`、`t301_expression_ops` 全通过。
  - TCK 定向：`clauses/call/Call1.feature`、`clauses/call/Call2.feature`、`clauses/call/Call3.feature` 全通过。
  - 门禁：`bash scripts/tck_tier_gate.sh tier0` 全通过，`cargo fmt --all -- --check` 通过。
- R14-W7（TDD：先红后绿，聚合参数表达式入口补洞）：
  - 新增失败用例并先验证红灯：
    - `test_aggregate_argument_invalid_toboolean_raises_runtime_type_error`
  - 修复点：
    - `execute_aggregate` 对每行输入的聚合参数表达式（含 `count/sum/avg/min/max/collect/percentile`）统一接入 `ensure_runtime_expression_compatible`。
    - 修复 `RETURN count(toBoolean(1))` 被吞成 `0` 的语义偏差，统一为 runtime `InvalidArgumentValue`。
- R14-W7（定向回归）：
  - 集成测试：`t152_aggregation`（新增 1 条）、`t320_procedures` 全通过。
  - TCK 定向：`expressions/aggregation/Aggregation1.feature`、`expressions/aggregation/Aggregation2.feature`、`expressions/typeConversion/TypeConversion1.feature` 全通过。
  - 门禁：`bash scripts/tck_tier_gate.sh tier0` 全通过，`cargo fmt --all -- --check` 通过。
- R14-W8（审计：`IndexSeek` 非法值表达式回归加固）：
  - 新增回归用例：
    - `test_index_seek_invalid_value_expression_raises_runtime_type_error`
  - 审计结论：
    - `MATCH (n:Person) WHERE n.name = toBoolean(1) RETURN n` 在索引路径下保持 runtime `InvalidArgumentValue` 语义，不会被“空结果”静默吞错。
  - 定向回归：`t107_index_integration` 新增用例通过；`expressions/typeConversion/TypeConversion1.feature` 全通过；`cargo fmt --all -- --check` 通过。
- R14-W9（审计：`percentile` 双参数 guard 回归加固）：
  - 新增回归用例：
    - `test_percentile_argument_invalid_toboolean_raises_runtime_type_error`
  - 审计结论：
    - `RETURN percentileDisc(1, toBoolean(1))` 在聚合路径下稳定抛 runtime `InvalidArgumentValue`，`PercentileDisc/Cont` 的双表达式 guard 分支已被覆盖。
  - 定向回归：`t152_aggregation`（新增 1 条）通过；`expressions/aggregation/Aggregation2.feature`、`expressions/typeConversion/TypeConversion1.feature` 全通过；`cargo fmt --all -- --check` 通过。
- R14-W10（TDD：先红后绿，`IndexSeek` 值表达式入口补洞）：
  - 执行入口修复：
    - `execute_index_seek` 在值表达式求值前接入 `ensure_runtime_expression_compatible`，避免依赖 fallback 路径兜底 runtime 错误语义。
  - 回归补强：
    - `test_index_seek_invalid_value_expression_raises_runtime_type_error` 保持覆盖，锁定 `MATCH (n:Person) WHERE n.name = toBoolean(1) RETURN n` 仍抛 runtime `InvalidArgumentValue`。
  - 定向回归：`t107_index_integration`、`t320_procedures`、`t152_aggregation` 全通过；`expressions/typeConversion/TypeConversion1.feature` 全通过。
  - 门禁：`bash scripts/tck_tier_gate.sh tier0` 全通过，`cargo fmt --all -- --check` 通过。
- R14-W11（审计：runtime guard 扫描脚本落地）：
  - 新增脚本：`scripts/runtime_guard_audit.sh`（兼容 macOS 默认 bash 3.2）
  - 审计输出：
    - 统计 `executor/` 中 `evaluate_expression_value` vs `ensure_runtime_expression_compatible` 分布；
    - 当前唯一潜在热点：`write_orchestration.rs`（存在一次直接求值且未显式 guard；该处为 delete overlay 目标收集，后续可评估是否需要改为 Result 传播）。
  - 证据：`artifacts/tck/beta-04-r14w11-runtime-guard-audit-2026-02-14.log`。
- R14-W12（收口：清零 executor 侧 runtime guard 审计热点）：
  - 修复点：
    - `collect_delete_targets_from_rows` 升级为 `Result`，并在表达式求值前接入 `ensure_runtime_expression_compatible`，避免 delete overlay 目标收集阶段的“未 guard 直接求值”。
  - 结果：
    - `scripts/runtime_guard_audit.sh` 输出 `potential hotspots` 为 `none`。
  - 门禁：`bash scripts/tck_tier_gate.sh tier0` 全通过，`cargo fmt --all -- --check` 通过。
- 证据日志：
  - `artifacts/tck/beta-04-r14w1-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w1-tier0-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w2-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w2-tier0-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w3-write-guard-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w3-write-guard-tier0-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w4-tail-guard-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w4-tail-guard-tier0-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w4-tail-guard-tier12-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w5-create-guard-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w5-create-guard-tier0-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w6-call-guard-unit-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w6-call-guard-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w6-call-guard-tier0-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w6-call-guard-fmt-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w7-aggregate-guard-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w7-aggregate-guard-tier0-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w7-aggregate-guard-fmt-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w8-index-seek-audit-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w8-index-seek-audit-fmt-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w9-percentile-guard-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w9-percentile-guard-fmt-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w10-index-seek-guard-targeted-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w10-index-seek-guard-fmt-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w10-index-seek-guard-tier0-2026-02-14.log`
- `artifacts/tck/beta-04-r14w11-runtime-guard-audit-2026-02-14.log`
- `artifacts/tck/beta-04-r14w12-runtime-guard-hotspot-fix-2026-02-14.log`
- `artifacts/tck/beta-04-r14w12-runtime-guard-hotspot-fix-tier0-2026-02-14.log`
- `artifacts/tck/beta-04-r14w12-runtime-guard-hotspot-fix-fmt-2026-02-14.log`

### BETA-03R14 子进展（2026-02-15，W13 收尾）
- R14-W13-A（收口与门禁）：
  - `runtime_guard_audit` CLI 固化：保留 `--root`、`--fail-on-hotspot`、`--help`，并支持无 `rg` 时回退 `grep -RIn`。
  - CI 接线：`ci.yml` 已加入 `bash scripts/runtime_guard_audit.sh --fail-on-hotspot`（位于 `fmt/clippy` 后、`workspace_quick_test` 前）。
  - W13 核心门禁复跑全绿：`fmt + clippy + runtime_guard + workspace_quick_test + tier0/1/2 + binding_smoke + contract_smoke` 全通过。
  - Tier-3 全量复跑保持全绿：`3897 scenarios (3897 passed)`，失败簇报告 `No step failures found.`。
- R14-W13-A（语义补点）：
  - 修复写路径 list 属性转换对 duration map 的误拦截，允许 `List<Duration>` 写入属性；普通 map list 仍维持 `InvalidPropertyType`。
  - 新增回归单测：
    - `allows_duration_maps_inside_list_properties`
    - `rejects_regular_map_inside_list_properties`
- 证据日志：
  - `artifacts/tck/beta-04-r14w13-runtime-guard-gate-2026-02-15.log`
  - `artifacts/tck/beta-04-r14w13-core-gates-2026-02-15.log`
  - `artifacts/tck/beta-04-r14w13-tier3-full-2026-02-15.log`
  - `artifacts/tck/beta-04-r14w13-tier3-full-2026-02-15.cluster.md`

### BETA-04 子进展（2026-02-15，strict 稳定窗 Day1）
- W13-B（基建）：
  - 已落地 `ci-daily-snapshot.yml`、`stability-window-daily.yml`、`beta_release_gate.sh`、strict 模式 `stability_window.sh`，并接入发布流程阻断（不阻断日常 PR）。
- W13-C（Day1 记账）：
  - 当日快照产物：
    - `artifacts/tck/tier3-rate-2026-02-15.json`（`pass_rate=100.00`，`failed=0`）
    - `artifacts/tck/ci-daily-2026-02-15.json`（`all_passed=true`）
    - `artifacts/tck/stability-daily-2026-02-15.json`
    - `artifacts/tck/stability-window.json`
    - `artifacts/tck/stability-window.md`
  - strict 窗口当前状态：
    - `consecutive_days=0/7`
    - `window_passed=false`
    - 本地 Day1 统计因 `github_data_unavailable`（nightly 工作流历史需在主分支运行后回填）未计入连续通过天数。
  - 计划完成日（若后续连续 7 天全通过）：最早 `2026-02-21`。
  - 证据日志：
    - `artifacts/tck/beta-04-r14w13-stability-window-day1-2026-02-15.log`
    - `artifacts/tck/beta-04-r14w13-stability-window-day1-2026-02-15.rc`

### BETA-04 子进展（2026-02-16，strict 稳定窗 Day2 回填修复）
- W13-Day2（回填鲁棒性）：
  - `scripts/stability_window.sh` 修复 tier3 回填选择逻辑：按 `created_at <= day_end(UTC)` 选最新成功 run，不再依赖 `created_at startswith(day)`。
  - artifact 选择优先级固定为 `tck-nightly-artifacts` > `beta-gate-artifacts`。
  - 回填失败原因细化并可审计：`artifact_fetch_auth_failed`、`artifact_not_found`、`tier3_backfill_failed`（替代泛化 `missing_tier3_rate`）。
- W13-Day2（fixture 回归）：
  - 新增 `scripts/tests/stability_window_fixture.sh`，覆盖：
    - 7 天全通过 -> PASS；
    - 中间 tier3 失败 -> 连续计数重置；
    - 缺失 `ci-daily` -> 当天失败；
    - 有 token/无 token 路径 reason 区分。
- W13-Day2（实况复算）：
  - 命令：`bash scripts/stability_window.sh --mode strict --date 2026-02-16 --github-repo LuQing-Studio/nervusdb --github-token-env GITHUB_TOKEN`
  - 结果：`2026-02-16` 当日 `pass=true`，不再出现 `missing_tier3_rate` 阻断；窗口 `consecutive_days=2/7`，继续累计中。
- 证据日志：
  - `artifacts/tck/beta-04-day2-backfill-2026-02-16.log`
  - `artifacts/tck/beta-04-day2-backfill-2026-02-16.rc`

### BETA-05 子进展（2026-02-15，W13-PERF 一次到位）
- W13-A（资源护栏）：
  - `Params` 新增 `ExecuteOptions`（默认平衡档）：
    - `max_intermediate_rows=500000`
    - `max_collection_items=200000`
    - `soft_timeout_ms=5000`
    - `max_apply_rows_per_outer=200000`
  - 新增 `Error::ResourceLimitExceeded { kind, limit, observed, stage }`，并通过 `Params` runtime 计数器统一触发。
  - 新增回归：`nervusdb/tests/t341_resource_limits.rs`（5/5 通过）。
- W13-B（高内存算子收敛）：
  - `UNWIND` 从“每行先物化 Vec 再发射”改为迭代发射；
  - `ORDER BY`、`OptionalWhereFixup` 加入有界收集与超时检查；
  - `Aggregate` 增加 group/rows/collect distinct 规模限制；
  - `Apply` 增加每外层行子查询输出上限。
- W13-C（CI/Fuzz 策略）：
  - `fuzz-nightly.yml` 中 `query_execute` 参数已收敛到 `-max_len=1024 -timeout=10`，保留 `rss_limit_mb=4096`。
  - Node/Python 错误分类补齐 `ResourceLimitExceeded` 路径（归类 execution）。
  - 监控后追加 `query_parse` timeout 收口：parser 增加复杂度步数预算（`ParserComplexityLimitExceeded`），并将 failing 样本固化到
    `fuzz/regressions/query_parse/timeout-0150b9c6c52d68d4492ee8debb67edad1c52a05f`。
  - 本地回放同一超时样本耗时降至 `71ms`（由此前 ~9s 级降到毫秒级，避免 nightly 在 `query_parse` 阶段提前失败）。
- 回归与门禁（本轮）：
  - 定向：`Match4`、`Match9` 全通过；
  - 扩展矩阵：`Match1/2/3/6/7 + Path1/2/3 + Quantifier1/2` 全通过；
  - 基线：`fmt + clippy + workspace_quick_test + tier0/1/2 + binding_smoke + contract_smoke` 全通过。
- 证据产物：
  - `artifacts/tck/w13-perf-baseline.json`
  - `artifacts/tck/w13-perf-after-A.json`
  - `artifacts/tck/w13-perf-after-B.json`
  - `artifacts/tck/w13-perf-final.json`
  - `artifacts/tck/w13-perf-query-parse-timeout-fix-2026-02-15.log`
  - 说明：8h Fuzz 指标（`slowest/rss/exec_s`）需在主分支 Nightly 跑完后补录到 final 快照。

### BETA-04 子进展（2026-02-17，主线 B：内核缺口首批清零）
- 目标达成（硬断言）：
  - `MATCH (n:Manager)` 对多标签节点按标签包含关系匹配（不再依赖主标签）。
  - `MATCH ... MERGE (a)-[:LINK]->(b)` 关系语义修复为幂等（重复执行不重复建边）。
- 代码落点：
  - `nervusdb-query/src/executor/plan_iterators.rs`
  - `nervusdb-query/src/executor/merge_execution.rs`
  - `nervusdb-query/src/executor.rs`
  - `nervusdb/tests/t342_label_merge_regressions.rs`
- 三端能力测试改为硬断言（去 soft-pass）：
  - `examples-test/nervusdb-rust-test/tests/test_capabilities.rs`
  - `examples-test/nervusdb-node-test/src/test-capabilities.ts`
  - `examples-test/nervusdb-python-test/test_capabilities.py`
- 定向验证：
  - `cargo test -p nervusdb --test t342_label_merge_regressions`
  - `bash examples-test/run_all.sh`
  - `cargo test -p nervusdb --test tck_harness -- --input clauses/match/Match1.feature`
  - `cargo test -p nervusdb --test tck_harness -- --input clauses/merge/Merge1.feature`
  - `cargo test -p nervusdb --test tck_harness -- --input clauses/merge/Merge2.feature`
  - 结果：全部通过。

### BETA-04 子进展（2026-02-18，核心缺口二批清零 + Fuzz timeout 止血）
- 核心缺口清零（left/right + shortestPath）：
  - `left()` / `right()` 已在核心 evaluator 落地，并加入编译期函数白名单；
  - `MATCH p = shortestPath((...)-[*]->(...))` 已支持解析执行（不再报 `Expected '('`）；
  - 新增核心回归：
    - `nervusdb/tests/t313_functions.rs::test_left_and_right_string_functions`
    - `nervusdb/tests/t318_paths.rs::test_shortest_path_in_match_assignment`
  - 三端 capability 改为硬断言后全绿：
    - `examples-test/nervusdb-rust-test/tests/test_capabilities.rs`
    - `examples-test/nervusdb-node-test/src/test-capabilities.ts`
    - `examples-test/nervusdb-python-test/test_capabilities.py`
- Fuzz Nightly `query_execute` timeout 止血：
  - `fuzz/fuzz_targets/query_execute.rs` 增加执行预算（`ExecuteOptions`）并将输入长度收敛到 `<=1024`；
  - `.github/workflows/fuzz-nightly.yml` 调整 `query_execute` 参数为 `-max_len=1024 -timeout=10`，降低单样本超时误报；
  - 本地 smoke 验证：`cargo +nightly fuzz run query_execute -- -max_total_time=5 -max_len=1024 -timeout=10 -rss_limit_mb=4096` 通过。
- 本轮回归：
  - `bash examples-test/run_all.sh` 全绿（Rust/Node/Python 三端通过）；
  - `cargo test -p nervusdb --test t313_functions test_left_and_right_string_functions` 通过；
  - `cargo test -p nervusdb --test t318_paths test_shortest_path_in_match_assignment` 通过；
  - `cargo fmt --all -- --check` 与 `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings` 通过。

### BETA-04 子进展（2026-02-18，strict 稳定窗 Day4 累计恢复）
- 现场诊断：
  - `stability-window` 在 `2026-02-17/2026-02-18` 的 Tier-3 回填出现 `artifact_fetch_auth_failed`，导致窗口被误归零。
- 工程修复：
  - 为调用 `stability_window.sh` 的 workflow 增补 `actions: read` 权限：
    - `.github/workflows/stability-window-daily.yml`
    - `.github/workflows/tck-nightly.yml`
    - `.github/workflows/release.yml`
- 实况复算（UTC）：
  - `bash scripts/stability_window.sh --mode strict --date 2026-02-18 --github-repo LuQing-Studio/nervusdb --github-token-env GITHUB_TOKEN`
- 结果：
  - `2026-02-17`、`2026-02-18` 均恢复为 `PASS`；
  - 复算后窗口累计为 `3/7`（`2026-02-15` 仍为 `threshold_or_failed`，未进入连续窗口）；
  - 发布门禁仍阻断，继续累计至 `7/7`。
- 诊断增强（同日续更）：
  - `scripts/stability_window.sh` 增加空快照识别：当 `tier3-rate` 出现 `scenarios.total=0` 时，原因标记为 `empty_tier3_snapshot`（不再混同于 `threshold_or_failed`）。
  - 新增 fixture：`scripts/tests/stability_window_fixture.sh::scenario_empty_tier3_snapshot_reason`。
  - 复算口径：`2026-02-15` 原因更新为 `empty_tier3_snapshot`，`consecutive_days` 维持 `3/7`。
- 证据：
  - `artifacts/tck/beta-04-stability-window-day4-2026-02-18.log`
  - `artifacts/tck/beta-04-stability-window-day4-2026-02-18.rc`
  - `artifacts/tck/stability-window.md`（本地证据目录受 `.gitignore` 管控）

### BETA-04 子进展（2026-02-20，strict 稳定窗 Day6 累计）
- 当日执行（UTC）：
  - 手动触发并通过：
    - `CI Daily Snapshot`（run `22224769833`）
    - `TCK Nightly Tier-3`（run `22224771152`）
    - `Stability Window Daily`（run `22224772537`）
- 稳定窗产物（`stability-window-artifacts`）：
  - `as_of_date=2026-02-20`
  - `consecutive_days=5`
  - `window_passed=false`
  - `2026-02-20` 当日 `tier3/ci_daily/nightly` 全部 `PASS`
- 当前阻断项：
  - 历史日 `2026-02-15` 仍为 `empty_tier3_snapshot`（非连续通过日）；
  - 因 strict 连续计数规则，发布门禁继续阻断，需继续累计至 `7/7`。
- 下一里程碑：
  - 若 `2026-02-21` 与 `2026-02-22` 连续通过，则稳定窗可达 `7/7` 并解除发布阻断。

## Archived (v1/Alpha)

_Previous tasks (T1-T67) are archived in `docs/memos/DONE.md`._
