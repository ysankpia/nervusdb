# T46: v2 Public API Facade（Rust First，绑定后置）

## 1. Context

v2 不兼容 v1，需要一个清晰的 public API facade，屏蔽底层实现细节（Pager/WAL/LSM/segments），并为未来 Node/Python/FFI 冻结契约做准备。

## 2. Goals

- Rust API 作为第一公民（先对 Rust 好用、可测试）
- API 面最小、稳定、可扩展：
  - open/close
  - begin_read/begin_write
  - statement/iterator（为 query 层做准备）
- WASM：提供同名 API，但后端选择 in-memory（不共享磁盘格式）

## 3. Non-Goals

- 不复用 v1 的 ABI/符号
- 不在此阶段冻结 C ABI（等 query/executor 稳定后再做）

## 4. Proposed Rust API（最小）

```text
struct Db { ... }

impl Db {
  fn open(path: &Path, opts: OpenOptions) -> Result<Db>;
  fn begin_read(&self) -> ReadTxn;
  fn begin_write(&self) -> WriteTxn;
  fn compact(&self) -> Result<()>;      // M2
  fn checkpoint(&self) -> Result<()>;   // T45
}

struct ReadTxn { snapshot: Snapshot, ... }
struct WriteTxn { ... }

// 图原语（M1/M2）
WriteTxn::create_node(external_id, label) -> InternalNodeId
WriteTxn::create_edge(src, rel, dst)
ReadTxn::neighbors(src, rel?) -> impl Iterator<...>
```

> 说明：query/statement API（prepare/step/column_*) 在 T47 之后再设计。

## 5. Error Model

- 错误类型分层：
  - IO / corruption / protocol violation / invalid input
- 明确哪些错误是“用户错误”，哪些是“数据损坏”（不可恢复）

## 6. WASM Policy

- `wasm32`：`Db::open` 返回 in-memory 引擎（或提供 `open_in_memory()`）
- 保证 API 一致，但不保证磁盘互通

