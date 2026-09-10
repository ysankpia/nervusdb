# GraphLite-RS 1.0 生产级完整版研发任务书 (Master Engineering Blueprint)

## 目标定位：打造真正可独立落地的“图数据库界的 SQLite”
不仅是一个玩具原型，而是一个可以直接嵌入到任何生产级软件、移动端、边缘设备中的单文件持久化图数据库引擎。

---

## 🛠️ 五大核心研发战役（必须全部实现完成）

### 战役一：突破物理内存瓶颈的存储与事务内核 (Storage & ACID)
1. **解决受限缓冲池与超大事务的根本冲突**：
   - 彻底修复 `test_07`、`test_10`（Buffer pool capacity exceeded）与 `test_tx_commit_failure_memory_cleanup`；
   - 引入符合 SQLite 理念的 **STEAL 策略 + WAL 撤销恢复**（或事务级 Spool 溢出暂存机制）：在 1MB/2MB 极小内存下，单事务能够写入数万节点与数十万边而不发生 OOM，且在 Rollback 或崩溃时绝对不泄漏任何脏数据到主库；
   - 保持全部 26 个集成测试 100% 通过。

### 战役二：工业级 Cypher 查询引擎闭环 (Cypher Engine 1.0)
现有的 Cypher 仅支持最简基础语句，必须完整实现以下生产级特性：
1. **数据修改与删除算子**：
   - `SET n.prop = value`, `SET n:Label`（更新节点/边属性与追加标签）；
   - `DELETE n`, `DETACH DELETE n`（安全级联删除关联边并进入 Freelist 槽位复用）；
2. **查询修饰与排序分页**：
   - `ORDER BY n.prop ASC / DESC`；
   - `SKIP m LIMIT n` 分页支持；
3. **聚合函数支持**：
   - `RETURN count(n)`, `RETURN sum(e.weight)`, `RETURN avg(...)`, `RETURN min(...)`, `RETURN max(...)`；
4. **多跳与可变长度路径匹配**：
   - 支持 `MATCH (a)-[:REL*1..3]->(b)` 与无向边 `(a)-[:REL]-(b)`。

### 战役三：企业级图算法引擎 (Graph Analytics Engine)
在原有 BFS / Dijkstra 基础上，新增三大工业级图算法：
1. **PageRank 算法**：基于磁盘邻接表与缓冲池的阻尼迭代，支持配置 `damping_factor` 与 `max_iterations`，用于节点影响力评估；
2. **弱连通分量 (WCC / Weakly Connected Components)**：大图孤岛检测与社群划分；
3. **K-Hop 局部子图提取 (K-Hop Subgraph Extraction)**：高效抽取指定节点周围 K 步以内的所有相连节点与边结构。

### 战役四：媲美 sqlite3 的交互式 REPL 终端 (CLI Tooling)
完善 `src/bin/cli.rs`，让 `graphlite-cli` 成为一个功能完备的生产级命令行工具：
- 支持多行 Cypher 输入、历史记录与整齐的 ASCII 表格对齐渲染；
- 支持内置管理命令：
  - `.schema`：打印当前数据库图模式、标签列表、边类型列表及索引分布；
  - `.stats`：实时打印 4KB Buffer Pool 命中率、物理读写次数、文件体积；
  - `.checkpoint`：手动触发全量 WAL 检查点刷盘与截断；
  - `.dump <file>`：导出当前图数据为 Cypher 导入脚本；
  - `.help` 与 `.quit`。

### 战役五：多语言生态与自动化验证 (Multi-Language SDKs & Test Suite)
1. **自动化集成测试套件扩容**：针对新增的 Cypher 语法、算法和事务边界，在 `tests/` 中编写全新的验证用例；
2. **Python / Node.js SDK 对齐**：确保 `bindings/python` 和 `bindings/nodejs` 均能无缝调用新增的 Cypher 语句与图算法；
3. **工作区编译健康度**：`cargo check --workspace` 必须达到 **0 Warnings, 0 Errors**。

---

## 🏁 验收标准（评委最终审查项目）
1. **功能完整度**：上述五大战役的所有功能是否全部实现并有对应测试覆盖；
2. **架构纯洁性**：严格遵守 `AGENTS.md`，保持单文件 + WAL，纯磁盘外存寻址，严禁在内存中偷偷缓存全图；
3. **代码健壮性**：`cargo test --workspace` 全量通过；
4. **生产就绪度**：文档、错误处理（`GraphError` 体系）、代码可维护性达到开源顶级项目水准。
