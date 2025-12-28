# T54: v2 属性存储层（Property Storage Layer）

## 1. Context

v2 存储引擎（M0-M2）已完成图拓扑存储（节点、边、tombstone），但**不支持属性存储**。这导致：
- 无法实现 `WHERE` 子句的属性过滤
- 无法实现 `CREATE` 语句的属性写入
- 无法实现 `RETURN` 中的属性访问

根据评估报告，属性存储层是 v2.0.0-alpha1 的**关键路径**，必须优先实现。

## 2. Goals

- 在 v2 存储层实现节点和关系的属性存储
- 支持属性的写入、读取和删除
- 属性数据持久化到 WAL，并在 MemTable/L0Run 中维护
- 扩展 `GraphSnapshot` trait 支持属性查询
- 为后续 Filter/Create 算子提供基础

## 3. Non-Goals

- **不实现**属性索引（后续任务）
- **不实现**属性压缩/编码优化（MVP 阶段）
- **不实现**属性在 CSR segments 中的持久化（MVP 阶段属性仅在 WAL/MemTable）

## 4. Proposed Solution

### 4.1 属性值类型

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}
```

### 4.2 MemTable 扩展

在 `MemTable` 中添加属性存储：

```rust
pub struct MemTable {
    out: HashMap<InternalNodeId, BTreeSet<EdgeKey>>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
    // 新增：节点属性
    node_properties: HashMap<InternalNodeId, HashMap<String, PropertyValue>>,
    // 新增：边属性（key: EdgeKey）
    edge_properties: HashMap<EdgeKey, HashMap<String, PropertyValue>>,
}
```

### 4.3 L0Run 扩展

在 `L0Run` 中添加属性数据：

```rust
pub struct L0Run {
    txid: u64,
    edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
    // 新增：节点属性
    node_properties: BTreeMap<InternalNodeId, BTreeMap<String, PropertyValue>>,
    // 新增：边属性
    edge_properties: BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
}
```

### 4.4 WAL 记录扩展

新增 WAL 记录类型：

```rust
pub enum WalRecord {
    // ... 现有记录 ...
    SetNodeProperty {
        node: u32,
        key: String,
        value: PropertyValue,
    },
    SetEdgeProperty {
        src: u32,
        rel: u32,
        dst: u32,
        key: String,
        value: PropertyValue,
    },
    RemoveNodeProperty {
        node: u32,
        key: String,
    },
    RemoveEdgeProperty {
        src: u32,
        rel: u32,
        dst: u32,
        key: String,
    },
}
```

**编码格式**：
- `SetNodeProperty`: `[record_type: u8][node: u32][key_len: u32][key: bytes][value_type: u8][value: bytes]`
- `SetEdgeProperty`: `[record_type: u8][src: u32][rel: u32][dst: u32][key_len: u32][key: bytes][value_type: u8][value: bytes]`
- `RemoveNodeProperty`: `[record_type: u8][node: u32][key_len: u32][key: bytes]`
- `RemoveEdgeProperty`: `[record_type: u8][src: u32][rel: u32][dst: u32][key_len: u32][key: bytes]`

### 4.5 GraphSnapshot trait 扩展

在 `nervusdb-v2-api` 中扩展 `GraphSnapshot`：

```rust
pub trait GraphSnapshot {
    // ... 现有方法 ...
    
    fn node_property(&self, iid: InternalNodeId, key: &str) -> Option<PropertyValue>;
    fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue>;
    fn node_properties(&self, iid: InternalNodeId) -> Option<&HashMap<String, PropertyValue>>;
    fn edge_properties(&self, edge: EdgeKey) -> Option<&HashMap<String, PropertyValue>>;
}
```

### 4.6 WriteTxn API 扩展

在 `nervusdb-v2` facade 中扩展 `WriteTxn`：

```rust
impl<'a> WriteTxn<'a> {
    // ... 现有方法 ...
    
    pub fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()>;
    
    pub fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()>;
    
    pub fn remove_node_property(
        &mut self,
        node: InternalNodeId,
        key: &str,
    ) -> Result<()>;
    
    pub fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) -> Result<()>;
}
```

## 5. Implementation Plan

### Phase 1: 核心数据结构（1 周）

1. 定义 `PropertyValue` 枚举
2. 扩展 `MemTable` 添加属性存储
3. 扩展 `L0Run` 添加属性数据
4. 实现 `MemTable::freeze_into_run()` 时复制属性数据

### Phase 2: WAL 支持（1 周）

1. 扩展 `WalRecord` 枚举，添加属性相关记录
2. 实现 `encode_body()` 和 `decode_body()` 方法
3. 更新 `replay_committed()` 支持属性记录
4. 在 `GraphEngine` 的 commit 路径中写入属性 WAL 记录

### Phase 3: API 扩展（1 周）

1. 扩展 `GraphSnapshot` trait
2. 在 `StorageSnapshot` 中实现属性查询方法
3. 扩展 `WriteTxn` API
4. 在 `GraphEngine::WriteTxn` 中实现属性写入

### Phase 4: 测试和集成（1 周）

1. 单元测试：属性存储/读取/删除
2. 集成测试：WAL replay 属性恢复
3. Crash gate：验证属性在崩溃恢复中的一致性
4. 端到端测试：通过 Query API 测试属性功能

## 6. Testing Strategy

### 6.1 单元测试

- `MemTable` 属性存储和冻结
- `L0Run` 属性查询
- WAL 属性记录编码/解码
- `PropertyValue` 序列化/反序列化

### 6.2 集成测试

- 属性写入 → commit → 读取
- WAL replay 恢复属性
- 多个事务的属性合并（L0Run 顺序）
- Tombstone 节点/边的属性清理

### 6.3 Crash Gate

- 扩展 `nervusdb-v2-crash-test` 验证属性一致性
- 验证崩溃恢复后属性数据正确

## 7. Risks

### 7.1 技术风险

- **内存开销**：属性存储在 MemTable 中，大属性可能导致内存压力
  - **缓解**：MVP 阶段接受此限制，后续可通过 compaction 优化

- **WAL 大小**：属性写入会增加 WAL 大小
  - **缓解**：MVP 阶段接受，后续可通过 checkpoint 控制

### 7.2 兼容性风险

- **WAL 格式变更**：新增记录类型需要版本管理
  - **缓解**：v2 仍在开发中，不涉及向后兼容

## 8. Success Criteria

- [x] `MemTable` 支持节点/边属性存储
- [x] `L0Run` 支持属性查询
- [x] WAL 支持属性记录持久化
- [x] `GraphSnapshot` 支持属性查询
- [x] `WriteTxn` 支持属性写入/删除
- [x] 所有单元测试通过
- [x] 集成测试通过
- [x] Crash gate 验证通过

**注意**: CSR segments 中的属性持久化是 v2.x 规划，不在 MVP 范围内。

## 9. Dependencies

- 无外部依赖（使用标准库和现有依赖）

## 10. References

- `docs/memos/v2-status-assessment.md` - 项目状态评估
- `docs/spec.md` - v2 产品规格
- `nervusdb-v2-storage/src/memtable.rs` - MemTable 实现
- `nervusdb-v2-storage/src/wal.rs` - WAL 实现
- `nervusdb-v2-storage/src/snapshot.rs` - Snapshot 实现
