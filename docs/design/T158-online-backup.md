# T158 Online Backup API 设计文档

## 1. 概述

### 1.1 问题陈述
NervusDB v2 目前支持 checkpoint-on-close，但缺少在线热备份能力。用户无法在数据库运行时创建一致的备份快照。

### 1.2 目标
实现 Online Backup API，支持：
- **热备份**：数据库运行时不阻塞读写操作
- **一致性保证**：备份是时间点一致（PITR）的
- **可恢复性**：可以从备份恢复数据库
- **增量备份**（可选）：仅备份变更数据

### 1.3 使用场景
1. **定期备份**：Cron job 定时备份
2. **关键操作前**：重大迁移前创建备份
3. **灾难恢复**：从备份恢复数据

## 2. 设计方案

### 2.1 核心原理

```
传统备份问题：
┌─────────────┐     复制中      ┌─────────────┐
│   Writer   │ ───────────> │  备份文件   │ ❌ 不一致
└─────────────┘              └─────────────┘

在线备份解决方案（Copy-on-File-Set）：
┌─────────────┐     1. 获取快照点  ┌─────────────┐
│   Writer   │ ───────────────> │  Snapshot  │
└─────────────┘                 └─────────────┘
                                     │
                         2. 复制文件（后台）
                                     │
                         3. 完成备份
                                     ↓
                          ┌─────────────┐
                          │  备份文件   │ ✅ 一致
                          └─────────────┘
```

### 2.2 架构设计

#### 2.2.1 BackupManager

```rust
pub struct BackupManager {
    db_path: PathBuf,
    backup_path: PathBuf,
    wal_path: PathBuf,
    // 用于跟踪备份状态
    active_backup: RwLock<Option<ActiveBackup>>,
}

struct ActiveBackup {
    id: Uuid,
    snapshot_txid: u64,
    snapshot_epoch: u64,
    created_at: DateTime<Utc>,
    status: BackupStatus,
}
```

#### 2.2.2 备份流程

```
1. begin_backup() ──┐
   - 记录当前 WAL 位置
   - 记录 manifest 状态
   - 创建备份元数据文件
   
2. copy_files() ────┤ (后台异步)
   - 复制 .ndb 文件
   - 复制 .wal (从 checkpoint 位置)
   - 记录复制进度
   
3. commit_backup() ──┤
   - 验证文件完整性
   - 更新备份清单
   - 清理临时文件
```

### 2.3 文件格式

#### 2.3.1 备份清单 (backup_manifest.json)

```json
{
  "backup_id": "550e8400-e29b-41d4-a716-446655440000",
  "created_at": "2025-12-30T12:00:00Z",
  "nervusdb_version": "2.0.0",
  "checkpoint": {
    "txid": 12345,
    "epoch": 5,
    "manifest_segments": [...],
    "properties_root": 123,
    "stats_root": 456
  },
  "files": [
    {
      "name": "test.ndb",
      "size": 8388608,
      "checksum": "sha256:abc123...",
      "copied": true
    },
    {
      "name": "test.wal",
      "size": 1048576,
      "start_offset": 8192,
      "checksum": "sha256:def456...",
      "copied": true
    }
  ],
  "status": "completed"
}
```

### 2.4 API 设计

#### 2.4.1 Db 层 API

```rust
impl Db {
    /// 开始在线备份
    pub fn begin_backup(&self, backup_dir: impl AsRef<Path>) -> Result<BackupHandle>;

    /// 查询备份状态
    pub fn backup_status(&self, handle: &BackupHandle) -> Result<BackupStatus>;

    /// 等待备份完成
    pub fn wait_for_backup(&self, handle: &BackupHandle) -> Result<()>;

    /// 取消备份
    pub fn cancel_backup(&self, handle: &BackupHandle) -> Result<()>;

    /// 列出所有备份
    pub fn list_backups(&self, backup_dir: impl AsRef<Path>) -> Result<Vec<BackupInfo>>;

    /// 从备份恢复
    pub fn restore_from_backup(&self, backup_dir: impl AsRef<Path>, backup_id: Uuid) -> Result<()>;
}
```

#### 2.4.2 BackupHandle

```rust
#[derive(Debug, Clone)]
pub struct BackupHandle {
    id: Uuid,
    backup_path: PathBuf,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum BackupStatus {
    InProgress {
        progress: f64,  // 0.0 - 1.0
        bytes_copied: u64,
        total_bytes: u64,
    },
    Completed {
        backup_info: BackupInfo,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub file_count: usize,
    pub nervusdb_version: String,
}
```

### 2.5 实现步骤

#### 步骤 1：创建 BackupManager 骨架（Low Risk）

**文件**：`nervusdb-v2-storage/src/backup.rs`

**任务**：
1. 定义 `BackupManager`、`BackupHandle`、`BackupStatus` 结构体
2. 实现基本的 `begin_backup()`、`list_backups()` 方法
3. 创建备份清单文件格式

**验证**：编译通过，基本 API 可用

#### 步骤 2：实现文件复制逻辑（Medium Risk）

**任务**：
1. 实现 `.ndb` 文件复制
2. 实现 `.wal` 文件复制（从 checkpoint 位置）
3. 实现校验和计算

**要点**：
- 使用后台线程异步复制大文件
- 支持进度查询
- 断点续传（可选）

**验证**：
- 备份文件可复制
- 校验和匹配

#### 步骤 3：实现一致性保证（High Risk）

**任务**：
1. 在 `begin_backup()` 时记录快照点
2. 确保 WAL replay 从正确位置开始
3. 实现 `commit_backup()` 原子提交

**要点**：
- 复用现有的 checkpoint 机制
- 确保备份是时间点一致的

**验证**：
- 从备份恢复后数据一致
- 崩溃恢复正常工作

#### 步骤 4：实现恢复功能（Medium Risk）

**任务**：
1. 实现 `restore_from_backup()`
2. 验证备份完整性
3. 支持恢复到不同路径

**验证**：
- 恢复的数据库可正常打开
- 数据完整无丢失

### 2.6 增量备份设计（可选 v2）

```
backup_1/              backup_2/
├── backup_manifest.json   ├── backup_manifest.json
├── test.ndb             ├── test.ndb (full copy)
├── test.wal             ├── test.wal (full copy)
└── backup_manifest.json ←── 引用 parent
                        └── test.wal.diff (增量)
```

## 3. 性能考虑

1. **后台复制**：不阻塞主读写线程
2. **流式复制**：大文件分块复制，避免内存峰值
3. **压缩**（可选）：备份时压缩文件

## 4. 风险评估

| 风险 | 影响 | 缓解措施 |
| ---- | ---- | --------- |
| 备份期间写入丢失 | 高 | 记录 WAL 位置，确保 replay 完整 |
| 磁盘空间不足 | 中 | 备份前检查空间 |
| 备份损坏 | 高 | 校验和验证 + 完整性检查 |
| 并发备份冲突 | 低 | 一次只允许一个备份 |

## 5. 测试计划

### 单元测试
- `test_backup_creation` - 创建备份
- `test_backup_list` - 列出备份
- `test_backup_progress` - 进度查询

### 集成测试
- `test_backup_during_write` - 写入时备份
- `test_restore_from_backup` - 从备份恢复
- `test_backup_crash_recovery` - 崩溃恢复

### 边界测试
- 空数据库备份
- 大文件备份
- 磁盘空间不足

## 6. 后续扩展

1. **增量备份**：仅备份变更数据
2. **远程备份**：S3/Azure Blob Storage 支持
3. **加密备份**：备份加密
4. **压缩备份**：gzip/zstd 压缩

## 7. 相关任务依赖

- T106 Checkpoint-on-Close（已完成）：提供基础
- T103 Compaction Integration（已完成）：确保属性持久化
