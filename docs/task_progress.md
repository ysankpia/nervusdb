| ID | Task | Complexity | Priority | Status | Branch/PR | Notes |
|:---|:-----|:---------:|:-------:|:------:|:---------|:------|
| T1 | 索引精简并添加字符串缓存/读事务复用以提升 NervusDB 写读性能 | L2 | P0 | Done | perf/T4-node-bulk-resolve | 索引收敛到 `SPO/POS/OSP`；读事务与表句柄复用；写路径字符串缓存改为真 LRU |
| T2 | 清理 Node 侧 `.synapsedb/.pages` 时代遗留（归档/删除） | L3 | P0 | Done | perf/T4-node-bulk-resolve | 删除 `.pages/WAL/维护工具` 等旧世界代码与测试；收口 open options；CLI 去 `synapsedb` 命名 |
| T3 | 重写 Rust interning：使用 `lru` crate 替换伪 LRU | L2 | P0 | Done | perf/T4-node-bulk-resolve | `WriteTableHandles` 真 LRU；写路径确保走 handles；commit 后读缓存失效避免脏读 |
| T4 | Node 吞吐修复：提供“批量返回字符串 triples”的 Native API | L3 | P0 | Done | perf/T4-node-bulk-resolve | 新增 `queryFacts/readCursorFacts`；TS 外壳优先使用并降级；避免 per-triple 3 次 `resolveStr()` |
| T5 | Fuck-off Test：`kill -9` 下的数据一致性验证 | L3 | P0 | WIP | feat/T5-fuck-off-test | 新增 `nervus-crash-test`：driver/writer/verify，默认手动运行不进 CI |
| T6 | 冻结并对齐 `nervusdb.h`：最小稳定 C 契约（含 Cypher 执行） | L3 | P0 | Plan | - | 目标：1.0 后三个月不改头文件；ABI/内存释放规则写清楚 |
