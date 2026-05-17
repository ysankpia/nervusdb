# T205 Implementation Plan: Pager Lock Granularity (Reduce Global Mutex Contention)

## 1. Overview

当前 `Pager` 以 `Arc<Mutex<Pager>>` 形式被广泛持有。读路径一旦需要触发页读取/解码（尤其是 cache miss 场景），会把整个引擎锁住，形成并发瓶颈。

本任务的目标：在不破坏 crash 一致性与现有 API 的前提下，降低锁争用。

## 2. Requirements Analysis

### 2.1 Use Scenarios

1. 多个 Snapshot Reader 并发执行查询（尤其包含 properties/index/hnsw）。
2. 写事务频繁提交导致读路径也频繁触碰 pager（索引/属性读取）。

### 2.2 Functional Requirements

- [ ] 读路径尽量不阻塞写路径（在保证一致性前提下）。
- [ ] 不引入外部 daemon；不依赖 mmap 特性才能运行。
- [ ] 不改变磁盘格式（或必须版本化并显式升级）。

## 3. Design

### 3.1 方案候选

1. **RwLock Pager（最小变更）**  
   - 读：`RwLock::read()`，写：`RwLock::write()`  
   - 问题：`Pager` 读写通常共用同一个 `File` + `Seek`，读锁下也可能需要 seek；需要重构 IO 访问方式。

2. **无锁读 + 独立 File handle（更务实）**  
   - 为读路径创建独立的 `File`（只读）句柄，避免 seek 冲突  
   - 写路径仍持有可写句柄  
   - 读路径靠 OS page cache；必要时在用户态加只读 cache

3. **页级锁（高复杂度，不建议 MVP）**  
   - 需要 lock table + deadlock 处理 + page lifecycle 管理

### 3.2 推荐路线（两阶段）

- Phase A：拆出只读 `PagerReader`（独立文件句柄），让 Snapshot 读路径不持有全局 mutex。  
- Phase B：基于真实 profile 数据再决定是否需要更细粒度锁或用户态 cache。

## 4. Implementation Plan

### Step 1: 抽象只读读取接口（Risk: High）

- File: `nervusdb-storage/src/pager.rs`
- 产出：`PagerReader`（只读 open + read_page_raw），并确保不改变现有写路径语义

### Step 2: Snapshot / Index / BlobStore 改用 Reader（Risk: High）

- File: `snapshot.rs`, `api.rs`, `blob_store.rs`, `index/*`
- 产出：读路径不再抢占写锁

### Step 3: 压测与回归（Risk: Medium）

- 基准：读并发 QPS、P99 延迟、写提交抖动

## 5. Verification Plan

- `cargo test` 全量
- 并发压力测试（新增 bench/脚本）
- crash-gate（确保一致性不退化）

## 6. Risk Assessment

| Risk Description                         | Impact Level | Mitigation Measures                             |
| ---------------------------------------- | ------------ | ----------------------------------------------- |
| 读写句柄分离导致可见性/一致性问题         | High         | 明确 snapshot 边界 + 只读句柄不做 write/seek     |
| 重构 pager 影响面过大导致回归            | High         | 分阶段迁移 + 小步 PR + 保留旧路径作为 fallback   |
| 复杂度膨胀（页级锁等过度工程）            | High         | 先做独立 reader + profile 驱动，拒绝一步到位     |

