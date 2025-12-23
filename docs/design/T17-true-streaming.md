# T17: 真流式执行器（消除 collect）

## 1. Context

当前 `nervusdb_step()` 在首次调用时执行 `iter.collect()`，将所有结果加载到内存。这对大结果集会导致 OOM。

根本原因：`ExecutionPlan::execute` 返回的迭代器生命周期绑定到 `ExecutionContext`，无法跨 FFI 边界保持。

## 2. Goals

- `nervusdb_step()` 每次调用只从存储层拉取一行
- 支持百万级结果集而不 OOM
- 保持 API 兼容性

## 3. Solution (已实现)

### 方案 A：Arc<Database> 包装 ✅

```rust
// DatabaseHandle 包装 Arc<Database>
struct DatabaseHandle {
    db: Arc<Database>,
}

// ArcExecutionContext 持有 Arc<Database>
pub struct ArcExecutionContext {
    pub db: Arc<Database>,
    pub params: Arc<HashMap<String, Value>>,
}

// execute_streaming 返回 'static 迭代器
impl PhysicalPlan {
    pub fn execute_streaming(
        self,
        ctx: Arc<ArcExecutionContext>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'static>, Error>
}
```

### 实现细节

1. `nervusdb_open` 创建 `DatabaseHandle { db: Arc::new(db) }`
2. `nervusdb_prepare_v2` 获取 `Arc<Database>` 并创建 `ArcExecutionContext`
3. `execute_streaming` 返回真正的惰性迭代器，无 `collect()`
4. `nervusdb_step` 每次调用 `iter.next()` 拉取一行

## 4. Testing Strategy

- 单元测试：所有现有测试通过
- 集成测试：FFI roundtrip 测试通过

## 5. Risks

- `Arc<Database>` 不是 `Send + Sync`，但 FFI 调用是单线程的，所以没问题
- 使用 `#[allow(clippy::arc_with_non_send_sync)]` 抑制警告
