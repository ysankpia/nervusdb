# T13: Node Statement API（对标 T10，拆掉 V8 对象爆炸）

## 1. Context

Node 绑定当前的 `executeQuery()` 会把查询结果一次性返回为 `Vec<HashMap<String, serde_json::Value>>`，这等价于让 N-API 在 JS 堆里创建 N 行 × 多列的对象图。

当结果集达到百万级时，这不是“慢”，而是 **GC 自杀**：CPU 被迫用于搬运堆内存，而不是业务执行。

我们已经在 C ABI（T10）里完成了 SQLite 风格的 `prepare_v2 → step → column_* → finalize`。Node 侧必须对齐同一套契约，把“按需转换”作为第一原则。

## 2. Goals

- **解决 V8 对象爆炸**：不再要求 JS 一次性承载大结果集。
- **对齐 T10 契约**：Node 侧提供 `prepareV2/step/column_* /finalize`，语义与列类型映射保持一致。
- **兼容不破坏**：保留 `executeQuery()`（旧 API 继续可用），但新增 Statement 路径给大结果集使用。

## 3. Non-Goals

- 不改 `nervusdb-core` 的执行器/结果表示（不做列式/惰性执行）。
- 不做 Node 侧“零拷贝 JS 对象”魔法（那是 1.1/2.0 的事情）。
- 不追求 100% Cypher 语义覆盖（只写清“支持子集”）。

## 4. Solution

### 4.1 Node Native：Statement 句柄（Option A：Rust 侧 Materialize）

- 新增 `StatementHandle`：
  - `columns: Vec<String>`：列名顺序（优先从 `RETURN` 推导；否则从结果 key 集合稳定排序）
  - `rows: Vec<Vec<Value>>`：每行按列展开后的值（Materialize，紧凑内存）
  - `next_row/current_row`：游标状态
- `DatabaseHandle.prepareV2(query, params?) -> StatementHandle`
  - 参数解析复用现有 `executeQuery` 的 JSON-serializable 约束
  - 执行走 `Database::execute_query_with_params`（不改 core）
- `StatementHandle.step() -> boolean`
  - 推进游标；有行返回 `true`，结束返回 `false`
- `StatementHandle.columnType(i)` 与 `column_*` 系列
  - 只在 `column_*` 被调用时才把 Rust 值转换成 JS 值
  - Node/Relationship 分别输出 `bigint` 与 `{subjectId,predicateId,objectId}`（对标 C 的 `nervusdb_relationship`）
- `finalize()` 释放缓存（并允许 GC 自动回收作为兜底）

### 4.2 TS 层：提供“流式/Statement”入口

- 暴露 `db.cypherPrepare(statement, params?) -> CypherStatement`
  - 用户可用 Statement 模式按行读取（对标 C 用户体验）
- 提供 `db.cypherQueryStream(...)`（可选）
  - 用 Statement 实现流式迭代，避免一次性构建大数组

### 4.3 README：去“实验性”与支持说明下沉

- README 首页移除“实验性”字眼
- 改为“Cypher 查询支持（子集）”，并链接到 `docs/cypher_support.md`（写清支持范围与限制）

## 5. Testing Strategy

- Rust：
  - `cargo test --workspace`
- Node：
  - 新增 native 单测：`prepareV2/step/column_*` 基本行为（列数/列名/类型/值正确；finalize 后报错或返回空）
  - 现有 `vitest .native.test.ts` 继续通过

## 6. Risks

- **内存占用转移**：Materialize 把内存压力从 JS 堆转移到 Rust 堆（但仍显著更紧凑/可控）。
- **列顺序/命名**：必须与 `RETURN` 顺序一致，否则上层会踩坑；需要稳定推导规则与测试覆盖。
- **API 扩展**：新增方法必须避免与现有命名冲突；保留旧 API，默认不破坏用户。

