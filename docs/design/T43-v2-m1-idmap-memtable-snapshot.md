# T43: v2 M1 — IDMap + MemTable + Snapshot（Log-Structured Graph）

## 1. Context

T42 已完成 v2 的 M0 内核：`.ndb` page store（8KB + bitmap allocator）+ `.wal` redo log（len+crc）+ replay。

M1 的目标是把“只有页”的内核升级为“能表达图语义、可写可读、支持快照读”的最小图存储（仍然不做 CSR/compaction）。

## 2. Goals（M1 验收）

### 2.1 图语义（MVP）

- 支持：
  - `create_node(external_id: u64, label: LabelId) -> InternalNodeId(u32)`
  - `create_edge(src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId)`
  - `neighbors(src, rel?) -> iterator`
  - tombstone：`tombstone_node(iid)` / `tombstone_edge(key)`（逻辑删除）
- 约束：
  - 节点仅 1 个主 label（宪法）
  - 属性仅存在于 WAL/MemTable（可写但不固化到 columnar pages）

### 2.2 事务与快照

- 单写者：
  - `begin_write()` 获取全局写锁
  - `commit()` 时把当前 MemTable 冻结为不可变 `L0Run` 并发布到全局段列表
- 快照读（关键）：
  - `begin_read()` 返回 `Snapshot`：持有 `Arc<Vec<Arc<L0Run>>>`（以及未来 `Arc<Vec<Arc<CsrSegment>>>`）
  - 读路径只对不可变段集合做 merge，不依赖 WAL offset 做过滤

### 2.3 持久化/恢复

- WAL 即 delta 的持久化形式：
  - 写事务的图语义事件按顺序追加 WAL（默认 commit fsync）
  - 启动时 replay WAL 重建：
    - IDMap（I2E）
    - 已提交事务形成的 L0Run（或合并进当前 MemTable，再统一 freeze）
- 不引入第二份日志

## 3. Non-Goals（M1 不做）

- CSR segment / compaction（M2）
- 属性 columnar 固化（M2+）
- on-disk E2I 索引（B+Tree/Hash）——M1 允许 O(N) 启动重建
- 多 label、在线 schema、在线物理删除
- 并行执行/查询引擎（M1 只提供 storage API）

## 4. Data Structures（核心）

### 4.1 IDs

- `ExternalId(u64)`：用户侧 ID
- `InternalNodeId(u32)`：dense 0..N（数组下标）
- `LabelId(u32)` / `RelTypeId(u32)`：M1 先用内存字典（可选持久化后置）

### 4.2 IDMap（M1 持久化策略）

#### I2E（持久化）

- `.ndb` 中存 `I2E` append-only 数组：
  - index = internal_id
  - value = external_id(u64) + label_id(u32)
- 记录格式（固定 16 bytes，便于顺序写）：
  - `external_id: u64`
  - `label_id: u32`
  - `flags: u32`（预留：tombstone/版本/校验）

> M1 不做“回收/重写 I2E”，只追加。物理回收留给 M2+。

#### E2I（内存重建）

- 启动时从 I2E 扫描重建 `HashMap<ExternalId, InternalNodeId>`。
- 接受 O(N) 启动成本（宪法已拍板）。

#### InternalId 分配

- `next_internal_id: u32` 存在 meta page
- `create_node` 时：
  - 若 external_id 已存在：按语义决定（M1 建议 fail-fast，MERGE 语义交给 query 层）
  - 否则分配新 internal_id，写入 WAL 并更新 I2E

### 4.3 MemTable（可变增量图）

M1 只需要一个“写友好”的邻接表，不追求极致性能。

建议结构：

- `AdjOut: HashMap<InternalNodeId, BTreeMap<EdgeKey, EdgeValue>>`
- `AdjIn` 暂不做（双向邻接后置到 M2/M3；M1 只保证 out-going expand）
- tombstone：
  - `tombstoned_nodes: HashSet<InternalNodeId>`
  - `tombstoned_edges: HashSet<EdgeKey>`

EdgeKey（排序键，宪法）：

- `(src_iid: u32, rel_type_id: u32, dst_iid: u32)`

### 4.4 L0Run（冻结不可变段）

`L0Run` 是一次 commit 产生的不可变增量：

- 包含：
  - `created_nodes: Vec<NodeRecord>`
  - `created_edges: Vec<EdgeRecord>`（按 EdgeKey 排序）
  - `tombstoned_nodes/edges`（同样 key-based）
- 为什么要 materialize 成不可变结构：
  - Snapshot 读只持有不可变 Arc，不需要锁，不需要“WALOffset 过滤”

## 5. WAL Event Semantics（M1 开始用图语义）

M1 WAL types（与 T40 宪法一致）：

- `BeginTx{txid}`
- `CommitTx{txid}`
- `CreateNode{external_id, label_id, internal_id}`
- `CreateEdge{src_iid, rel_type_id, dst_iid}`
- `TombstoneNode{node_iid}`
- `TombstoneEdge{src_iid, rel_type_id, dst_iid}`
- （可选）`SetNodeProp/SetEdgeProp`：写入 MemTable（不固化）

编码 envelope 沿用 M0：

`[len:u32][crc32:u32][type:u8][payload...]`

## 6. Read Path（Snapshot + Merge）

### 6.1 Snapshot 内容

M1 Snapshot 至少包含：

- `Arc<Vec<Arc<L0Run>>>`：从新到旧排序（最新优先）
- （预留）`Arc<Vec<Arc<CsrSegment>>>`：M2 才加入

### 6.2 Merge 规则

- 查询 `neighbors(src, rel?)`：
  - 从 `L0Run`（新到旧）枚举 `created_edges`，按 key 去重
  - 如果命中 `tombstoned_edges` 或 src/dst 在 `tombstoned_nodes`，跳过
  - M1 不依赖 CSR，因此只处理 L0Runs

> 注意：M1 不要求“全图扫描/复杂模式匹配”，只保证邻接扩展的最小能力。

## 7. Crash / Replay Strategy（M1）

启动流程：

1. open `.ndb`
2. replay `.wal`：
   - 仅重放 committed tx（沿用 M0）
   - 将每个 committed tx materialize 为一个 `L0Run`（或先合并到 MemTable，再 freeze）
3. 重建 E2I：
   - 从 `.ndb` 的 I2E 数组扫描

验收重点：

- “commit 前崩溃”：该 tx 的 node/edge 不可见
- “commit 后崩溃”：该 tx 的 node/edge 必须可见

## 8. Testing Strategy（必须覆盖）

- 单测：
  - I2E 记录 append + 扫描重建 E2I
  - tombstone key-based 覆盖（node/edge）
- 集成测：
  - `create_node/create_edge` commit -> reopen -> replay -> neighbors 正确
  - 未 commit 的 tx 写入 -> reopen -> 不可见
  - snapshot 一致性：
    - begin_read 拿到 snapshot
    - writer commit 新 L0Run
    - 老 snapshot 不应看到新边

## 9. Implementation Plan（按文件/模块）

在 `nervusdb-storage` 内新增：

- `idmap.rs`：I2E 持久化 + 启动重建 E2I
- `memtable.rs`：可变邻接表 + tombstone
- `snapshot.rs`：`Snapshot` / `L0Run` / merge iterator（neighbors）
- `engine.rs`：组合 `Pager + Wal + IdMap + MemTable + L0Runs`，实现 begin_read/begin_write/commit/replay
- `wal.rs`：扩展 record types（图语义）

## 10. Acceptance Criteria

- `cargo test -p nervusdb-storage` 全绿
- M1 的 3 个关键集成测全通过：
  - commit 可恢复
  - 未 commit 不可见
  - snapshot 隔离成立

