# T40: NervusDB v2 Kernel Spec（Property Graph + LSM Segments）

## 1. Context

`nervusdb-core v1.0.3` 的核心是 `redb` + RDF triple（三表 `SPO/POS/OSP`）。它把 “KV on Graph” 做到了当前数据结构的上限，但图遍历与属性过滤的物理瓶颈无法突破。

v2 目标是“真正嵌入式 Property Graph DB”：自研 Pager + WAL，面向遍历/过滤的数据布局（CSR + Columnar），并以 LSM 思路解决写入与 compaction。

**兼容性**：v2 不兼容 v1（新 crate / 新 API / 新磁盘格式）。

## 2. Goals（MVP）

- 单写多读：`Single Writer + Snapshot Readers`（快照读，读无锁）
- `.ndb + .wal` 两文件：`.ndb` 为 page store，`.wal` 为 redo log（WAL 即 delta 的持久化形式）
- 先跑通 end-to-end：创建节点/边、邻居遍历、崩溃恢复
- LSM 形态图存储：
  - L0：可变 MemTable（内存 delta）
  - L0 frozen runs：commit 时冻结（不可变）
  - L1+：磁盘不可变 CSR segment（多段，非全局完美 CSR）
- MVP 删除：仅 tombstone 逻辑删除，物理回收推迟到 compaction

## 3. Non-Goals（MVP 不做）

- 多写者并发 / Serializable / SSI
- 在线物理删除/在线 CSR 重写
- 在线 schema 管理（属性在 M1 只存在于 WAL/MemTable）
- WASM 持久化（WASM 仅提供 in-memory 实现，不共享磁盘格式）
- 向量索引/全文索引/二级属性索引（后置 feature gate）

## 4. Core Decisions（宪法级约束）

### 4.1 事务与隔离

- 全局同一时间最多一个写事务（互斥锁）。
- 读事务并发，读取“开始时刻”的快照（snapshot）。
- commit 边界是快照边界：读事务只看见它开始时已经冻结/发布的段集合。

### 4.2 文件布局

- `.ndb`：page store（mmap 读为主，写通过 pager）
- `.wal`：append-only redo log（默认每次 commit fsync，可配置 durability）

### 4.3 ID 模型

- ExternalID：用户可见（`u64`；后续可扩展 string）
- InternalID：系统内部 dense（`u32`，范围 `0..N`）
- 持久化策略（M1）：
  - `I2E`：append-only array（pages）
  - 启动时重建内存 `HashMap<E2I>`（允许 O(N) 启动成本）
  - B+Tree / Hash index（E2I on-disk）推迟到 M2+

### 4.4 Label/Schema

- MVP：每个节点只有一个主 label。
- MVP：属性只在 WAL/MemTable（delta）中存在；compaction 时固化到属性页（columnar pages）。

## 5. Data Model（Property Graph）

### 5.1 Dictionaries

- `LabelId: u32`
- `RelTypeId: u32`
- `StringId: u32`（interning；实现可后置到 M1/M2）

### 5.2 Node / Edge Keys（LSM Sort Key）

LSM 的所有不可变段都必须定义排序键（否则 merge/compaction 无从谈起）。

- Edge sort key：`(src_internal_id, rel_type_id, dst_internal_id)`
- Tombstone 必须基于 key：`TombstoneEdge{src, rel, dst}` / `TombstoneNode{iid}`

## 6. Snapshot Model（关键：读无锁、无过滤）

不要用“VisibleWALOffset”在读路径过滤。读应该只是“持有一组不可变段的引用”。

### 6.1 写事务 commit 的段发布

- 写事务把当前 MemTable 冻结为不可变 `L0Run`（`Arc` 持有）
- 立即切换到一个新的空 MemTable
- 把 `L0Run` 追加到全局段列表（原子发布）

### 6.2 读事务 begin 的快照获取

`Snapshot` 至少包含：

- `Vec<Arc<L0Run>>`：当时已发布的 frozen runs
- `Vec<Arc<CsrSegment>>`：当时可见的磁盘段（L1+）

读路径只在这两类不可变集合上做 merge iterator，不依赖全局锁。

## 7. WAL Event Semantics（Property Graph 语义）

WAL 记录的是“图语义事件”，不是 triple。

建议的记录类型（M0/M1）：

- `BeginTx{txid}`
- `CommitTx{txid}`
- `CreateNode{external_id, label_id, internal_id}`
- `CreateEdge{src_iid, rel_type_id, dst_iid}`
- `SetNodeProp{node_iid, key_id, value}`
- `SetEdgeProp{src_iid, rel_type_id, dst_iid, key_id, value}`
- `TombstoneNode{node_iid}`
- `TombstoneEdge{src_iid, rel_type_id, dst_iid}`

编码格式（建议，便于演进）：

- `[len:u32][crc32:u32][type:u8][payload...]`

## 8. Storage Layout（M0/M1 先只定义“必须的最小集合”）

### 8.1 `.ndb` Page Types（M0）

- Meta page (0)：magic/version/page_size/roots/epoch 等
- Bitmap/Freelist pages：页分配
- Data pages：用于 I2E、未来 CSR/columnar/blob

### 8.2 CSR Segment（M2+）

CSR segment 是不可变的、mmap 友好的段结构；允许多个段并存。

最小内容：

- segment meta（统计、范围、checksum）
- offsets（按 `src_iid` 或按块分段）
- edges data（按 edge sort key 排序）

## 9. Durability Levels（API 必须提供）

- `Full`（默认）：每次 commit `fsync` WAL
- `GroupCommit{...}`：可配置（批量刷盘）
- `None`：不 fsync（明确告知风险）

## 10. WASM Boundary

- `wasm32`：只提供 in-memory graph store（API 兼容，磁盘格式不兼容）。
- Native：提供 Pager/WAL/LSM 段。

## 11. Milestones（去风险版）

### M0: Pager + WAL Replay（Week 1-2）

- `.ndb`：open/create、read/write page、allocate/free page
- `.wal`：append、crc、replay（重放到内存结构/或重建必要元数据）
- 测试：崩溃恢复（模拟：写入随机记录 -> 进程异常退出 -> 重启 replay -> 校验）

### M1: Log-Structured Graph（Week 3-6）

- IDMap（I2E 持久化 + 启动重建 E2I）
- MemTable：邻接表（支持 CreateNode/CreateEdge + tombstone）
- Snapshot：commit 冻结 L0Run，读拿 Snapshot 做 merge
- 测试：API 级 CRUD（append-only + tombstone），并验证 snapshot 读一致性

### M2: CSR Flush + Compaction（Week 7-10）

- `db.compact()` 显式 compaction（后台线程可选 feature）
- Flush：MemTable/L0Run -> CSR segment（L1）
- Merge iterator：L0Runs + CSR segments（key-based tombstone 过滤）
- 验收：遍历性能显著提升（数量级）

## 12. Risks（必须正面写清）

- Tombstone 扩张：若不 compaction，会导致读放大；必须提供显式 `compact()`
- mmap + durability：必须严格定义 WAL fsync 与 metadata 更新顺序
- IDMap O(N) 启动：M1 可接受，M2+ 需要 on-disk E2I 索引

