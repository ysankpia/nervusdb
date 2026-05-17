# T204 Implementation Plan: BlobStore Orphan Page Reclamation (VACUUM)

## 1. Overview

当前 `BlobStore::write_direct` 会直接向 `Pager` 分配页并写入数据。如果在“写入 blob”与“将 blob_id 写入可达结构（B-Tree / manifest）”之间发生崩溃，这些页会变成 **已分配但不可达（orphaned）** 的空间泄漏。随着长期写入/更新，这会把 `.ndb` 膨胀成垃圾场。

本任务的目标是：提供一种**可验证、可回滚、不会破坏文件格式兼容性**的回收机制。

## 2. Requirements Analysis

### 2.1 Use Scenarios

1. 长期运行的嵌入式应用持续更新属性/向量，文件变大但有效数据不多。
2. 崩溃场景中产生 orphan blob 页，恢复后数据库可用但磁盘空间持续泄漏。
3. 用户希望在维护窗口做一次“瘦身”（VACUUM）。

### 2.2 Functional Requirements

- [ ] 提供离线/维护模式的 VACUUM（重建可达页集合，清理不可达页）。
- [ ] VACUUM 完成后数据库可正常打开并通过现有测试。
- [ ] VACUUM 失败时不破坏原 DB（可回滚/原子替换）。

### 2.3 Performance Goals

- VACUUM 是离线操作，吞吐优先于延迟；允许 O(total_pages) 扫描。

## 3. Design

### 3.1 核心思路

采用“**mark-sweep**”的离线回收：

1. **Mark**：扫描所有“根”结构，收集所有可达的 `blob_id` 与其占用页范围（以及索引页/段页等）。
2. **Sweep**：重建 `.ndb` 的 bitmap（或生成新文件并搬迁可达页），释放不可达页。

### 3.2 根集合（Roots）

在 v2 当前实现里，blob 主要来自：

- 属性 store（`properties_root` B-Tree，payload=blob_id）
- 统计（`stats_root` blob_id）
- HNSW 向量/图（HNSW 的 B-Tree payload=blob_id）
- 其他 future root（需要列清单，避免漏标）

根集合信息来源：

- WAL 里最新 `ManifestSwitch`/`Checkpoint` 记录（epoch + roots）
- 或引擎启动时内存里的 `properties_root/stats_root`（作为运行期权威）

### 3.3 原子性与回滚

MVP 方案：生成新文件并原子替换：

1. 复制 `.ndb` 的可达页到 `db.ndb.vacuum.tmp`
2. 生成新的 meta/bitmap
3. `rename()` 替换原文件（失败则保留原文件）

不在本任务里做“在线 vacuum”。

## 4. Implementation Plan

### Step 1: 建立可达页扫描器（Risk: High）

- File: `nervusdb-storage/src/blob_store.rs`、`nervusdb-storage/src/index/btree.rs`、`nervusdb-storage/src/wal.rs`
- 产出：给定 roots，枚举所有可达 blob_id 与相关页

### Step 2: 生成 vacuum 输出文件（Risk: High）

- File: `nervusdb-storage/src/pager.rs`（新增辅助 API：按 page_id 读写 raw）
- 产出：`vacuum_to(target_path)`

### Step 3: CLI/工具入口（Risk: Medium）

- File: `nervusdb-cli` 新增子命令（例如 `v2 vacuum`）
- 产出：可在维护窗口执行

## 5. Technical Key Points

- 必须保证“根集合不漏”：漏了就是数据丢失（致命）。
- 必须保证替换原子：失败不能破坏原 DB。
- 需要明确定义 blob_id 到页范围的映射（当前 `BlobStore` 需要暴露元信息）。

## 6. Verification Plan

- 单测：构造 orphan blob（模拟：写 blob 但不插入索引），VACUUM 后文件大小下降且数据可读。
- 集成：跑 `cargo test` 全量。
- 破坏性测试：VACUUM 过程中 crash（可用 kill/中断），确保原 DB 未损坏。

## 7. Risk Assessment

| Risk Description                     | Impact Level | Mitigation Measures                                  |
| ------------------------------------ | ------------ | ---------------------------------------------------- |
| roots 漏标导致数据丢失               | High         | 白名单 + 单测覆盖 + 与 manifest/checkpoint 严格对齐 |
| vacuum 替换失败破坏原 DB             | High         | tmp 文件 + 原子 rename + 失败不覆盖                  |
| 页搬迁导致引用失效（blob_id 语义变） | High         | 只搬迁页内容，不改变 blob_id 语义；或重写引用        |

## 8. Out of Scope

- 在线 vacuum
- 细粒度增量 GC
- 多版本/快照 GC（需要更完整 MVCC 设计）

