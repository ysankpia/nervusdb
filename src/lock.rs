//! 数据库文件的进程级锁。
//!
//! 依据：`std::fs::File::try_lock` / `try_lock_shared` 自 Rust 1.89 起稳定，
//! Unix 对应 `flock`、Windows 对应 `LockFileEx`，因此**无需引入任何第三方依赖**，
//! 符合本项目「零依赖嵌入式」的定位。
//!
//! 锁直接加在**主数据文件**上，因此不产生 `.lock` 之类的边车文件，
//! 继续满足 AGENTS.md 的「严格两文件（`{path}` + `{path}.wal`）」不变量。
//!
//! ## 两种模式
//!
//! - **排他（读写句柄）**：跨进程与同进程一律互斥。这消除了「两个写入者各自
//!   以为成功、后者静默覆盖前者」的数据损坏路径。
//! - **共享（只读句柄）**：多个读者可以共存（[`DbLock::acquire_shared`]），
//!   但与任何写者互斥。SQLite 的 WAL 模式即此模型。
//!
//! 只读共享**不是**仅仅换一个锁调用：读者不能触发 WAL 回放（那会写主数据
//! 文件），因此调用方必须先确认没有待回放内容——见
//! `StorageEngine::pending_replay_pages`。

use crate::graph::GraphError;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// 锁的持有模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// 排他：可读可写，与任何其它句柄互斥
    Exclusive,
    /// 共享：仅可读，可与其它读者共存，但与写者互斥
    Shared,
}

/// 持有数据库文件锁的守卫。`Drop` 时随文件描述符关闭自动释放。
pub struct DbLock {
    /// 保持文件句柄存活即持有锁；字段本身不直接读取，故允许未使用。
    #[allow(dead_code)]
    file: File,
    path: PathBuf,
    mode: LockMode,
}

impl DbLock {
    /// 尝试对主数据文件加**排他**锁（读写句柄）。
    ///
    /// - 文件不存在时先创建（与 `StorageEngine::open` 的行为一致）；
    /// - 锁被其它进程或本进程的另一个句柄持有时返回 `GraphError::DatabaseLocked`；
    /// - `:memory:` 模式无文件，调用方应跳过本函数。
    pub fn acquire<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        Self::acquire_inner(path, LockMode::Exclusive)
    }

    /// 尝试加**共享**锁（只读句柄）。
    ///
    /// 多个共享锁可以共存；只要有一个共享锁存在，`acquire` 就会失败，
    /// 反之亦然。这把「多个读者 + 一个写者」的互斥交给内核，无需自旋或重试。
    ///
    /// 调用方仍需保证自己不写文件：共享锁不阻止写入，它阻止的是**别的进程**
    /// 同时写。请配合 `StorageEngine::pending_replay_pages() == 0` 一起使用。
    pub fn acquire_shared<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        Self::acquire_inner(path, LockMode::Shared)
    }

    fn acquire_inner<P: AsRef<Path>>(path: P, mode: LockMode) -> Result<Self, GraphError> {
        let path_ref = path.as_ref();
        let path = path_ref.to_path_buf();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // 锁必须能读能写：Windows 上仅以 append 模式打开的文件无法加锁。
        // 只读模式也以可写方式打开**文件描述符**——共享锁限制的是别人的写权限，
        // 不是本句柄的。是否真的写入由上层逻辑保证（只读句柄不调用写路径）。
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let result = match mode {
            LockMode::Exclusive => file.try_lock(),
            LockMode::Shared => file.try_lock_shared(),
        };

        match result {
            Ok(()) => Ok(Self { file, path, mode }),
            Err(TryLockError::WouldBlock) => Err(GraphError::DatabaseLocked(format!(
                "Database '{}' is already open {} this handle. \
                 A NervusDb database allows one writer and any number of readers; \
                 a write handle excludes readers and vice versa.",
                path.display(),
                match mode {
                    // 拿不到排他锁：可能是别的写者，也可能有读者
                    LockMode::Exclusive => "by another process or handle, or by a reader,",
                    // 拿不到共享锁：一定有写者
                    LockMode::Shared => "for writing by another process or handle,",
                }
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

    /// 本句柄持有的锁模式
    pub fn mode(&self) -> LockMode {
        self.mode
    }
}

impl Drop for DbLock {
    fn drop(&mut self) {
        // 显式解锁更清晰；即便失败也无妨——文件描述符关闭时锁必然释放。
        let _ = self.file.unlock();
    }
}
