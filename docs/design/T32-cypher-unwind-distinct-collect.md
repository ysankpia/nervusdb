# T32: Cypher UNWIND + DISTINCT + COLLECT 测试覆盖

## 1. Context

当前 Cypher 解析器对 `UNWIND` 与 `RETURN DISTINCT` 直接返回 `NotImplemented`，导致常见查询表达力不足。与此同时，`COLLECT` 已在执行器中实现但缺少测试覆盖，行为未被锁定。

目标是补齐“SQLite 级别”必须的查询能力，同时保持嵌入式定位：小、稳、可预测。

## 2. Goals

- 支持 `UNWIND <expr> AS <var>` 作为行生成器。
- 支持 `RETURN DISTINCT` / `WITH DISTINCT` 的结果去重。
- 为 `COLLECT` 添加测试，锁定现有行为。

**Non-Goals**

- 不引入 `CALL PROCEDURE / YIELD`。
- 不引入 Cypher DDL（索引、约束）。
- 不引入正则匹配（`=~`）。

## 3. Solution

### 3.1 AST

新增：
- `Clause::Unwind(UnwindClause)`
- `UnwindClause { expression: Expression, alias: String }`

### 3.2 Parser

- 解析 `UNWIND <expr> AS <identifier>`。
- 移除 `UNWIND` 的 `NotImplemented` 快速失败。
- `RETURN DISTINCT` / `WITH DISTINCT` 设置 `distinct = true`。

### 3.3 Planner

新增物理算子：
- `PhysicalPlan::Unwind(UnwindNode { input, expression, alias })`
- `PhysicalPlan::Distinct(DistinctNode { input })`

规划规则：
- `UNWIND` 作为管线节点：若前序 plan 存在则串联；否则使用空输入（单条空 Record）作为起点。
- `DISTINCT` 插入在 `Project` 之后、`Sort/Skip/Limit` 之前，保证去重语义与 Cypher 一致。

### 3.4 Executor

**UNWIND**
- 对每条输入记录，求值 `expression`：
  - `Expression::List` 直接展开。
  - 列表 JSON 字符串（来自 list literal/comprehension/collect）通过现有解析函数展开。
- 为每个元素生成新 record，并写入 `alias`。

**DISTINCT**
- 对输入记录生成稳定 key（按字段名排序 + 值串行化）。
- 用 `HashSet` 过滤重复行；保留首次出现的记录。

### 3.5 Tests

- 将现有“UNWIND/DISTINCT 失败”测试改为成功用例。
- 新增 `UNWIND` 行生成测试。
- 新增 `DISTINCT` 去重测试（含重复节点/值）。
- 新增 `COLLECT` 测试：验证返回列表包含期望值（不强绑定内部字符串格式）。

## 4. Testing Strategy

- `cargo test -p nervusdb-core --tests cypher_query_test`
- 重点验证：
  - `UNWIND` 行数正确
  - `DISTINCT` 去重结果正确
  - `COLLECT` 输出包含预期元素

## 5. Risks

- `DISTINCT` 需要物化去重，可能增加内存使用。
- `UNWIND` 非列表输入的处理需明确（建议报错而非静默忽略）。
- `COLLECT` 行为目前为字符串拼接，测试会锁定现有语义；后续若要标准化为 JSON，需要显式升级。
