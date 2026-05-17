# Design T105: Query `MERGE` (Idempotent Create)

> **Status**: Draft
> **Parent**: T100 (Architecture)
> **Risk**: Medium (write semantics)

## 0. The Real Problem

`CREATE` 每次都写新节点/新边，重复执行就会制造垃圾数据。

真正的应用写入需要一个“幂等”的入口：同一条语句跑两次，第二次不应该再创建重复实体。

这就是 `MERGE`。

## 1. MVP Scope (Stupid but Clear)

只做最小、可测试、不会把系统复杂度炸穿的子集：

### 1.1 语法

- 单节点：`MERGE (n {k: v, ...})`
- 单跳：`MERGE (a {k: v})-[:REL]->(b {k: v})`

### 1.2 行为

- MERGE **不执行 RETURN pipeline**（M3 里先只支持“单条写语句”）。
- 返回值沿用现有 write API：`execute_write()` 返回 **本次实际创建的实体数量**：
  - 单节点：新建节点 → `1`；命中已存在 → `0`
  - 单跳：可能创建 0~3（src/dst node + edge）

### 1.3 匹配规则（MVP）

- 节点匹配条件：
  - label（若给出）必须匹配
  - property map 中的每个键必须存在且值相等
- 关系匹配条件：
  - `src`/`dst` 节点分别按上述规则匹配/创建
  - 边匹配：存在一条 `src -[:REL]-> dst` 即视为命中（关系属性匹配留到后续）

### 1.4 明确限制（fail-fast）

为了避免“看起来支持但实际乱来”的坑：

- MERGE 节点 **必须**提供非空 property map（否则没有稳定 identity）
- 只支持 pattern elements 为 `1` 或 `3`
- 只支持 `->` 方向
- 只取第一个 label / 第一个 rel type（多 label/type 直接 fail-fast，后续再扩展）

## 2. Compatibility Rule

**Never break userspace**：

- 不新增/修改 `ast::Clause` / `executor::Plan` 这些 public enum（新增 variant 会让下游 match 直接炸编译）。
- MERGE 的“语义差异”通过 `PreparedQuery` 的 **私有字段**表达（对外 API 不破坏）。

实现上：

- `compile_m3_plan()` 仍然产出 `Plan::Create { pattern }`（作为写入载体）
- 额外返回一个内部标志 `write_semantics = Merge`
- `PreparedQuery::execute_write()` 根据该标志选择 `execute_create` 或 `execute_merge`

## 3. Tests

- `MERGE (n {name:'Alice'})`：
  - 第一次返回 `1`，第二次返回 `0`
  - `MATCH (n) WHERE n.name='Alice' RETURN n` 只应得到 1 条记录
- `MERGE (a {name:'A'})-[:1]->(b {name:'B'})`：
  - 第一次返回 `3`（2 nodes + 1 edge），第二次返回 `0`

