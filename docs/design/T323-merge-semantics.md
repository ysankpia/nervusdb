# T323: MERGE Full Semantics（ON CREATE / ON MATCH）

## 1. 概述

在 v2 M3 的 `MERGE`（T105：幂等写入）基础上，补齐 Cypher 常用的条件写入子句：

- `ON CREATE SET ...`：当本次 `MERGE` 发生“创建”（节点/边任意一个被创建）时执行
- `ON MATCH SET ...`：当本次 `MERGE` 完全命中且不需要创建任何实体时执行

目标是让 `MERGE` 具备可用的 upsert 能力，同时保持 v2 M3 现有的 fail-fast 与兼容性原则。

## 2. 需求与约束

### 2.1 语法（v2 M3 子集）

支持：

- 单节点：
  - `MERGE (n {k: v}) ON CREATE SET n.a = 1 ON MATCH SET n.a = 2`
- 单跳：
  - `MERGE (a {k:v})-[r:1]->(b {k:v}) ON CREATE SET r.w = 1`

不支持（本任务 out of scope，保持 fail-fast）：

- `MERGE` 链式 pipeline（`MERGE ... SET ... RETURN ...` 等）
- `ON CREATE` / `ON MATCH` 里除了 `SET` 之外的子句
- `SET n:Label` / `REMOVE n:Label`（标签写入由 T322 系列任务覆盖）

### 2.2 行为

- `PreparedQuery::execute_write()` 的返回值语义不变：只返回 **本次实际创建的实体数量**（node/edge），不包含属性更新次数。
- `ON CREATE SET` / `ON MATCH SET` 只允许写入 `MERGE` pattern 中已绑定的变量（节点变量、以及可选的关系变量）。
- 表达式求值沿用现有 `SET` 的能力范围（当前以 evaluator 的实现为准）。

### 2.3 兼容性原则（不破坏 userspace）

延续 T105 的约束：

- **不新增/修改** `ast::Clause`、`executor::Plan` 等 public enum（避免下游 `match` 被迫改代码）。
- 新增语义通过 `PreparedQuery` 的 **私有字段**表达，并在 `execute_write()` 走 merge 专用执行路径。

## 3. 测试用例设计（TDD）

### 3.1 Node MERGE

1. `MERGE (n {name:'Alice'}) ON CREATE SET n.age = 1 ON MATCH SET n.age = 2`
   - 第一次执行：创建节点，`n.age == 1`，返回 created = 1
   - 第二次执行：命中节点，`n.age == 2`，返回 created = 0

### 3.2 Edge MERGE

1. `MERGE (a {name:'A'})-[r:1]->(b {name:'B'}) ON CREATE SET r.weight = 1 ON MATCH SET r.weight = 2`
   - 第一次执行：创建 2 nodes + 1 edge，`r.weight == 1`，返回 created = 3
   - 第二次执行：命中 edge，`r.weight == 2`，返回 created = 0

## 4. 设计方案

### 4.1 Parser：收集 Merge 子句的 ON 子句（不改 AST）

- 扩展 `parse_merge()`：
  - 在读取 pattern 后，吞掉 `ON (CREATE|MATCH) SET ...` 序列
  - 将解析出的 `SetClause` 按 `on_create` / `on_match` 收集到 **parser 私有 side-channel**
- 新增 crate 内部方法：
  - `Parser::parse_with_merge_subclauses()` → `(Query, Vec<MergeSubclauses>)`
  - `Parser::parse()` 仍返回 `Query`，但能“吃掉” ON 子句（低层 parse 不再报错）。

### 4.2 Planner：把 ON 子句编译成 merge 执行参数

- 继续让 `MERGE` 编译为 `Plan::Create { pattern }`
- `CompiledQuery/PreparedQuery` 增加私有字段：
  - `merge_on_create_items: Vec<(String, String, Expression)>`
  - `merge_on_match_items: Vec<(String, String, Expression)>`

### 4.3 Executor：execute_merge 条件执行 SET

- `execute_merge(plan, snapshot, txn, params, on_create_items, on_match_items)`
  - 先按现有逻辑完成“find-or-create”并得到绑定变量 → `Row`
  - 若本次 created_count > 0：执行 `on_create_items`；否则执行 `on_match_items`
  - 写入使用现有 `set_node_property / set_edge_property`

## 5. 实施步骤

1. 新增测试（先红）：`nervusdb/tests/t323_merge_semantics.rs`
2. Parser 增强：支持吞掉并收集 `ON CREATE/MATCH SET`
3. Planner/PreparedQuery 透传 merge 子句的 set items
4. Executor 执行：在 merge 完成后按条件执行 SET
5. 跑本地相关测试：`cargo test -p nervusdb --test t323_merge_semantics`

## 6. 风险与折衷

- 当前 evaluator 求值依赖 read snapshot：`ON CREATE SET` 的表达式若依赖“同一语句内刚写入的属性”，可能不可见；测试与文档先按 fail-fast/限制规避，后续再单独提升事务内读一致性。

