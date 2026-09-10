//! 数据库文件的进程级排他锁。
//!
//! 依据：`std::fs::File::try_lock` 自 Rust 1.89 起稳定，Unix 对应 `flock`、
//! Windows 对应 `LockFileEx`，因此**无需引入任何第三方依赖**，符合本项目
//! 「零依赖嵌入式」的定位。
//!
//! 锁直接加在**主数据文件**上，因此不产生 `.lock` 之类的边车文件，
//! 继续满足 AGENTS.md 的「严格两文件（`{path}` + `{path}.wal`）」不变量。
//!
//! 语义：一个数据库文件在任一时刻只允许**一个**打开的句柄，跨进程与同进程
//! 一律互斥（`flock` 绑定到打开的文件描述符，故同一进程内两次 `open` 同样冲突）。
//! 这消除了「两个写入者各自以为成功、后者静默覆盖前者」的数据损坏路径。

use crate::graph::GraphError;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// 持有数据库文件排他锁的守卫。`Drop` 时随文件描述符关闭自动释放。
pub struct DbLock {
    /// 保持文件句柄存活即持有锁；字段本身不直接读取，故允许未使用。
    #[allow(dead_code)]
    file: File,
    path: PathBuf,
}

impl DbLock {
    /// 尝试对主数据文件加排他锁。
    ///
    /// - 文件不存在时先创建（与 `StorageEngine::open` 的行为一致）；
    /// - 锁被其它进程或本进程的另一个句柄持有时返回 `GraphError::DatabaseLocked`；
    /// - `:memory:` 模式无文件，调用方应跳过本函数。
    pub fn acquire<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        let path_ref = path.as_ref();
        let path = path_ref.to_path_buf();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // 锁必须能读能写：Windows 上仅以 append 模式打开的文件无法加锁。
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        match file.try_lock() {
            Ok(()) => Ok(Self { file, path }),
            Err(TryLockError::WouldBlock) => Err(GraphError::DatabaseLocked(format!(
                "Database '{}' is already open by another process or handle. \
                 A GraphLite database allows only one handle at a time; \
                 close the other handle before opening it again.",
                path.display()
            ))),
            Err(TryLockError::Error(e)) => Err(GraphError::StorageError(format!(
                "Failed to lock database file '{}': {}",
                path.display(),
                e
            ))),
        }
    }

    /// 数据库主文件路径
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DbLock {
    fn drop(&mut self) {
        // 显式解锁更清晰；即便失败也无妨——文件描述符关闭时锁必然释放。
        let _ = self.file.unlock();
    }
}
