# T17: 真流式执行器（消除 collect）

## 1. Context

当前 `nervusdb_step()` 在首次调用时执行 `iter.collect()`，将所有结果加载到内存。这对大结果集会导致 OOM。

根本原因：`ExecutionPlan::execute` 返回的迭代器生命周期绑定到 `ExecutionContext`，无法跨 FFI 边界保持。

## 2. Goals

- `nervusdb_step()` 每次调用只从存储层拉取一行
- 支持百万级结果集而不 OOM
- 保持 API 兼容性

## 3. Solution

### 方案 A：Arc<Database> 包装

```rust
// 修改 Database 为 Arc 包装
pub struct DatabaseHandle(Arc<DatabaseInner>);

// 执行器返回 'static 迭代器
fn execute(plan: PhysicalPlan, db: Arc<DatabaseInner>, params: HashMap<...>) 
    -> Box<dyn Iterator<Item = Result<Record>> + Send + 'static>
```

影响范围：
- `Database` 结构体重构
- 所有 FFI 函数签名
- Node/Python 绑定层

### 方案 B：Generator/Coroutine（nightly）

使用 Rust nightly 的 generator 特性，但会锁定 nightly 版本。

### 推荐：方案 A

## 4. Testing Strategy

- 单元测试：100 万行结果集，内存峰值 < 10MB
- 集成测试：并发 step() 调用

## 5. Risks

- 破坏性 API 变更，需要 bump ABI 版本
- 需要审计所有 `&Database` 使用点
