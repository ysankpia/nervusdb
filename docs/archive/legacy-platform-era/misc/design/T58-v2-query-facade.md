# T58: v2 Query Facade DX 优化

## 1. Context

当前用户使用 v2 查询需要引入多个 crate：
```rust
use nervusdb::Db;
use nervusdb_api::GraphSnapshot;
use nervusdb_query::{prepare, Params};
```

目标：提供 "SQLite 体验"——一个入口，统一 API。

## 2. Goals

- 在 `nervusdb` crate 中提供便捷查询方法
- 减少用户代码量
- 保持架构解耦

## 3. Non-Goals

- 不合并 crate（保持架构清晰）
- 不破坏现有 API 兼容性

## 4. Proposed Solution

### 4.1 添加 feature gate

```toml
# nervusdb/Cargo.toml
[features]
default = []
query = ["nervusdb-query"]
```

### 4.2 便捷方法

```rust
// 当 feature = "query" 启用时

impl Db {
    /// 便捷查询方法
    pub fn query(&self, cypher: &str) -> Result<PreparedQuery> {
        prepare(cypher)
    }
}

impl ReadTxn {
    /// 便捷流式查询
    pub fn query_streaming<'a>(
        &'a self,
        cypher: &'a str,
        params: &'a Params,
    ) -> impl Iterator<Item = Result<Row>> + 'a {
        prepare(cypher)
            .and_then(|q| Ok(q.execute_streaming(self.snapshot(), params))
            .unwrap_or_else(|e| Box::new(std::iter::once(Err(e)) as Box<dyn Iterator<Item = _>)
    }
}
```

### 4.3 简化使用

**当前**（需要 3 个 import）:
```rust
use nervusdb::Db;
use nervusdb_query::prepare;
use nervusdb_query::Params;

let db = Db::open("graph.ndb")?;
let query = prepare("MATCH (n)-[:1]->(m) RETURN n, m")?;
let rows: Vec<_> = query.execute_streaming(&db.snapshot(), &Params::new()).collect();
```

**优化后**（1 个 import）:
```rust
#[cfg(feature = "query")]
use nervusdb::Db;

let db = Db::open("graph.ndb")?;
let query = db.query("MATCH (n)-[:1]->(m) RETURN n, m")?;
let rows: Vec<_> = query.execute_streaming(&db.snapshot(), &Default::default()).collect();
```

### 4.4 导出类型

```rust
// lib.rs
#[cfg(feature = "query")]
pub use nervusdb_query::{prepare, Params, PreparedQuery, Row, Value, Error, Result};
```

## 5. Implementation Plan

### Phase 1: 基础设施 (1d)

1. 添加 Cargo.toml feature
2. 添加 re-export 语句
3. 添加 `Db::query()` 方法

### Phase 2: ReadTxn 扩展 (1d)

1. 添加 `ReadTxn::query_streaming()`
2. 文档和示例

### Phase 3: 测试验证 (0.5d)

1. 更新现有测试使用新 API
2. 添加 DX 示例

## 6. Testing Strategy

- 验证新 API 与旧 API 结果一致
- 确保 feature gate 正确工作

## 7. Risks

- 无重大风险
- 向后兼容

## 8. Success Criteria

- [ ] feature = "query" 正确工作
- [ ] `Db::query()` 方法可用
- [ ] `ReadTxn::query_streaming()` 方法可用
- [ ] 文档更新
- [ ] 所有测试通过

## 9. Dependencies

- 无外部依赖
- 依赖 `nervusdb-query` crate

## 10. References

- `nervusdb/src/lib.rs`
- `nervusdb-query/src/query_api.rs`
- `docs/memos/v2-next-steps.md`
