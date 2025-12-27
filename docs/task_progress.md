| ID | Task | Complexity | Priority | Status | Branch/PR | Notes |
|:---|:-----|:---------:|:-------:|:------:|:---------|:------|
| T1 | 索引精简并添加字符串缓存/读事务复用以提升 NervusDB 写读性能 | L2 | P0 | Done | perf/T4-node-bulk-resolve | 索引收敛到 `SPO/POS/OSP`；读事务与表句柄复用；写路径字符串缓存改为真 LRU |
| T2 | 清理 Node 侧旧目录引擎遗留（归档/删除） | L3 | P0 | Done | perf/T4-node-bulk-resolve | 删除旧目录引擎相关代码与测试；收口 open options；CLI 去旧命名 |
| T3 | 重写 Rust interning：使用 `lru` crate 替换伪 LRU | L2 | P0 | Done | perf/T4-node-bulk-resolve | `WriteTableHandles` 真 LRU；写路径确保走 handles；commit 后读缓存失效避免脏读 |
| T4 | Node 吞吐修复：提供“批量返回字符串 triples”的 Native API | L3 | P0 | Done | perf/T4-node-bulk-resolve | 新增 `queryFacts/readCursorFacts`；TS 外壳优先使用并降级；避免 per-triple 3 次 `resolveStr()` |
| T5 | Fuck-off Test：`kill -9` 下的数据一致性验证 | L3 | P0 | Done | feat/T5-fuck-off-test | 新增 `nervus-crash-test`：driver/writer/verify；目标：重启后要么事务前要么事务后，校验字典/索引引用一致 |
| T6 | 冻结并对齐 `nervusdb.h`：最小稳定 C 契约（含 Cypher 执行） | L3 | P0 | Done | feat/T6-ffi-freeze | 收口导出符号；补齐 resolve/exec_cypher/version；写清 ABI/内存释放规则（目标：1.0 后三个月不改头文件） |
| T7 | Node 绑定去插件化 + 修复 Cypher 调用致命 Bug | L3 | P0 | Done | feat/T7-node-thin-binding | 删 `PluginManager`/JS 聚合/TS Cypher；Cypher 只走 Rust Core 执行器；算法接口统一为 `db.algorithms.*` 原生透传 |
| T8 | Temporal 变为 optional feature（Default OFF） | L3 | P0 | Done | feat/T7-node-thin-binding | `nervusdb-core`/N-API 增加 `temporal` feature（默认关闭）；TS 侧 capability guard：未启用直接 fail-fast |
| T9 | Node Tests 纳入 CI（覆盖 Binding ↔ Native） | L2 | P0 | Done | feat/T7-node-thin-binding | CI 增加 node job（Ubuntu+macOS）：typecheck + TS-only tests + native addon smoke + crash-smoke |
| T10 | C API 二进制 Row 迭代器（替代 exec_cypher JSON 热路径）+ ABI 冻结策略 | L3 | P0 | Done | feat/T10-binary-row-iterator | 保留 `nervusdb_exec_cypher`（JSON）兼容；新增 stmt/step/column* 最小 API；目标：减少序列化与复制成本，并为 1.0 冻结 `nervusdb.h` 提供硬契约 |
| T11 | 性能重测与报告刷新（修正 redb 基线 + 补充 T10 stmt 对比） | L1 | P0 | Done | docs/T11-perf-refresh | 修正 `bench_compare` 的 redb 方法论；更新 `PERFORMANCE_ANALYSIS.md`（写清测量边界/环境）；补充 exec_cypher vs stmt 数据 |
| T12 | 1.0 封版准备（ABI 冻结 + 文档清洗 + Crash Gate 复跑） | L2 | P0 | Done | release/T12-1.0-prep | README/CHANGELOG/项目结构去谎言化；明确 ABI 冻结规则；本地 crash-gate 1000x 通过 |
| T13 | Node Statement API（对标 T10）+ 避免 V8 对象爆炸 | L3 | P0 | Done | feat/T13-node-statement | 新增 `prepareV2/step/column_* /finalize`；TS 提供流式消费路径；保留 `executeQuery` 兼容但不再是大结果集默认路径 |
| T14 | v1.0.0 封版（ABI 法律化 + Cypher 白名单 + Crash Gate） | L3 | P0 | Done | release/T14-v1.0.0 | 冻结 `nervusdb.h`；明确 Cypher 子集与 NotImplemented 行为；发布前必须通过 crash-gate 1000x |
| T15 | 真流式 Cypher 执行器（替换伪流式 Vec 预加载） | L3 | P0 | Done | - | Phase 1+2 完成：延迟执行 + StreamingQueryIterator；所有 warnings 已清理 |
| T16 | 代码清理：删除 _archive + 统一命名 | L1 | P1 | Done | - | 删除 `_archive/`；`synapseDb.ts` → `nervusDb.ts`；删除冗余 `lock.ts` |
| T17 | 真流式执行器（消除 collect） | L3 | P0 | Done | feat/T17-arc-database | Arc<Database> 包装 + execute_streaming 返回 'static 迭代器；FFI 层无 collect() |
| T18 | Node.js 属性写入优化 - 消除 JSON 序列化 | L2 | P0 | Done | feat/T18-msgpack-properties | 添加 *Direct 方法，直接传 JS Object，跳过 JSON.stringify/parse |
| T19 | temporal_v2 分离为独立 crate | L3 | P1 | Done | refactor/T19-T20-architecture | 创建 nervusdb-temporal crate，nervusdb-core 通过 feature gate 依赖 |
| T21 | Cypher ORDER BY + SKIP | L2 | P0 | Done | #10 | 支持 ORDER BY/SKIP；新增 Sort/Skip 计划节点 |
| T22 | Cypher 聚合函数（COUNT/SUM/AVG/MIN/MAX） | L3 | P0 | Done | #11 | Aggregate 节点；支持分组聚合 |
| T23 | Cypher WITH 子句 | L2 | P0 | Done | #12 | WITH 管线 + WHERE/DISTINCT/ORDER BY/SKIP/LIMIT |
| T24 | Cypher OPTIONAL MATCH | L3 | P0 | Done | #13 | Left outer join 语义；无匹配返回 NULL |
| T25 | Cypher MERGE | L3 | P0 | Done | #14 | 基础 MERGE 节点/关系；幂等创建 |
| T26 | Cypher 可变长度路径 | L3 | P0 | Done | #15 | 变长路径匹配；受限于无关系变量/属性 |
| T27 | Cypher UNION/UNION ALL | L2 | P0 | Done | #16 | 仅读查询；列对齐校验；distinct 去重 |
| T28 | 扩展 Cypher 内置函数 | L2 | P0 | Done | #17 | type/labels/keys/size/toUpper/toLower/trim/coalesce |
| T29 | Cypher CASE WHEN | L2 | P0 | Done | #18 | Case 表达式求值 |
| T30 | EXISTS/CALL 子查询 | L3 | P0 | Done | #19 | EXISTS 模式/子查询；CALL 仅支持独立子查询 |
| T31 | 列表字面量与推导式 | L2 | P0 | Done | #20 | List literal/comprehension；用于 IN/RETURN |
| T32 | Cypher 基础补全：UNWIND + DISTINCT + COLLECT 测试覆盖 | L3 | P0 | Done | #21 | UNWIND 行生成；DISTINCT 去重；补 COLLECT 行为测试 |
| T33 | Vector Index + Full-Text Search（usearch + tantivy） | L3 | P0 | Done | #26 | MVP 落地：feature gate + sidecar + 重建；`vec_similarity`/`txt_score`；后续：FTS 下推见 T34，Vector Top-K 见 T35 |
| T34 | FTS 下推：`txt_score` 谓词走索引候选集 | L3 | P0 | Done | #27 | planner 重写 Scan→FtsCandidateScan；限制：`txt_score(n.prop, $q) > 0` / `>= 正数`；Vector TopK 下推见 T35 |
| T35 | Vector Top-K 下推：`ORDER BY vec_similarity(...) DESC LIMIT k` | L3 | P0 | Done | #28 | 仅做 Sort+Limit 模式；不碰 WHERE/range；`usearch.search(query, k)` 候选集回表 |
| T36 | 发布准备 v1.0.3（版本统一 + docs 归档 + 发布前构建验证） | L3 | P0 | Done | release/T36-v1.0.3 | 版本统一到 1.0.3（Rust+Node 发布，Python 仅同步）；docs/ 根目录只保留 task_progress；Rust `cargo test` 通过；`cargo publish --dry-run -p nervusdb-temporal` 通过（发布需先 temporal 再 core）；Node CI 等价构建/测试通过；`npm publish --dry-run` 通过 |
| T37 | UniFFI 多语言绑定：以 C ABI Statement 为唯一硬契约（Python 重做、扩展 Swift/Kotlin/Ruby） | L3 | P0 | Done | #31 | Python 重做已落地（同步真流式）；Node 门禁与真流式修复拆到 T38；`nervusdb.h` 不改 ABI |
| T38 | Node 真流式 Statement + 契约门禁（对齐 `nervusdb.h`） | L3 | P0 | Done | feat/T38-node-contract-ci | Node `prepareV2` 基于 `PhysicalPlan::execute_streaming` 真流式；CI 已加入 `pnpm -C bindings/node check:contract` 门禁 |
| T39 | Rust CLI（查询/流式输出） | L2 | P1 | Done | feat/T39-rust-cli | `nervusdb-cli` 提供 `nervusdb query --db ... --cypher/--file ...`，基于 `execute_streaming` 流式输出 NDJSON |
| T40 | NervusDB v2 Kernel Spec（Property Graph + LSM Segments） | L3 | P0 | Done | feat/T40-v2-kernel-spec | v2 不兼容 v1：新 crate/新磁盘格式；Single-Writer+Snapshot Readers；.ndb+.wal；MemTable 冻结为 L0 runs；多 CSR segments + 显式 compaction；MVP: 单 label、属性仅在 WAL/MemTable、tombstone 删除、WASM 仅 in-memory |
| T41 | v2 Workspace / Crate 结构与边界 | L2 | P0 | Done | docs/T41-v2-workspace-structure | 定义 v2 新 crates（v2-storage/v2-query/v2 facade/v2-cli）与边界；明确 feature gate（WASM in-memory、显式 compaction、durability）；v1 不重构不被打断；M1 先复制 parser/planner，避免早期抽共享 crate |
| T42 | v2 M0：Pager + WAL Replay（Kernel 可验证内核） | L3 | P0 | Done | feat/T42-v2-m0-pager-wal | 新增 `nervusdb-v2-storage`：实现 page store（8KB）+ bitmap 分配 + redo WAL（len+crc）+ replay；提供最小集成测试（崩溃/尾部损坏 WAL 可恢复） |
| T43 | v2 M1：IDMap + MemTable + Snapshot（Log-Structured Graph） | L3 | P0 | Done | feat/T43-v2-m1-idmap-memtable-snapshot | 实现 I2E 持久化（E2I 启动重建）、邻接 MemTable + tombstone、commit 冻结 L0Run、Snapshot 读隔离、WAL 扩展为图语义事件与 replay |
| T44 | v2 M2：CSR Segments + 显式 Compaction | L3 | P0 | Done | feat/T44-v2-m2-csr-compaction | 实现内存 CSR segment + `GraphEngine::compact()`（显式 compaction）并扩展 Snapshot 读路径支持 segments；持久化/manifest 原子切换在 T45 |
| T45 | v2 Durability / Checkpoint / Crash Model | L3 | P0 | Done | feat/T45-v2-durability-manifest | CSR 段持久化（meta+blob pages）+ WAL ManifestSwitch/Checkpoint；recovery 按 checkpoint 跳过已固化 tx；保证 `.ndb` fsync 先于 WAL manifest 提交 |
| T46 | v2 Public API Facade（Rust） | L2 | P0 | Done | feat/T46-v2-facade-api | 新增 `nervusdb-v2` facade：`Db/ReadTxn/WriteTxn` 最小 API（open/begin_read/begin_write/compact/checkpoint）并提供 smoke test |
| T47 | v2 Query ↔ Storage 边界 | L3 | P0 | Done | feat/T47-v2-api-boundary | 新增 `nervusdb-v2-api` 定义 `GraphStore/GraphSnapshot` streaming 契约；`nervusdb-v2-storage::GraphEngine` 实现该 trait 并补测试 |
| T48 | v2 Benchmarks & Perf Gate | L2 | P1 | Done | feat/T48-v2-bench-perf-gate | 新增 `bench_v2`（insert + neighbors hot/cold + M1/M2 对比）与 `docs/perf/V2_BENCH.md`；提供 `scripts/v2_bench.sh` 便捷落盘 |
| T49 | v2 Crash Gate | L3 | P1 | Done | feat/T49-v2-crash-gate | 新增 `nervusdb-v2-crash-test`（driver/writer/verify）+ `crash-gate-v2.yml`（PR 小跑/定时大跑），验证 manifest/segment 与邻接一致性 |
| T50 | v2 M3：Query Crate（复用 Parser/Planner 的落地） | L3 | P0 | Done | #51 | 新增 `nervusdb-v2-query`（独立于 v1）；务实策略：复制 v1 `query/{ast,lexer,parser,planner}`，先保证可编译与最小语法子集（设计文档见 `docs/design/T50-v2-m3-query-crate.md`） |
| T51 | v2 M3：Executor MVP（基于 GraphSnapshot 的流式算子） | L3 | P0 | Done | #53 | 定义最小算子集（Scan/Expand/Filter/Project/Limit），Pull-based streaming；扩展 `GraphSnapshot` 支持 nodes/label/external/tombstone，并在 v2 storage 实现；新增最小 executor scaffold |
| T52 | v2 M3：Query API（prepare/execute_streaming + 参数） | L3 | P0 | Done | #55 | `nervusdb-v2-query` 提供 `prepare/PreparedQuery::execute_streaming/Params`；支持 `RETURN 1` 与单跳 `MATCH (n)-[:<u32>]->(m) RETURN ... LIMIT ...`；附测试覆盖 |
| T53 | v2 M3：Query Tests + CLI 验收路径 | L2 | P0 | Done | #57 | 增加 v2 query 端到端集成测试（v2-storage + v2-query）；CLI 新增 `nervusdb v2 query`，流式 NDJSON 输出 |
| T54 | v2 属性存储层（Property Storage Layer） | L3 | P0 | WIP | feat/T54-v2-property-storage | 实现节点/边属性存储（MemTable/L0Run/WAL/API）；为 Filter/Create 算子提供基础；设计文档见 `docs/design/T54-v2-property-storage.md` |
| T57 | v2.0.0 正式版发布（Spec 6.3 实现） | L3 | P0 | Plan | feat/v2.0.0 | 稳定公开 Rust API；Cypher 基础读写闭环测试覆盖；数据一致性测试（tombstone/crash/compaction）；性能基准方法论；设计文档见 `docs/design/T57-v2.0.0-release.md` |

## 阻塞任务

| ID | Task | Dependency | Status |
|:---|:-----|:----------:|:------:|
| T54 | v2 属性存储层 | - | WIP |
