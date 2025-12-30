# Design T104: Query `EXPLAIN` (Plan Introspection)

> **Status**: Draft
> **Parent**: T100 (Architecture)
> **Risk**: Low

## 0. The Real Problem

现在的 v2 query 是“能跑就行”的 M3 子集。

但一旦查询稍微复杂一点（WHERE/LIMIT/ORDER BY/VarLen），你根本不知道 planner 生成了什么 plan。
没有可视化，就只能靠猜，然后写出一堆垃圾 workaround。

## 1. MVP Scope (Stupid but Clear)

我们先做最小集：

- 语法：`EXPLAIN <query>`
- 行为：**只返回 plan，不执行 query**
- 输出：返回 **1 行**，单列：
  - 列名：`plan`
  - 值：`STRING`（一个稳定的 plan 文本）

这就够了：先能看到 planner 的结构，再谈优化器。

## 2. Compatibility Rule

**Never break userspace**：

- 不修改 `ast::Query` / `ast::Clause` 这些 public 类型（避免新增 enum variant 造成下游匹配编译失败）。
- 不修改 `executor::Plan`（同理）。

实现上：在 `query_api::prepare()` 做一个轻量的前置解析：

- 如果输入以 `EXPLAIN` 开头：剥掉前缀，按普通 query 编译出 `Plan`，然后把 `render_plan(plan)` 的结果缓存到 `PreparedQuery`。
- `execute_streaming()` 直接返回该字符串，不触发任何读取/写入。

## 3. Plan Rendering

MVP 的“稳定输出”标准很简单：

- 递归打印 `Plan` 的节点类型 + 关键字段
- 表达式（如 WHERE predicate）允许用 `Debug` 输出（后续如果需要再做 pretty printer）

## 4. Tests

- `EXPLAIN RETURN 1`：返回 1 行，包含列 `plan`，值包含 `ReturnOne`
- `EXPLAIN MATCH (n) RETURN n`：plan 包含 `NodeScan`
- `EXPLAIN CREATE (n)`：依然能返回 plan（不执行写入）

