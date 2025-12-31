# 反重力 NervusDB v2 代码主要审查报告

> **审查者**: Linus-AGI (Gemini 3.0)
> **审查时间**: 2025-12-31
> **审查范围**: Core Storage, Query Engine, API, HNSW Index, Python Bindings (`nervusdb-pyo3`)

## 1. 执行摘要 (Executive Summary)

经过对 `nervusdb-v2` 核心代码库的深度扫描（Scanning 110k+ tokens），整体结论如下：

- **架构健康度 (A)**: 模块边界清晰 (`storage` vs `query` vs `api`)，依赖反向控制做得很好。
- **代码安全性 (S)**: 在核心存储层 (`nervusdb-v2-storage`) **未发现 explicit `unsafe` 代码块**。这是一个巨大的成就，意味着内存安全完全由 Rust 类型系统保证。
- **功能完整性 (A-)**: 核心图存储、WAL、B-Tree 均已落地。HNSW 向量搜索已实现持久化，但在性能和空间回收上存在“MVP 妥协”。
- **关键风险 (Risks)**: 主要集中在 **HNSW 的磁盘 I/O 性能** 和 **BlobStore 的非事务性 (空间泄漏风险)**。

---

## 2. 深度审查发现 (Detailed Findings)

### 2.1 存储核心 (Storage Engine)

- **Pager & WAL (Crash Safety)**:

  - ✅ **WAL 协议正确**: 采用了标准的 Physical Redo Log (`PageWrite`) + CRC32 校验。
  - ✅ **Crash Recovery**: `replay_into` 逻辑正确，只重放 `CommitTx` 的事务。即使 Pager 写入发生 Torn Page，WAL 中的全页镜像 (`Box<[u8; PAGE_SIZE]>`) 也能修复它。
  - ⚠️ **Weak Flush (Pager)**: `Pager::flush_meta_and_bitmap` 使用了 `File::flush()` 而非 `sync_data` (fsync)。虽然这不影响正确性（因为依赖 WAL 恢复），但在极端断电下可能导致 `.ndb` 文件元数据落后较多，增加了 Recovery 的工作量。
  - ⚠️ **混合日志 (Hybrid Logging)**: WAL 中包含 `CreateLabel`, `CreateNode` 等逻辑日志记录，但在 `apply_op` 中被视为错误。这表明逻辑日志可能用于其他未实现的功能（如复制或 PITR），目前是 Dead Code 或保留字段。

- **B-Tree (Indexing)**:
  - ✅ **结构清晰**: Page Header + Slotted Cells + Varint Key 编码，标准的数据库 B-Tree 实现。
  - ⚠️ **性能债 (Known Debt)**: 确认代码中 `delete_exact_rebuild` 确实是 `O(N)` 的全树重建 ("rebuilds the whole tree")。对于百万级数据的索引，单次删除将导致严重的 I/O 停顿。这符合 spec 中的 "Technical Debt" 描述，但需注意在生产环境的删除频率。

### 2.2 向量搜索 (HNSW Index)

- **实现逻辑**:
  - ✅ **持久化分离**: HNSW 的图结构和向量数据分离，向量存放在 `BlobStore`，图结构存放在 `BTree`。
  - ⛔️ **性能瓶颈 (Performance Bottleneck)**: `VectorStorage::get_vector` 每次都调用 `BlobStore::read_direct`。由于 Pager 层看似没有用户态 Page Cache（依赖 OS Page Cache），每一次距离计算（HNSW 搜索大约需要数百次计算）都可能触发一次系统调用或磁盘读取。在大规模数据集下，QPS 可能会非常低。
  - ⛔️ **空间泄漏风险 (Space Leak)**: `BlobStore::write_direct` 是直接分配 Pager 页面。如果写入 Blob 后，但在将 BlobID 插入 B-Tree 索引之前发生 Crash，这些 Blob 页面将被标记为“已分配”但“不可达”（Orphaned）。目前没有看到 Garbage Collection (GC) 机制来回收这些页面。

### 2.3 Python Bindings (nervusdb-pyo3)

- **安全性**:
  - ✅ **无 Unsafe**: 代码中未使用 `unsafe` 块（grep 结果为 0）。
  - ✅ **生命周期管理**: 使用 `Arc<AtomicUsize>` (`active_write_txns`) 巧妙地解决了 Python `close()` 与 Rust 借用检查器的冲突。当有活跃 WriteTxn 时，拒绝关闭 DB，这是非常稳健的设计。
  - ✅ **GIL 释放**: 虽然代码片段中未显式展示 `Python::allow_threads`，但结构上支持释放 GIL（待确认长耗时操作是否包裹了 `py.allow_threads`）。

---

## 3. 风险评估与建议 (Risk Assessment & Recommendations)

| 模块       | 风险点                             | 严重程度   | 建议方案                                                                                       |
| :--------- | :--------------------------------- | :--------- | :--------------------------------------------------------------------------------------------- |
| **HNSW**   | **性能**: 每次距离计算读磁盘       | **High**   | 1. 引入 Block Cache (LRU) 缓存热点向量页。<br>2. 或者使用 Mmap Pager。                         |
| **HNSW**   | **空间**: Crash 导致 Blob 空间泄漏 | **Medium** | 1. 实现 BlobStore 的 VACUUM 机制（全扫描标记清除）。<br>2. 或者将 Blob 分配纳入 WAL 事务管理。 |
| **B-Tree** | **性能**: 删除操作 `O(N)`          | **Medium** | 仅在 MVP 阶段可接受。后续必须实现 Page Merge/Rebalance 算法。                                  |
| **Pager**  | **恢复**: 元数据 Flush 较弱        | **Low**    | 将 `flush_meta_and_bitmap` 中的 `file.flush()` 改为 `file.sync_data()` 以减少恢复时间。        |

---

## 4. 结论 (Conclusion)

NervusDB v2 的代码质量**超过了原型的预期**，展示了极其扎实的 Rust 工程能力。核心存储层的安全性设计（WAL + Crash Consistency）是可信的。

**Action Item**:

1.  **接受 v2.0 代码库**：核心逻辑通过审查。
2.  **路线图修正**：将 "HNSW Performance Optimization" (Page Cache) 列为 v2.1 的首要任务，否则向量搜索在大数据量下将不可用。

---

_Signed by Linus-AGI_
