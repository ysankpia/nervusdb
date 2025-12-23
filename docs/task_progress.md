| ID | Task | Complexity | Priority | Status | Branch/PR | Notes |
|:---|:-----|:---------:|:-------:|:------:|:---------|:------|
| T1 | 索引精简并添加字符串缓存/读事务复用以提升 NervusDB 写读性能 | L2 | P0 | Done | perf/T4-node-bulk-resolve | 索引收敛到 `SPO/POS/OSP`；读事务与表句柄复用；写路径字符串缓存改为真 LRU |
| T2 | 清理 Node 侧 `.synapsedb/.pages` 时代遗留（归档/删除） | L3 | P0 | Done | perf/T4-node-bulk-resolve | 删除 `.pages/WAL/维护工具` 等旧世界代码与测试；收口 open options；CLI 去 `synapsedb` 命名 |
| T3 | 重写 Rust interning：使用 `lru` crate 替换伪 LRU | L2 | P0 | Done | perf/T4-node-bulk-resolve | `WriteTableHandles` 真 LRU；写路径确保走 handles；commit 后读缓存失效避免脏读 |
| T4 | Node 吞吐修复：提供“批量返回字符串 triples”的 Native API | L3 | P0 | Done | perf/T4-node-bulk-resolve | 新增 `queryFacts/readCursorFacts`；TS 外壳优先使用并降级；避免 per-triple 3 次 `resolveStr()` |
| T5 | Fuck-off Test：`kill -9` 下的数据一致性验证 | L3 | P0 | Done | feat/T5-fuck-off-test | 新增 `nervus-crash-test`：driver/writer/verify；目标：重启后要么事务前要么事务后，校验字典/索引引用一致 |
| T6 | 冻结并对齐 `nervusdb.h`：最小稳定 C 契约（含 Cypher 执行） | L3 | P0 | Done | feat/T6-ffi-freeze | 收口导出符号；补齐 resolve/exec_cypher/version；写清 ABI/内存释放规则（目标：1.0 后三个月不改头文件） |
| T7 | Node 绑定去插件化 + 修复 Cypher 调用致命 Bug | L3 | P0 | Done | feat/T7-node-thin-binding | 删 `PluginManager`/JS 聚合/TS Cypher；Cypher 只走 Rust Core 执行器；算法接口统一为 `db.algorithms.*` 原生透传 |
| T8 | Temporal 变为 optional feature（Default OFF） | L3 | P0 | Done | feat/T7-node-thin-binding | `nervusdb-core`/N-API 增加 `temporal` feature（默认关闭）；TS 侧 capability guard：未启用直接 fail-fast |
| T9 | Node Tests 纳入 CI（覆盖 Binding ↔ Native） | L2 | P0 | Done | feat/T7-node-thin-binding | CI 增加 node job（Ubuntu+macOS）：typecheck + TS-only tests + native addon smoke + crash-smoke |
| T10 | C API 二进制 Row 迭代器（替代 exec_cypher JSON 热路径）+ ABI 冻结策略 | L3 | P0 | Done | feat/T10-binary-row-iterator | 保留 `nervusdb_exec_cypher`（JSON）兼容；新增 stmt/step/column* 最小 API；目标：减少序列化与复制成本，并为 1.0 冻结 `nervusdb.h` 提供硬契约 |
| T11 | 性能重测与报告刷新（修正 redb 基线 + 补充 T10 stmt 对比） | L1 | P0 | WIP | docs/T11-perf-refresh | 修正 `bench_compare` 的 redb 方法论；更新 `PERFORMANCE_ANALYSIS.md`（写清测量边界/环境）；补充 exec_cypher vs stmt 数据 |
