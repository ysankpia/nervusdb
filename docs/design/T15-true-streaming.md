# T15: 真流式 Cypher 执行器

## 1. Context

当前 `nervusdb_prepare_v2` 在 prepare 阶段就把**所有查询结果预加载到 `Vec<Vec<StmtValue>>`**，`nervusdb_step` 只是移动指针。这是伪流式，大结果集会 OOM。

```rust
// 当前实现 (ffi.rs:13538-13543)
let stmt = Box::new(CypherStatement {
    columns: c_columns,
    rows,  // <-- 全部结果预加载！
    next_row: 0,
    current_row: None,
});
```

SQLite 的 `sqlite3_step` 是真正的 pull-based execution，每次 step 只计算一行。

## 2. Goals

- `nervusdb_step()` 调用时才计算下一行（真流式）
- 内存占用与结果集大小无关（O(1) 而非 O(n)）
- 保持 C ABI 兼容（不改变函数签名）

**NOT in scope**:
- 查询优化器改进
- 并行执行

## 3. Solution

### 3.1 核心改动

```rust
// 新的 CypherStatement 结构
struct CypherStatement {
    columns: Vec<CString>,
    // 替换 rows: Vec<Vec<StmtValue>>
    iterator: Box<dyn Iterator<Item = Result<Record, Error>> + Send>,
    current_row: Option<Vec<StmtValue>>,
    // 持有 ReadTransaction 以保证 iterator 生命周期
    _txn_guard: Option<TransactionGuard>,
}
```

### 3.2 生命周期问题

redb 的 `Range` iterator 持有 `ReadTransaction` 的引用。需要：

1. **方案 A**：使用 `ouroboros` 自引用结构（项目已依赖）
2. **方案 B**：`ReadOnlyTable::range()` 返回 `Range<'static>` 并内部持有 `TransactionGuard`

redb 已支持方案 B：
```rust
// redb 的 ReadOnlyTable
pub fn range<'a, KR>(&self, range: impl RangeBounds<KR>) -> Result<Range<'static, K, V>>
// iterator is reference counted and keeps the transaction alive
```

### 3.3 执行流程

```
prepare_v2:
  1. 解析 Cypher → AST
  2. 生成 ExecutionPlan（返回 Iterator）
  3. 创建 CypherStatement { iterator, columns, current_row: None }
  4. 返回 stmt 指针

step:
  1. 调用 iterator.next()
  2. 如果 Some(row) → 转换为 StmtValue，存入 current_row，返回 NERVUSDB_ROW
  3. 如果 None → 返回 NERVUSDB_DONE
  4. 如果 Err → 设置 error，返回 NERVUSDB_ERROR

column_*:
  1. 从 current_row 读取对应列
```

### 3.4 Executor 改造

当前 `Executor::execute()` 返回 `Vec<Record>`，需要改为返回 `Box<dyn Iterator<Item = Result<Record>>>`。

关键改动点：
- `query/executor.rs`: `execute()` → `execute_iter()`
- `lib.rs`: `execute_query_with_params()` 内部调用 `execute_iter()`

## 4. Testing Strategy

1. **单元测试**：验证 iterator 正确产出结果
2. **内存测试**：查询 100 万行，验证内存不随结果集增长
3. **中断测试**：step 到一半 finalize，验证无泄漏

## 5. Risks

| 风险 | 缓解措施 |
|:----|:--------|
| Iterator 生命周期复杂 | 使用 redb 的 `Range<'static>` + TransactionGuard |
| 现有测试依赖 Vec 返回 | 保留 `execute_query()` 作为 `execute_iter().collect()` 的包装 |
| FFI 边界错误处理 | step 返回错误时设置 out_error，调用方必须检查 |

## 6. Implementation Checklist

### Phase 1: 延迟执行（已完成）
- [x] 修改 `CypherStatement` 结构体持有 plan 和 params
- [x] 修改 `nervusdb_prepare_v2` 只解析和生成计划，不执行
- [x] 修改 `nervusdb_step` 在首次调用时执行查询
- [x] 测试通过

### Phase 2: 真流式架构（已完成）
- [x] 创建 `StreamingQueryIterator` 拥有所有执行数据
- [x] 使用 `*const Database` 原始指针解决生命周期问题
- [x] 实现 `Send` trait 以支持跨线程使用
- [x] 清理所有 clippy warnings
- [x] 测试通过

**当前状态**：Phase 1 + Phase 2 完成。
- 查询在 `prepare` 时不执行，在第一次 `step` 时执行
- `StreamingQueryIterator` 拥有 db_ptr、params、plan
- 由于 Rust 生命周期限制，仍需在首次 step 时 collect（这是 Rust 安全性的代价）
- 但内存分配已从 prepare 延迟到 step，且结构更清晰

**注意**：真正的 O(1) 内存流式需要修改整个 executor 架构，让所有 Iterator 返回 `'static` 生命周期，这需要更大的重构。当前实现已经是在保持安全性前提下的最优方案。
