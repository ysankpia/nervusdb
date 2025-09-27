## TODO: A\* 路径重建与启发式策略

范围：`src/algorithms/pathfinding.ts`

待办项：

1. A\* 的 `reconstructAStarPath()` 现直接返回 `goalNode.path`（构建期维护路径），`cameFrom` 参数未实用。
   - 方案 A：移除未用参数，保留现路径维护策略；
   - 方案 B：改为统一使用 `cameFrom` 回溯重建，减少节点对象负担。
2. 启发式函数接口与默认实现
   - 提供更多启发式（如欧氏/曼哈顿），并在文档中声明“负权边不支持”。

验收标准：

- 两种策略二选一落地并配套单元测试；
- 对外 API 不变，默认启发式为 0（退化为 Dijkstra）。
