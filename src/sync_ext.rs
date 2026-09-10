//! 锁中毒恢复辅助特质。
//!
//! 背景：本引擎的锁保护的都是**可重建的派生状态或纯数据结构**
//! （缓冲池帧表、分配器元数据、图句柄包装），并不承载「必须整体拒绝服务」
//! 的业务不变量。因此当某个持有者在持锁期间 panic 导致锁中毒时，
//! 继续取出内部数据比直接 panic 更合适——后者会把一次局部失败放大成
//! 调用方不可恢复的进程终止，对嵌入式使用场景尤其有害。
//!
//! 用法：把 `.lock().unwrap()` 替换为 `.lock_recover()`，
//! `.read().expect("Lock poisoned")` → `.read_recover()`，
//! `.write().expect(...)` → `.write_recover()`。
//! 一次消除 panic 路径，且**不改变任何函数签名**。

use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 互斥锁的中毒恢复访问
pub trait MutexRecoverExt<T> {
    /// 获取互斥锁；若锁已中毒则恢复内部数据而非 panic
    fn lock_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexRecoverExt<T> for Mutex<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// 读写锁的中毒恢复访问
pub trait RwLockRecoverExt<T> {
    /// 获取读守卫；若锁已中毒则恢复内部数据而非 panic
    fn read_recover(&self) -> RwLockReadGuard<'_, T>;

    /// 获取写守卫；若锁已中毒则恢复内部数据而非 panic
    fn write_recover(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockRecoverExt<T> for RwLock<T> {
    fn read_recover(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write_recover(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(PoisonError::into_inner)
    }
}
