# T52: v2 M3 — Query API（prepare/execute_streaming + 参数）

## 1. Context

v2 的存储内核和 `GraphSnapshot` 边界已经存在，但对外缺一个“能被上层调用”的查询入口。v1 的 `execute_streaming` 形态已经被验证：它让 CLI/Node/FFI 都能消费一个 row iterator，而不是吃一整个 `Vec<Row>`。

v2 M3 必须把这个形态“重新定义为 v2 的公共 API”，并明确：

- 参数怎么传（避免早期引入复杂 runtime）
- 结果行（Row/Value）的最小稳定表示
- 不支持的语法如何 fail-fast（避免半吊子行为）

## 2. Goals

- 为 `nervusdb-v2-query` 提供稳定入口：
  - `prepare()`：解析 + 规划（可选缓存）
  - `execute_streaming()`：在给定 `GraphSnapshot` 上执行并返回 iterator
- 定义 `Params`、`Row`、`Value`、`QueryError` 的最小集合
- 明确与 CLI/绑定的对接方式：上层负责序列化（NDJSON/JSON），query 层只给 typed row

## 3. Non-Goals

- 不做 SQL/Cypher 的统一接口（只做 Cypher）
- 不在 M3 做 plan cache/LRU（后置）
- 不在 M3 暴露事务（读写事务仍由 `nervusdb-v2` facade 管）

## 4. Proposed API（Rust）

建议在 `nervusdb-v2-query` 暴露如下接口（具体命名以实现为准）：

```text
pub struct PreparedQuery { /* AST + Plan */ }

pub fn prepare(cypher: &str) -> Result<PreparedQuery, QueryError>;

impl PreparedQuery {
  pub fn execute_streaming<'a, S: GraphSnapshot + 'a>(
    &'a self,
    snapshot: &'a S,
    params: &'a Params,
  ) -> impl Iterator<Item = Result<Row, QueryError>> + 'a;
}
```

### 4.1 Params

MVP 先支持基础标量：

- `Int(i64)`, `Float(f64)`, `Bool(bool)`, `String(String)`, `Null`

禁止隐式类型转换（减少特殊情况）。

### 4.2 Row / Value

MVP 结果只需要支撑图拓扑返回：

- `Value::NodeId(InternalNodeId)`
- `Value::EdgeKey(EdgeKey)`
- `Value::ExternalId(ExternalId)`（如果 snapshot 支持 resolve）
- 以及标量值（用于 `RETURN 1` / `LIMIT` 相关）

Row 表示：

- `Row { columns: Vec<(String, Value)> }`（最直接、最少魔法）

## 5. Error Model

必须区分三类错误：

- `ParseError`：语法错误
- `NotSupported`：语法在 M3 子集之外（明确报错，不要 silent fallback）
- `ExecutionError`：执行期错误（例如未知 label/rel id、参数缺失）

## 6. Testing Strategy

- API 单测：
  - `prepare()` 对最小子集的成功/失败
  - `params` 缺失 / 类型不匹配时的错误
- 与 T51 的 executor 集成测试复用同一套 graph fixture

## 7. Risks

- 如果 Row/Value 设计不清晰，后续绑定会各自发明一套格式，最后变成兼容地狱
- 如果 NotSupported 不强制，planner/executor 会被迫背上大量边缘语法（这是典型的“坏味道”）

