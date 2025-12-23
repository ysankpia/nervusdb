# T7: Node 绑定去插件化 + 修复 Cypher 调用致命 Bug

## 1. Context

- 现状：
  - `bindings/node/src/synapseDb.ts` 仍在维护 `PluginManager`，并默认加载 Aggregation/Pathfinding 等插件。
  - `cypherQuery()` 存在致命 bug：把 `statement` 当成 criteria 传给 `store.query(statement as any)`，等价于“随便给点输入就可能全库扫一遍然后当作 Cypher 结果返回”。
  - Cypher 还保留了一套 TS 前端（`plugins/cypher.ts` + `extensions/query/cypher.ts`），违背“绑定层闭嘴”的原则。

## 2. Goals

- **现在就砍（Breaking Change）**：移除 Node 侧插件系统与 JS 聚合逻辑，绑定层退化为“参数转换 + N-API 调用”。
- 修复 `cypherQuery()` 的错误数据返回路径，确保 Cypher **只走 Rust Core 的查询执行器**（Native `executeQuery` / `exec_cypher`）。
- 保留 Cypher 的 `@experimental` 标签，但实现必须正确、可测。
- 输出面向用户的最小 Node API：点/边/属性 + 事务 + Cypher（实验性）+ **图算法 native 透传接口**（如 Path / PageRank）。

## 3. Non-Goals

- 不在 Node 侧保留任何聚合/优化/算法的“回退实现”（宁可没有，也别给错的）；算法接口允许存在，但必须是 **绝对透传** 的 native wrapper。
- 不在本任务里实现 Rust 侧聚合执行器（后续单独 T 任务）。
- 不引入新的二进制 Row 协议（1.0 前先用 JSON/对象返回，后续再做迭代器/二进制）。

## 4. Solution

### 4.1 API 收敛（Breaking Change）

- 删除/停止导出：
  - `PluginManager` / `NervusDBPlugin` / `AggregationPlugin` / `CypherPlugin`（TS 前端）等插件相关 API。
  - `AggregationPipeline`（纯 JS 聚合管道）。
- `NervusDB` 保留为单一入口，但职责变为薄封装：
  - 只持有 `PersistentStore` / native handle。
  - 不再做插件注册/生命周期管理。
  - 图算法接口统一挂在 `db.algorithms.*` 命名空间下（避免把主类搞成“工具箱大杂烩”）。

### 4.2 Cypher 正确路径（P0）

- `NervusDB.cypherQuery()` / `cypherRead()`：
  - 只调用 Native `executeQuery(statement, params)`（Rust Core 执行器）。
  - 若 native 不可用或 experimental 未开启：直接报错（拒绝“错数据”）。
  - 严禁再走 `store.query(...)` 这种事实三元组查询 API。

### 4.3 代码清理策略

- 删除 Node 侧“聪明逻辑”文件与导出：
  - `bindings/node/src/plugins/*`（至少移除 aggregation/cypher/pluginManager；路径算法若仍保留，必须是纯 native wrapper 且不走插件系统）。
  - `bindings/node/src/extensions/query/aggregation.ts`
  - `bindings/node/src/extensions/query/cypher.ts`（TS Cypher 前端）
- 同步更新：
  - `bindings/node/src/index.ts` 的 export surface
  - `bindings/node/src/cli/cypher.ts` 等 CLI 调用点
  - README/示例中不再提插件系统

## 5. Testing Strategy

- Node 单测（以 stub/native handle 为边界）：
  - 回归：`cypherQuery()` 必须调用 native `executeQuery`，并且 **不会**调用 `store.query`。
  - 禁用 native 时，`cypherQuery()` 必须 fail-fast（而不是返回“全库事实”这种错数据）。
- Rust CI 仍保持不变；Node 测试纳入 CI 见 T9。

## 6. Risks

- Breaking change 会打断现有 Node 用户：但 1.0 之前必须干净，不然以后就是坏疽。
- 删除插件系统后，部分“便利方法”会消失：这是刻意的收敛，避免绑定层各写各的逻辑。
