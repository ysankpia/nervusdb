//! 主数据文件的页级校验和存储。
//!
//! ## 为什么需要它
//!
//! WAL 的每一帧都带 CRC32，但**主数据文件的页没有**。这意味着磁盘位翻转、
//! 半写页、误操作造成的损坏不会在读取时被发现——`get_node` 只会返回「不存在」，
//! 静默地给出错误的图。本模块为每一张数据页维护 CRC32，在页从主文件读入时校验。
//!
//! ## 为什么绕开 BufferPoolManager
//!
//! 若 CRC 目录页也经由缓冲池读写，就会形成这条无界递归：
//!
//! ```text
//! fetch_page() → acquire_frame() → evict_frame() → 写页并更新 CRC
//!              → 需要 CRC 目录页 → fetch_page() → acquire_frame() → …
//! ```
//!
//! 因此 `CrcStore` 直接经 [`DiskManager`] 读写目录页，不进入缓冲池，依赖方向是
//! `BufferPoolManager → CrcStore → DiskManager`，无环。代价是需要自己的有界缓存
//! （[`CRC_CACHE_CAPACITY`] 页 = 256KB），它只存目录元数据，不承载图拓扑。
//!
//! ## 布局
//!
//! - 页 `1..=255`：CRC32 内联在 Page 0（`INLINE_CRC_OFFSET` 起，每页 4 字节）。
//!   这样极小库无需分配任何目录页，守住「最小库 ≤16KB（4 页）」的约束。
//! - 页 `>=256`：两级 radix 目录。L1 页存指针，L2 页存 CRC32。
//!   单条 L1 条目覆盖 [`CrcDirPage::ENTRIES_PER_PAGE`]² 页 ≈ 4.06 GiB。
//!
//! ## `0` 是「未记录」而不是「不匹配」
//!
//! 新分配、刚从 freelist 复用、或崩溃后尚未确认的页，其 CRC 条目为 `0`，读取时
//! **跳过校验**。原则是「宁可漏检，绝不假报」：把「未知」当成「损坏」会让一次正常
//! 的崩溃恢复之后，服务器拒绝启动。

use crate::buffer::DiskManager;
use crate::graph::GraphError;
use crate::page::{CrcDirPage, HeaderPage, PageId, INVALID_PAGE_ID, PAGE_SIZE};
use std::collections::HashMap;
use std::sync::Arc;

/// CRC 目录页的内存缓存容量（页）。256KB，只缓存目录元数据。
pub const CRC_CACHE_CAPACITY: usize = 64;

/// 单张目录页缓存的条目
struct CachedDirPage {
    data: [u8; PAGE_SIZE],
    dirty: bool,
    /// 简易 LRU 序号，越大越新
    used_at: u64,
}

/// 页级校验和存储。
pub struct CrcStore {
    dm: Arc<DiskManager>,
    /// L1 链头（Page 0 的 `crc_dir_root`）
    root: PageId,
    /// 有界目录页缓存
    cache: HashMap<PageId, CachedDirPage>,
    /// 页 1..255 的内联 CRC 在内存中的权威副本。
    ///
    /// 刻意不直接读改写 Page 0 的磁盘字节：Page 0 同时由缓冲池持有（Header 元数据），
    /// 两个写入者会互相覆盖。改为在此累积，由 [`CrcStore::flush`] 作为**最后一步**
    /// 统一写入 Page 0，顺序确定。
    inline: [u32; HeaderPage::INLINE_CRC_PAGE_COUNT],
    /// 内联区相对 Page 0 的基线快照：flush 时在此之上叠加本轮的更新
    inline_dirty: bool,
    /// 目录根页号自上次 flush 后是否变化（变化时由 flush 写入 Page 0）
    root_changed: bool,
    clock: u64,
}

impl CrcStore {
    /// 创建一个校验和存储句柄。
    ///
    /// `root` 为 Page 0 中记录的 L1 链头（`INVALID_PAGE_ID` 表示尚未建立目录，
    /// 此时所有页都还没有 CRC，读取一律跳过校验）。
    pub fn new(dm: Arc<DiskManager>, root: PageId) -> Self {
        let mut inline = [0u32; HeaderPage::INLINE_CRC_PAGE_COUNT];
        // 载入已有的内联区，使冷启动后仍能校验前 256 页
        let mut page0 = [0u8; PAGE_SIZE];
        if dm.read_page(0, &mut page0).is_ok() {
            for (pid, slot) in inline.iter_mut().enumerate() {
                let off = HeaderPage::INLINE_CRC_OFFSET + pid * 4;
                *slot = u32::from_le_bytes(page0[off..off + 4].try_into().unwrap());
            }
        }
        Self {
            dm,
            root,
            cache: HashMap::new(),
            inline,
            inline_dirty: false,
            root_changed: false,
            clock: 0,
        }
    }

    pub fn root(&self) -> PageId {
        self.root
    }

    /// 该页是否需要校验（即是否有已记录的 CRC）
    fn is_covered(page_id: PageId) -> bool {
        page_id > 0
    }

    /// 读取某页已记录的 CRC32；返回 `None` 表示「未记录」（应跳过校验）。
    fn get_checksum(&mut self, page_id: PageId) -> Result<Option<u32>, GraphError> {
        if !Self::is_covered(page_id) {
            return Ok(None);
        }

        if (page_id as usize) < HeaderPage::INLINE_CRC_PAGE_COUNT {
            let v = self.inline[page_id as usize];
            return Ok(if v == 0 { None } else { Some(v) });
        }

        if self.root == INVALID_PAGE_ID || self.root == 0 {
            return Ok(None);
        }

        let slot_result = self.locate_slot(page_id)?;
        let (l2_pid, slot) = match slot_result {
            Some(x) => x,
            None => return Ok(None),
        };

        let data = self.load_dir_page(l2_pid)?;
        let off = CrcDirPage::ENTRIES_OFFSET + slot * 4;
        let v = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        Ok(if v == 0 { None } else { Some(v) })
    }

    /// 把页号映射为该页在**目录区**中的线性下标。
    ///
    /// 内联区覆盖页 `1..=255`（数组下标即页号），因此目录区的地址空间从页 256
    /// 开始，下标从 0 起：
    ///
    /// ```text
    /// index = page_id - INLINE_CRC_PAGE_COUNT     (page_id >= 256)
    /// ```
    ///
    /// 注意不能写成 `page_id - 1`：那会让页 256 与页 255 落在同一个槽位上，
    /// 导致 256 之后的页互相覆盖校验和——表面上有目录，实际并不覆盖。
    fn dir_index(page_id: PageId) -> Option<u64> {
        let first = HeaderPage::INLINE_CRC_PAGE_COUNT as u64;
        if (page_id as u64) < first {
            return None;
        }
        Some(page_id as u64 - first)
    }

    /// 计算某页的 `(L2 页号, 页内槽位)`；目录不存在时返回 `None`。
    fn locate_slot(&mut self, page_id: PageId) -> Result<Option<(PageId, usize)>, GraphError> {
        let n = match Self::dir_index(page_id) {
            Some(n) => n,
            None => return Ok(None),
        };
        let per_l2 = CrcDirPage::ENTRIES_PER_PAGE as u64;
        let slot = (n % per_l2) as usize;
        let l2_index = ((n / per_l2) % per_l2) as usize;
        let l1_index = (n / (per_l2 * per_l2)) as usize;

        let l1_pid = match self.walk_l1(self.root, l1_index, false)? {
            Some(p) => p,
            None => return Ok(None),
        };
        let l1 = self.load_dir_page(l1_pid)?;

        let off = CrcDirPage::ENTRIES_OFFSET + l2_index * 4;
        let l2_pid = u32::from_le_bytes(l1[off..off + 4].try_into().unwrap());
        if l2_pid == 0 || l2_pid == INVALID_PAGE_ID {
            return Ok(None);
        }
        Ok(Some((l2_pid, slot)))
    }

    /// 沿 L1 链走 `index` 步；`create` 为真时按需生成缺失的链节。
    fn walk_l1(
        &mut self,
        start: PageId,
        index: usize,
        create: bool,
    ) -> Result<Option<PageId>, GraphError> {
        let mut cur = start;
        for _ in 0..index {
            if cur == 0 || cur == INVALID_PAGE_ID {
                // 链头本身就是无效页号：没有“上一页”可以把新页挂上去，
                // 因此无法在不猜测的前提下补链。诚实地报错，不伪造修复。
                // 调用方在进入前已保证 `start` 有效，故正常路径不会走到这里。
                if !create {
                    return Ok(None);
                }
                return Err(GraphError::StorageError(
                    "CRC directory chain head is invalid".into(),
                ));
            }
            let next = {
                let data = self.load_dir_page(cur)?;
                CrcDirPage::next(data)
            };
            if next == 0 || next == INVALID_PAGE_ID {
                if !create {
                    return Ok(None);
                }
                let new_page = self.alloc_dir_page()?;
                // 把当前页的 next 指向新页
                let d = self.load_dir_page_mut(cur)?;
                CrcDirPage::set_next(&mut d.data, new_page);
                d.dirty = true;
                cur = new_page;
            } else {
                cur = next;
            }
        }
        Ok(Some(cur))
    }

    /// 分配一张新的目录页。
    ///
    /// 统一走 [`DiskManager::allocate_page`]，它是全库唯一的页号来源，因此目录页
    /// 永远不会与数据页分配冲突（两者共享同一个 `num_pages` 高水位）。
    fn alloc_dir_page(&mut self) -> Result<PageId, GraphError> {
        let pid = self.dm.allocate_page()?;
        let mut data = [0u8; PAGE_SIZE];
        CrcDirPage::init(&mut data, 1);
        self.cache.insert(
            pid,
            CachedDirPage {
                data,
                dirty: true,
                used_at: self.clock,
            },
        );
        self.clock = self.clock.wrapping_add(1);
        self.evict_if_needed()?;
        Ok(pid)
    }

    /// 记录某页的 CRC32（页内容刚被写入主文件时调用）。
    pub fn record(&mut self, page_id: PageId, data: &[u8; PAGE_SIZE]) -> Result<(), GraphError> {
        if !Self::is_covered(page_id) {
            return Ok(());
        }
        let crc = crc32fast::hash(data);

        if (page_id as usize) < HeaderPage::INLINE_CRC_PAGE_COUNT {
            self.inline[page_id as usize] = crc;
            self.inline_dirty = true;
            return Ok(());
        }

        // 需要目录：没有就建
        if self.root == INVALID_PAGE_ID || self.root == 0 {
            let l1 = self.alloc_dir_page()?;
            self.root = l1;
        }

        let n = match Self::dir_index(page_id) {
            Some(n) => n,
            None => return Ok(()),
        };
        let per_l2 = CrcDirPage::ENTRIES_PER_PAGE as u64;
        let slot = (n % per_l2) as usize;
        let l2_index = ((n / per_l2) % per_l2) as usize;
        let l1_index = (n / (per_l2 * per_l2)) as usize;

        let l1_pid = self
            .walk_l1(self.root, l1_index, true)?
            .ok_or_else(|| GraphError::StorageError("CRC L1 walk failed".into()))?;

        // 取/建 L2 页
        let (l2_pid, need_new) = {
            let l1 = self.load_dir_page(l1_pid)?;
            let off = CrcDirPage::ENTRIES_OFFSET + l2_index * 4;
            let pid = u32::from_le_bytes(l1[off..off + 4].try_into().unwrap());
            if pid == 0 || pid == INVALID_PAGE_ID {
                (0, true)
            } else {
                (pid, false)
            }
        };

        let l2_pid = if need_new {
            let new_l2 = self.alloc_dir_page()?;
            let l1 = self.load_dir_page_mut(l1_pid)?;
            let off = CrcDirPage::ENTRIES_OFFSET + l2_index * 4;
            l1.data[off..off + 4].copy_from_slice(&new_l2.to_le_bytes());
            l1.dirty = true;
            new_l2
        } else {
            l2_pid
        };

        let l2 = self.load_dir_page_mut(l2_pid)?;
        let off = CrcDirPage::ENTRIES_OFFSET + slot * 4;
        l2.data[off..off + 4].copy_from_slice(&crc.to_le_bytes());
        let count = CrcDirPage::entry_count(&l2.data).max(slot + 1);
        CrcDirPage::set_entry_count(&mut l2.data, count);
        l2.dirty = true;

        Ok(())
    }

    /// 清除某页的 CRC（该页被回收复用或释放时调用）。
    ///
    /// 置 `0` 即「未记录」，读取时跳过校验。这防止复用的页被上一任主人的校验和误判。
    pub fn clear(&mut self, page_id: PageId) -> Result<(), GraphError> {
        if !Self::is_covered(page_id) {
            return Ok(());
        }

        if (page_id as usize) < HeaderPage::INLINE_CRC_PAGE_COUNT {
            self.inline[page_id as usize] = 0;
            self.inline_dirty = true;
            return Ok(());
        }

        if let Some((l2_pid, slot)) = self.locate_slot(page_id)? {
            let l2 = self.load_dir_page_mut(l2_pid)?;
            let off = CrcDirPage::ENTRIES_OFFSET + slot * 4;
            l2.data[off..off + 4].copy_from_slice(&0u32.to_le_bytes());
            l2.dirty = true;
        }
        Ok(())
    }

    /// 校验某页内容；不符则返回 [`GraphError::PageChecksumMismatch`]。
    ///
    /// 未记录 CRC 的页直接通过（见模块头「宁可漏检，绝不假报」）。
    pub fn verify(&mut self, page_id: PageId, data: &[u8; PAGE_SIZE]) -> Result<(), GraphError> {
        let expected = match self.get_checksum(page_id)? {
            Some(v) => v,
            None => return Ok(()),
        };
        let actual = crc32fast::hash(data);
        if actual != expected {
            return Err(GraphError::PageChecksumMismatch {
                page_id: page_id as u64,
                expected,
                actual,
            });
        }
        Ok(())
    }

    /// 把所有脏目录页与 Page 0 的内联区写回磁盘并 fsync。
    ///
    /// 调用方必须在本函数返回之后才允许截断 WAL：否则崩溃时数据页已写而校验和未写，
    /// 下次打开会把不一致误判成损坏。
    pub fn flush(&mut self) -> Result<(), GraphError> {
        // 1. 目录页
        let dirty: Vec<PageId> = self
            .cache
            .iter()
            .filter(|(_, p)| p.dirty)
            .map(|(&pid, _)| pid)
            .collect();
        for pid in dirty {
            if let Some(entry) = self.cache.get_mut(&pid) {
                CrcDirPage::seal(&mut entry.data);
                self.dm.write_page(pid, &entry.data)?;
                entry.dirty = false;
            }
        }

        // 2. Page 0：本函数是 Page 0 的**最后写入者**，因此在这里同时落下
        //    内联 CRC 区与目录根页号。Header 的其余字段由 `sync_header()` 先写好，
        //    这里只读-改-写属于自己的两个区域，不会互相覆盖。
        //
        //    写入条件刻意放宽为「缓存里存在任何目录页」而不只是「本轮有变化」：
        //    `sync_header()` 会重写 Page 0 的其它字段，若它在本函数之后落盘，
        //    就可能把 root 覆盖回旧值；而本函数只在有变化时才动 Page 0，
        //    就会漏掉这种「别人改了 Page 0」的情形。只要目录存在，就把
        //    属于自己的两个字段重新钉一遍——幂等，且代价是一次 4KB 读改写。
        let dir_exists = self.root != INVALID_PAGE_ID && self.root != 0;
        if dir_exists || self.inline_dirty {
            let mut page0 = [0u8; PAGE_SIZE];
            self.dm.read_page(0, &mut page0)?;
            for (pid, &crc) in self.inline.iter().enumerate() {
                let off = HeaderPage::INLINE_CRC_OFFSET + pid * 4;
                page0[off..off + 4].copy_from_slice(&crc.to_le_bytes());
            }
            let off = HeaderPage::CRC_DIR_PAGE_OFFSET;
            page0[off..off + 4].copy_from_slice(&self.root.to_le_bytes());
            self.dm.write_page(0, &page0)?;
            self.inline_dirty = false;
            self.root_changed = false;
        }

        self.dm.sync_all()?;
        Ok(())
    }

    /// 从磁盘读取一张目录页（带缓存）
    ///
    /// 目录页自身也做自校验：一张被静默损坏的 L2 页会把它覆盖的**全部**数据页
    /// 报成校验和不匹配——成千上万条假阳性，而真正的坏页（这张目录页）却不在
    /// 报告里。自校验把「目录页坏了」与「数据页坏了」区分开，符合
    /// 「宁可漏检，绝不假报」的原则。
    fn load_dir_page(&mut self, pid: PageId) -> Result<&[u8; PAGE_SIZE], GraphError> {
        if !self.cache.contains_key(&pid) {
            let mut data = [0u8; PAGE_SIZE];
            self.dm.read_page(pid, &mut data)?;
            // self_crc == 0 表示未封存（新分配或旧格式），此时跳过自校验
            if !CrcDirPage::verify_sealed(&data) {
                return Err(GraphError::StorageError(format!(
                    "CRC directory page {} is corrupt (self-checksum mismatch)",
                    pid
                )));
            }
            self.clock = self.clock.wrapping_add(1);
            let used_at = self.clock;
            self.cache.insert(
                pid,
                CachedDirPage {
                    data,
                    dirty: false,
                    used_at,
                },
            );
            self.evict_if_needed()?;
        }
        if let Some(e) = self.cache.get_mut(&pid) {
            self.clock = self.clock.wrapping_add(1);
            e.used_at = self.clock;
        }
        Ok(&self
            .cache
            .get(&pid)
            .ok_or_else(|| GraphError::StorageError("CRC dir page vanished".into()))?
            .data)
    }

    fn load_dir_page_mut(&mut self, pid: PageId) -> Result<&mut CachedDirPage, GraphError> {
        if !self.cache.contains_key(&pid) {
            let mut data = [0u8; PAGE_SIZE];
            self.dm.read_page(pid, &mut data)?;
            if !CrcDirPage::verify_sealed(&data) {
                return Err(GraphError::StorageError(format!(
                    "CRC directory page {} is corrupt (self-checksum mismatch)",
                    pid
                )));
            }
            self.clock = self.clock.wrapping_add(1);
            let used_at = self.clock;
            self.cache.insert(
                pid,
                CachedDirPage {
                    data,
                    dirty: false,
                    used_at,
                },
            );
            self.evict_if_needed()?;
        }
        self.clock = self.clock.wrapping_add(1);
        let clock = self.clock;
        let e = self
            .cache
            .get_mut(&pid)
            .ok_or_else(|| GraphError::StorageError("CRC dir page vanished".into()))?;
        e.used_at = clock;
        Ok(e)
    }

    /// 缓存满时淘汰最旧的一张**干净**页（脏页先落盘）
    fn evict_if_needed(&mut self) -> Result<(), GraphError> {
        while self.cache.len() > CRC_CACHE_CAPACITY {
            let victim = self
                .cache
                .iter()
                .min_by_key(|(_, p)| p.used_at)
                .map(|(&pid, _)| pid);
            if let Some(pid) = victim {
                if let Some(entry) = self.cache.get_mut(&pid) {
                    if entry.dirty {
                        // 落盘前必须封存 self_crc：否则这张页带着**上一次**封存的
                        // 校验和落盘，而内容已经变了，下次载入时自校验必然失败
                        // （大规模 churn 下正是如此，见 CHANGELOG）。
                        CrcDirPage::seal(&mut entry.data);
                        self.dm.write_page(pid, &entry.data)?;
                        entry.dirty = false;
                    }
                }
                self.cache.remove(&pid);
            } else {
                break;
            }
        }
        Ok(())
    }
}
