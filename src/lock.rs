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
use std::time::{Duration, Instant};

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
        Self::acquire_inner(path, LockMode::Exclusive, None)
    }

    /// 加**排他**锁，但在 `wait` 时间内**重试**而不是立即失败。
    ///
    /// 这是本项目版的 `busy_timeout`：两个进程（例如两个会话窗口）先后写同一个库时，
    /// 后来者不再立刻拿到错误，而是等先到者写完。默认不等待，见
    /// [`NervusDbOptions::lock_wait_ms`](crate::NervusDbOptions::lock_wait_ms)。
    ///
    /// **它不会让两个写者并存**——只是把「立即失败」变成「短暂等待后仍然失败」。
    /// 真正的并发写需要版本可见性或服务器模型，见 `docs/concurrency.md`。
    pub fn acquire_wait<P: AsRef<Path>>(path: P, wait: Duration) -> Result<Self, GraphError> {
        Self::acquire_inner(path, LockMode::Exclusive, Some(wait))
    }

    /// 加**共享**锁，在 `wait` 时间内重试（等待写者释放）。
    pub fn acquire_shared_wait<P: AsRef<Path>>(
        path: P,
        wait: Duration,
    ) -> Result<Self, GraphError> {
        Self::acquire_inner(path, LockMode::Shared, Some(wait))
    }

    /// 尝试加**共享**锁（只读句柄）。
    ///
    /// 多个共享锁可以共存；只要有一个共享锁存在，`acquire` 就会失败，
    /// 反之亦然。这把「多个读者 + 一个写者」的互斥交给内核，无需自旋或重试。
    ///
    /// 调用方仍需保证自己不写文件：共享锁不阻止写入，它阻止的是**别的进程**
    /// 同时写。请配合 `StorageEngine::pending_replay_pages() == 0` 一起使用。
    pub fn acquire_shared<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        Self::acquire_inner(path, LockMode::Shared, None)
    }

    fn acquire_inner<P: AsRef<Path>>(
        path: P,
        mode: LockMode,
        wait: Option<Duration>,
    ) -> Result<Self, GraphError> {
        let path_ref = path.as_ref();
        let path = path_ref.to_path_buf();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // 打开模式取决于锁的**意图**，而不是「反正只锁文件描述符」。
        //
        // ## 只读句柄必须只用读权限打开
        //
        // 这里曾经一律用 `read(true).write(true).create(true)`，理由是 Windows 上
        // 只以 append 模式打开的文件无法加锁。那条理由只适用于**排他**锁；对共享锁
        // （只读句柄）而言，它带来一个真实后果：
        //
        // 一个 0444 的库文件上，`NervusDb::open_read_only` 在**加锁**这一步就报
        // `Permission denied` —— 一个只想读的句柄，因为锁的打开模式而失败。只读库是
        // 常见部署形态（容器里只挂载数据盘、卷被标成 read-only），因此这条路径
        // 必须能独立工作。
        //
        // 原注释写着「是否真的写入由上层逻辑保证（只读句柄不调用写路径）」——又是
        // 一句**约定**而非代码强制。改为按模式选择权限后，即使上层出错，内核也会
        // 拒绝写入（返回 `Err`，而不是静默丢弃）。
        //
        // `create(true)` 同样只在排他模式下需要：只读打开**不应创建**文件。文件
        // 不存在时 `open` 直接失败，这正是只读打开应有的行为。
        let file = match mode {
            LockMode::Exclusive => OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)?,
            // Windows 的顾虑见上：共享锁在这里只用读权限，必要时由调用方保证文件
            // 已存在（`NervusDb::open` 在加锁前的 `check_format_version` 已确认）。
            LockMode::Shared => OpenOptions::new().read(true).open(&path)?,
        };

        // 重试的**退避**：先让出 CPU，再逐渐拉长间隔。
        //
        // 不用固定 1ms 忙等：那在 1 秒预算里会打 1000 次 `try_lock`，都是系统调用，
        // 而竞争者可能只是要写完几个字节。也不用纯 `sleep`：第一次就睡 10ms 会让
        // 「竞争者刚好要释放」的常见情形白等 10ms。折中：起始 200µs，每次 ×2，
        // 上限 20ms —— 覆盖 1ms 到数秒的等待预算，且总系统调用数是个位数到几十。
        let deadline = wait.map(|w| Instant::now() + w);
        let mut backoff = Duration::from_micros(200);

        loop {
            let result = match mode {
                LockMode::Exclusive => file.try_lock(),
                LockMode::Shared => file.try_lock_shared(),
            };

            match result {
                Ok(()) => return Ok(Self { file, path, mode }),
                Err(TryLockError::WouldBlock) => {
                    // 没给等待预算，或已经等够了 —— 立即失败，行为与过去完全一致。
                    let Some(deadline) = deadline else {
                        return Err(Self::locked_error(&path, mode));
                    };
                    if Instant::now() >= deadline {
                        return Err(Self::locked_error(&path, mode));
                    }
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(Duration::from_millis(20));
                }
                Err(TryLockError::Error(e)) => {
                    return Err(GraphError::StorageError(format!(
                        "Failed to lock database file '{}': {}",
                        path.display(),
                        e
                    )))
                }
            }
        }
    }

    fn locked_error(path: &Path, mode: LockMode) -> GraphError {
        GraphError::DatabaseLocked(format!(
            "Database '{}' is already open {} this handle. \
             A NervusDb database allows one writer and any number of readers; \
             a write handle excludes readers and vice versa. \
             Raise `lock_wait_ms` in NervusDbOptions to wait for the other handle \
             instead of failing immediately.",
            path.display(),
            match mode {
                // 拿不到排他锁：可能是别的写者，也可能有读者
                LockMode::Exclusive => "by another process or handle, or by a reader,",
                // 拿不到共享锁：一定有写者
                LockMode::Shared => "for writing by another process or handle,",
            }
        ))
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
