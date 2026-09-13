use crate::crc32::Hasher;
use crate::graph::GraphError;
use crate::page::{PageId, INVALID_PAGE_ID, PAGE_SIZE};
use crate::storage::{WalRecord, WalWriter};
use crate::sync_ext::MutexRecoverExt;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 启用 STEAL 外存溢出的最小缓冲池帧数（外存最小工作集水位）。
///
/// 缓冲池小于该水位时，未提交脏页严禁置换（严格 NO-STEAL 防线）；达到或超过该水位时，
/// 未提交脏页可被安全溢出到 WAL 并以 baseline 索引支持回滚，从而允许单个事务修改的物理页
/// 数量远超缓冲池容量（如 1MB / 2MB 极小内存下的数万节点大事务）。
pub const MIN_SPILL_FRAMES: usize = 16;

fn page_crc(data: &[u8; PAGE_SIZE]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

/// 底层物理磁盘页管理器（直接面向单文件 {path} 进行 4KB 物理分页管理，支持 :memory: 纯内存模式）
pub struct DiskManager {
    is_memory: bool,
    file: Mutex<Option<File>>,
    memory_pages: Mutex<Vec<[u8; PAGE_SIZE]>>,
    num_pages: AtomicU64,
    num_reads: AtomicU64,
    num_writes: AtomicU64,
}

impl DiskManager {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        let path_ref = path.as_ref();
        let is_memory = path_ref.to_str() == Some(":memory:") || path_ref.as_os_str().is_empty();

        if is_memory {
            return Ok(Self {
                is_memory: true,
                file: Mutex::new(None),
                memory_pages: Mutex::new(Vec::new()),
                num_pages: AtomicU64::new(0),
                num_reads: AtomicU64::new(0),
                num_writes: AtomicU64::new(0),
            });
        }

        let file_path = path_ref.to_path_buf();
        if let Some(parent) = file_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&file_path)?;

        let file_len = file.metadata()?.len();
        let num_pages = (file_len / PAGE_SIZE as u64).max(1);

        Ok(Self {
            is_memory: false,
            file: Mutex::new(Some(file)),
            memory_pages: Mutex::new(Vec::new()),
            num_pages: AtomicU64::new(num_pages),
            num_reads: AtomicU64::new(0),
            num_writes: AtomicU64::new(0),
        })
    }

    /// 取出文件句柄，**不 panic**。
    ///
    /// `file` 是 `Option<File>`：`:memory:` 模式构造为 `None`，其它模式构造为
    /// `Some`。所有调用点都在 `if self.is_memory { return ... }` 之后，因此这里
    /// 逻辑上不会拿到 `None`——但那条保证跨越函数体，是编译器看不到的不变量。
    ///
    /// 原先四处写的是 `file_guard.as_ref().unwrap()`：一旦有人改动构造顺序或
    /// 新增调用点，就会在运行时 panic。嵌入式库里「一次局部疏忽变成进程终止」
    /// 的代价很高（AGENTS.md §13 的同一理由），因此改为显式映射到 `GraphError`。
    fn file_handle<'a>(
        guard: &'a std::sync::MutexGuard<'_, Option<File>>,
    ) -> Result<&'a File, GraphError> {
        guard.as_ref().ok_or_else(|| {
            GraphError::StorageError(
                "file-backed DiskManager has no open file handle; \
                 this is a bug: `file` is Some whenever `is_memory` is false"
                    .to_string(),
            )
        })
    }

    pub fn read_page(
        &self,
        page_id: PageId,
        buffer: &mut [u8; PAGE_SIZE],
    ) -> Result<(), GraphError> {
        if self.is_memory {
            let pages = self.memory_pages.lock_recover();
            let idx = page_id as usize;
            if idx < pages.len() {
                buffer.copy_from_slice(&pages[idx]);
            } else {
                buffer.fill(0);
            }
            self.num_reads.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let file_guard = self.file.lock_recover();
        let mut file = Self::file_handle(&file_guard)?;
        let offset = (page_id as u64) * (PAGE_SIZE as u64);
        let file_len = file.metadata()?.len();

        if offset >= file_len {
            buffer.fill(0);
            return Ok(());
        }

        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buffer)?;
        self.num_reads.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn write_page(&self, page_id: PageId, buffer: &[u8; PAGE_SIZE]) -> Result<(), GraphError> {
        if self.is_memory {
            let mut pages = self.memory_pages.lock_recover();
            let idx = page_id as usize;
            if idx >= pages.len() {
                pages.resize(idx + 1, [0u8; PAGE_SIZE]);
            }
            pages[idx].copy_from_slice(buffer);
            self.num_writes.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let file_guard = self.file.lock_recover();
        let mut file = Self::file_handle(&file_guard)?;
        let offset = (page_id as u64) * (PAGE_SIZE as u64);
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(buffer)?;
        self.num_writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn allocate_page(&self) -> Result<PageId, GraphError> {
        if self.is_memory {
            let mut pages = self.memory_pages.lock_recover();
            if pages.is_empty() {
                pages.push([0u8; PAGE_SIZE]); // Page 0
                pages.push([0u8; PAGE_SIZE]); // Page 1
                return Ok(1);
            }
            let new_page_id = pages.len() as PageId;
            pages.push([0u8; PAGE_SIZE]);
            return Ok(new_page_id);
        }

        let cur = self.num_pages.fetch_add(1, Ordering::SeqCst);
        let new_page_id = if cur == 0 {
            self.num_pages.fetch_add(1, Ordering::SeqCst) as PageId
        } else {
            cur as PageId
        };

        Ok(new_page_id)
    }

    pub fn sync_all(&self) -> Result<(), GraphError> {
        if self.is_memory {
            return Ok(());
        }
        let file_guard = self.file.lock_recover();
        if let Some(ref file) = *file_guard {
            file.sync_all()?;
        }
        Ok(())
    }

    pub fn file_size(&self) -> u64 {
        if self.is_memory {
            return (self.memory_pages.lock_recover().len() * PAGE_SIZE) as u64;
        }
        let file_guard = self.file.lock_recover();
        if let Some(ref file) = *file_guard {
            file.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        }
    }
}

/// 标准 LRU 页面置换淘汰器。
///
/// 采用**侵入式双向链表**（`prev`/`next` 数组按 frame_id 索引），使
/// `pin` / `unpin` / `victim` 全部为严格 O(1)，且真实维护 LRU 顺序。
///
/// 此前用 `VecDeque` + 线性扫描实现是 O(池容量)：每次页操作都要遍历整个候选队列，
/// 池越大单次操作越慢，在 16K 帧量级会成为吞吐主瓶颈。中途曾改用「与队尾交换」
/// 的数组实现，但那会破坏 LRU 顺序（被交换到前端的元素会被提前驱逐），故此版本
/// 用链表精确维护顺序。
pub struct LRUReplacer {
    /// 前驱 frame_id（NULL 表示无）
    prev: Vec<usize>,
    /// 后继 frame_id（NULL 表示无）
    next: Vec<usize>,
    /// frame_id 是否在候选链表中
    linked: Vec<bool>,
    head: usize,
    tail: usize,
    len: usize,
}

/// 空指针哨兵
const LRU_NULL: usize = usize::MAX;

impl LRUReplacer {
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// 按缓冲池容量预分配，避免运行期扩容
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            prev: vec![LRU_NULL; capacity],
            next: vec![LRU_NULL; capacity],
            linked: vec![false; capacity],
            head: LRU_NULL,
            tail: LRU_NULL,
            len: 0,
        }
    }

    fn ensure_capacity(&mut self, frame_id: usize) {
        if frame_id >= self.prev.len() {
            let new_len = frame_id + 1;
            self.prev.resize(new_len, LRU_NULL);
            self.next.resize(new_len, LRU_NULL);
            self.linked.resize(new_len, false);
        }
    }

    /// 把 frame 从候选链表中摘除（Pin 住或已淘汰时调用）
    pub fn pin(&mut self, frame_id: usize) {
        self.ensure_capacity(frame_id);
        if !self.linked[frame_id] {
            return;
        }
        self.unlink(frame_id);
    }

    /// 把 frame 追加到链表尾部（Unpin 且引用计数归零时调用）
    pub fn unpin(&mut self, frame_id: usize) {
        self.ensure_capacity(frame_id);
        if self.linked[frame_id] {
            return;
        }
        self.link_back(frame_id);
    }

    /// 从队首开始寻找第一个满足谓词的候选并摘除；找不到返回 `None`。
    ///
    /// 扫描期间不修改链表结构，命中后按链表语义摘除，因此 LRU 顺序始终精确。
    pub fn victim_filter<F>(&mut self, mut predicate: F) -> Option<usize>
    where
        F: FnMut(usize) -> bool,
    {
        let mut cursor = self.head;
        let mut found = None;
        while cursor != LRU_NULL {
            if predicate(cursor) {
                found = Some(cursor);
                break;
            }
            cursor = self.next[cursor];
        }

        let frame_id = found?;
        self.unlink(frame_id);
        Some(frame_id)
    }

    /// 从链表摘除（O(1)）
    fn unlink(&mut self, frame_id: usize) {
        let p = self.prev[frame_id];
        let n = self.next[frame_id];

        if p != LRU_NULL {
            self.next[p] = n;
        } else {
            self.head = n;
        }
        if n != LRU_NULL {
            self.prev[n] = p;
        } else {
            self.tail = p;
        }

        self.prev[frame_id] = LRU_NULL;
        self.next[frame_id] = LRU_NULL;
        self.linked[frame_id] = false;
        self.len -= 1;
    }

    /// 追加到链表尾部（O(1)）
    fn link_back(&mut self, frame_id: usize) {
        self.prev[frame_id] = self.tail;
        self.next[frame_id] = LRU_NULL;
        if self.tail != LRU_NULL {
            self.next[self.tail] = frame_id;
        } else {
            self.head = frame_id;
        }
        self.tail = frame_id;
        self.linked[frame_id] = true;
        self.len += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for LRUReplacer {
    fn default() -> Self {
        Self::new()
    }
}

/// Buffer Pool 中的物理页缓存帧。
///
/// 这里**没有**页级门闩（latch）。此结构体曾带一个 `latch: Arc<RwLock<()>>` 字段，
/// 声明并初始化后**从未被读写**——即死代码，却让读者以为存在页级并发控制。
/// 实际的并发控制是整个 `BufferPoolManager` 外面那一把 `Arc<Mutex<..>>`，因此
/// 读会被串行化；这是已测量的限制，代价与修复方向见 AGENTS.md §10 与
/// `docs/benchmarks.md#concurrency-scaling`。
///
/// 真要实现页级并行，需要给帧加真正的 latch 并把缓冲池的并发模型改成分帧锁定
/// ——那是 ROADMAP 里的一项设计工作，不是在这里补一个没人用的字段。
pub struct Frame {
    pub page_id: PageId,
    pub pin_count: usize,
    pub is_dirty: bool,
    pub data: [u8; PAGE_SIZE],
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    pub fn new() -> Self {
        Self {
            page_id: INVALID_PAGE_ID,
            pin_count: 0,
            is_dirty: false,
            data: [0u8; PAGE_SIZE],
        }
    }
}

/// 缓冲池运行监控指标
#[derive(Debug, Clone)]
pub struct BufferStats {
    pub capacity_frames: usize,
    pub used_frames: usize,
    pub dirty_frames: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub hit_rate_percentage: f64,
    pub disk_reads: u64,
    pub disk_writes: u64,
    pub file_size_bytes: u64,
    /// 当前仅存在于 WAL 中的页数（未经 Checkpoint 落回主文件的已提交页 + 未提交溢出页）
    pub wal_page_count: usize,
    /// 累计溢出（STEAL）次数
    pub spill_count: u64,
    /// WAL 物理体积
    pub wal_size_bytes: u64,
    /// 累计 WAL fsync 次数（批量提交时一次 commit 应恰好 +1）
    pub wal_fsync_count: u64,
    /// 累计写入的 WAL 帧数
    pub wal_frames_written: u64,
    /// 累计页驱逐次数（用于区分「容量不足」与「结构性重复访问」）
    pub evictions: u64,
}

/// 4KB 缓冲池管理器 (BufferPoolManager)
///
/// 严格管理 Page Pin/Unpin 计数、Dirty 脏页标记、页级门闩与 LRU 换页淘汰。
/// 淘汰策略为 SQLite 风格的 STEAL：未提交脏页在缓冲池达到最小工作集水位时可被
/// 溢出至 WAL（记入 `wal_pages` 位置索引），并持有事务基线（`tx_baseline`）
/// 以支持任意时刻的精确回滚。
pub struct BufferPoolManager {
    disk_manager: Arc<DiskManager>,
    pool_size: usize,
    frames: Vec<Frame>,
    page_table: HashMap<PageId, usize>, // PageId -> FrameId
    free_list: Vec<usize>,              // 空闲 FrameId 列表
    replacer: LRUReplacer,
    uncommitted_pages: HashSet<PageId>, // 事务中修改的未提交脏页
    /// 该页最新镜像仅在 WAL 中的位置索引（SQLite wal-index 同性质）
    wal_pages: HashMap<PageId, u64>,
    /// 本事务首次触碰各未提交页时的 WAL 位置基线（None 表示当时主文件即权威副本）
    tx_baseline: HashMap<PageId, Option<u64>>,
    /// 置换豁免页：Page 0（Header）与多级页目录页必须常驻，严禁被数据页踢出。
    /// 目录页穿透发生在每次节点/边寻址上，一旦被换出会引发整条寻址链反复重读。
    protected_pages: HashSet<PageId>,
    wal: Option<Arc<WalWriter>>,
    /// 页级校验和存储（惰性计算：只在页真正落盘时算，见
    /// [`BufferPoolManager::record_checksum_on_flush`]）
    crc: Option<crate::crc::CrcStore>,
    spill_enabled: bool,
    current_tx_id: u64,
    spill_count: u64,
    evictions: u64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

impl BufferPoolManager {
    pub fn new(disk_manager: Arc<DiskManager>, pool_size: usize) -> Self {
        let mut frames = Vec::with_capacity(pool_size);
        let mut free_list = Vec::with_capacity(pool_size);
        for i in 0..pool_size {
            frames.push(Frame::new());
            free_list.push(i);
        }

        let spill_enabled = pool_size >= MIN_SPILL_FRAMES;

        Self {
            disk_manager,
            pool_size,
            frames,
            page_table: HashMap::new(),
            free_list,
            replacer: LRUReplacer::with_capacity(pool_size),
            uncommitted_pages: HashSet::new(),
            wal_pages: HashMap::new(),
            tx_baseline: HashMap::new(),
            protected_pages: HashSet::new(),
            wal: None,
            crc: None,
            spill_enabled,
            current_tx_id: 0,
            spill_count: 0,
            evictions: 0,
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }

    /// 挂载 WAL 写入器：挂载后未提交脏页即可在达到最小工作集水位时安全溢出入 WAL
    pub(crate) fn attach_wal(&mut self, wal: Arc<WalWriter>) {
        self.wal = Some(wal);
    }

    /// 挂载页校验和存储：挂载后，页在**落盘时**记录 CRC、在**从主文件读入时**校验。
    ///
    /// 校验故意不在事务写入路径上做——那只会在每次写页时叠加一次 CRC 计算，
    /// 把吞吐拉低数倍（实测对手实现正是因此在 6900 万边场景从 18 万降到 7.2 万）。
    pub fn attach_crc(&mut self, crc: crate::crc::CrcStore) {
        self.crc = Some(crc);
    }

    /// 把 WAL 中已提交的页重放到主数据文件，**同时为每页记录校验和**。
    ///
    /// 放在缓冲池上是因为它同时持有 `DiskManager` 与 `CrcStore`，无需把后者借出再借回。
    /// 校验和在此处（惰性）计算，而不是在事务写入路径上。
    pub(crate) fn replay_wal_with_crc(
        &mut self,
        wal: &WalWriter,
        db_path: &Path,
    ) -> Result<usize, GraphError> {
        let dm = Arc::clone(&self.disk_manager);
        let crc = &mut self.crc;
        crate::storage::apply_committed_pages_with(wal, db_path, &mut |page_id, data| {
            if let Some(store) = crc.as_mut() {
                store.record(page_id, data)?;
            }
            let _ = &dm;
            Ok(())
        })
    }

    /// 取出 CRC 存储句柄（提交/检查点路径需要它，见 `NervusDb::checkpoint`）
    pub fn take_crc(&mut self) -> Option<crate::crc::CrcStore> {
        self.crc.take()
    }

    /// 记录某页的校验和（供上层在把页写入主文件后调用）
    pub(crate) fn record_checksum(
        &mut self,
        page_id: PageId,
        data: &[u8; PAGE_SIZE],
    ) -> Result<(), GraphError> {
        Self::record_checksum_on_flush(&mut self.crc, page_id, data)
    }

    /// 把 CRC 目录页刷盘并 fsync。
    ///
    /// 必须在**数据页全部落盘之后**调用，否则崩溃时会出现「数据已写、校验和未写」
    /// 的中间态。
    pub(crate) fn flush_crc(&mut self) -> Result<(), GraphError> {
        if let Some(store) = self.crc.as_mut() {
            store.flush()?;
        }
        Ok(())
    }

    /// 当前 CRC 目录根页号（供 Header 持久化）
    pub fn crc_dir_root(&self) -> Option<PageId> {
        self.crc.as_ref().map(|s| s.root())
    }

    /// 记录某页的校验和（页内容已写入主文件之后调用）
    fn record_checksum_on_flush(
        crc: &mut Option<crate::crc::CrcStore>,
        page_id: PageId,
        data: &[u8; PAGE_SIZE],
    ) -> Result<(), GraphError> {
        if let Some(store) = crc.as_mut() {
            store.record(page_id, data)?;
        }
        Ok(())
    }

    /// 校验从主文件读入的页
    fn verify_checksum_on_load(
        crc: &mut Option<crate::crc::CrcStore>,
        page_id: PageId,
        data: &[u8; PAGE_SIZE],
    ) -> Result<(), GraphError> {
        if let Some(store) = crc.as_mut() {
            store.verify(page_id, data)?;
        }
        Ok(())
    }

    /// 获取或从磁盘（或 WAL 溢出副本）调入指定物理页并 Pin 住（返回 frame_id）
    pub fn fetch_page(&mut self, page_id: PageId) -> Result<usize, GraphError> {
        if let Some(&frame_id) = self.page_table.get(&page_id) {
            self.frames[frame_id].pin_count += 1;
            self.replacer.pin(frame_id);
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(frame_id);
        }

        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        let frame_id = self.acquire_frame()?;

        let wal_offset = self.wal_pages.get(&page_id).copied();
        let mut restored_from_wal = false;
        if let Some(offset) = wal_offset {
            if let Some(ref wal) = self.wal {
                if let Some(image) = wal.read_page_image(offset)? {
                    self.frames[frame_id].data.copy_from_slice(&image);
                    restored_from_wal = true;
                }
            }
        }
        if !restored_from_wal {
            // 只有从**主文件**读入的页才校验：WAL 镜像自带帧级 CRC32，
            // 再校验一次主文件的校验和既多余又会因「尚未落盘」而误报。
            self.disk_manager
                .read_page(page_id, &mut self.frames[frame_id].data)?;
            let data = self.frames[frame_id].data;
            Self::verify_checksum_on_load(&mut self.crc, page_id, &data)?;
        }

        let frame = &mut self.frames[frame_id];
        frame.page_id = page_id;
        frame.pin_count = 1;
        frame.is_dirty = false;

        self.page_table.insert(page_id, frame_id);
        self.replacer.pin(frame_id);

        Ok(frame_id)
    }

    /// 释放页面的 Pin 引用，可选标记为脏页
    pub fn unpin_page(&mut self, page_id: PageId, is_dirty: bool) {
        if let Some(&frame_id) = self.page_table.get(&page_id) {
            let frame = &mut self.frames[frame_id];
            if is_dirty {
                frame.is_dirty = true;
            }
            if frame.pin_count > 0 {
                frame.pin_count -= 1;
                if frame.pin_count == 0 {
                    self.replacer.unpin(frame_id);
                }
            }
        }
    }

    /// 分配全新的物理页并在缓冲池中分配 Frame 并 Pin 住
    pub(crate) fn new_page(&mut self) -> Result<(PageId, usize), GraphError> {
        let frame_id = self.acquire_frame()?;
        let page_id = self.disk_manager.allocate_page()?;

        // 物理页被重新分配时，历史 WAL 镜像失效
        self.wal_pages.remove(&page_id);
        // 页被复用：清掉上一任主人的校验和，避免新内容被旧 CRC 误判为损坏
        if let Some(store) = self.crc.as_mut() {
            store.clear(page_id)?;
        }

        let frame = &mut self.frames[frame_id];
        frame.data.fill(0);
        frame.page_id = page_id;
        frame.pin_count = 1;
        frame.is_dirty = true; // 新页待刷盘

        self.page_table.insert(page_id, frame_id);
        self.replacer.pin(frame_id);

        Ok((page_id, frame_id))
    }

    /// 强制刷盘所有已提交脏页（未提交页与仅存在于 WAL 的页绝不会污染主文件）
    pub(crate) fn flush_all_pages(&mut self) -> Result<(), GraphError> {
        for i in 0..self.frames.len() {
            let pid = self.frames[i].page_id;
            if pid == INVALID_PAGE_ID || !self.frames[i].is_dirty {
                continue;
            }
            if self.uncommitted_pages.contains(&pid) {
                continue;
            }
            let data = self.frames[i].data;
            self.disk_manager.write_page(pid, &data)?;
            // 惰性校验和：页刚落到主文件，此刻才算 CRC，避免污染写入热路径
            Self::record_checksum_on_flush(&mut self.crc, pid, &data)?;
            self.frames[i].is_dirty = false;
            self.wal_pages.remove(&pid);
        }
        self.disk_manager.sync_all()?;
        Ok(())
    }

    /// 寻找可用的帧：空闲链表 → 非未提交页 LRU 淘汰 → STEAL 溢出未提交页
    fn acquire_frame(&mut self) -> Result<usize, GraphError> {
        if let Some(frame_id) = self.free_list.pop() {
            return Ok(frame_id);
        }

        // 第一轮：优先淘汰非未提交页（已提交脏页正常写回主文件）
        // 保护页（Header 与目录页）在两轮候选中一律豁免，保证寻址链常驻。
        if let Some(frame_id) = self.replacer.victim_filter(|fid| {
            let pid = self.frames[fid].page_id;
            !self.uncommitted_pages.contains(&pid) && !self.protected_pages.contains(&pid)
        }) {
            self.evict_frame(frame_id)?;
            return Ok(frame_id);
        }

        // 第二轮：STEAL —— 把未提交脏页镜像溢出到 WAL 后安全置换
        if self.spill_enabled {
            if let Some(frame_id) = self.replacer.victim_filter(|fid| {
                let frame = &self.frames[fid];
                frame.page_id != INVALID_PAGE_ID
                    && frame.is_dirty
                    && !self.protected_pages.contains(&frame.page_id)
            }) {
                self.spill_frame(frame_id)?;
                self.evict_frame(frame_id)?;
                return Ok(frame_id);
            }
        }

        Err(GraphError::StorageError(
            "Buffer pool capacity exceeded: all frames are uncommitted dirty pages (NO-STEAL enforced)"
                .into(),
        ))
    }

    /// 将某页登记为置换豁免页（Page 0 Header 与多级页目录页）
    pub fn protect_page(&mut self, page_id: PageId) {
        if page_id != INVALID_PAGE_ID {
            self.protected_pages.insert(page_id);
        }
    }

    /// 清空置换豁免集（事务回滚恢复元数据后调用，再由上层重新同步当前目录页）
    pub(crate) fn clear_protected_pages(&mut self) {
        self.protected_pages.clear();
    }

    /// 淘汰帧：已提交脏页写回主文件，仅存在于 WAL 的页保留其 WAL 位置索引
    fn evict_frame(&mut self, frame_id: usize) -> Result<(), GraphError> {
        let page_id = self.frames[frame_id].page_id;
        if page_id == INVALID_PAGE_ID {
            return Ok(());
        }
        if self.frames[frame_id].is_dirty {
            if self.uncommitted_pages.contains(&page_id) {
                return Err(GraphError::StorageError(
                    "Refusing to evict uncommitted dirty page without spill".into(),
                ));
            }
            let data = self.frames[frame_id].data;
            self.disk_manager.write_page(page_id, &data)?;
            // 惰性校验和：换出落盘时计算，与写入热路径解耦
            Self::record_checksum_on_flush(&mut self.crc, page_id, &data)?;
            self.frames[frame_id].is_dirty = false;
            self.wal_pages.remove(&page_id);
        }
        self.page_table.remove(&page_id);
        self.evictions += 1;
        Ok(())
    }

    /// 将未提交脏页镜像追加到 WAL（STEAL 溢出），并记录其位置索引
    fn spill_frame(&mut self, frame_id: usize) -> Result<(), GraphError> {
        let page_id = self.frames[frame_id].page_id;
        let data = self.frames[frame_id].data;
        let wal = match self.wal {
            Some(ref w) => Arc::clone(w),
            None => {
                return Err(GraphError::StorageError(
                    "Buffer pool capacity exceeded: all frames are uncommitted dirty pages (NO-STEAL enforced)"
                        .into(),
                ))
            }
        };

        let offset = wal.append(&WalRecord::PageWrite {
            tx_id: self.current_tx_id,
            page_id,
            crc32: page_crc(&data),
            data: data.to_vec(),
        })?;

        self.wal_pages.insert(page_id, offset);
        self.frames[frame_id].is_dirty = false;
        self.spill_count += 1;
        Ok(())
    }

    /// 标记物理页为未提交事务修改页，并记录其回滚基线
    pub fn mark_page_uncommitted(&mut self, page_id: PageId) {
        if self.uncommitted_pages.insert(page_id) {
            let baseline = self.wal_pages.get(&page_id).copied();
            self.tx_baseline.insert(page_id, baseline);
        }
    }

    /// 开启/切换事务上下文
    pub fn begin_tx(&mut self, tx_id: u64) {
        self.current_tx_id = tx_id;
        self.tx_baseline.clear();
    }

    /// 事务提交：常驻脏页写入 WAL redo 帧后追加 TxCommit 并 fsync，随后放行未提交标记
    pub(crate) fn commit_tx(
        &mut self,
        tx_id: u64,
        modified_pages: &[PageId],
    ) -> Result<(), GraphError> {
        if let Some(wal) = self.wal.clone() {
            wal.append(&WalRecord::TxBegin { tx_id })?;

            for &pid in modified_pages {
                let frame_id = match self.page_table.get(&pid) {
                    Some(&fid) => fid,
                    None => continue, // 非常驻页的最新镜像已在溢出时写入 WAL
                };
                if !self.frames[frame_id].is_dirty {
                    continue;
                }
                let data = self.frames[frame_id].data;
                let offset = wal.append(&WalRecord::PageWrite {
                    tx_id,
                    page_id: pid,
                    crc32: page_crc(&data),
                    data: data.to_vec(),
                })?;
                self.wal_pages.insert(pid, offset);
                self.frames[frame_id].is_dirty = false;
            }

            wal.append(&WalRecord::TxCommit { tx_id })?;
            wal.sync()?;
        }

        self.mark_pages_committed(modified_pages);
        self.tx_baseline.clear();
        Ok(())
    }

    /// 事务提交后放行修改页，允许后续安全置换刷盘
    fn mark_pages_committed(&mut self, pages: &[PageId]) {
        for pid in pages {
            self.uncommitted_pages.remove(pid);
            self.tx_baseline.remove(pid);
        }
    }

    /// 事务回滚：按基线还原 WAL 位置索引与页内容，未提交数据绝不会残留到主文件
    pub fn rollback_uncommitted_pages(&mut self, pages: &[PageId]) -> Result<(), GraphError> {
        for &pid in pages {
            self.uncommitted_pages.remove(&pid);
            match self.tx_baseline.remove(&pid) {
                Some(Some(offset)) => {
                    self.wal_pages.insert(pid, offset);
                }
                Some(None) => {
                    self.wal_pages.remove(&pid);
                }
                None => {}
            }

            let target = self.wal_pages.get(&pid).copied();
            let frame_id = match self.page_table.get(&pid) {
                Some(&fid) => fid,
                None => continue,
            };
            self.frames[frame_id].is_dirty = false;
            if self.frames[frame_id].pin_count > 0 {
                continue;
            }

            let mut restored = false;
            if let Some(offset) = target {
                if let Some(ref wal) = self.wal {
                    if let Some(image) = wal.read_page_image(offset)? {
                        self.frames[frame_id].data.copy_from_slice(&image);
                        restored = true;
                    }
                }
            }
            if !restored {
                self.disk_manager
                    .read_page(pid, &mut self.frames[frame_id].data)?;
            }
        }
        Ok(())
    }

    /// 清空「仅存在于 WAL」的页位置索引（Checkpoint 落盘并截断 WAL 后调用）
    pub(crate) fn clear_wal_page_index(&mut self) {
        self.wal_pages.clear();
    }

    pub fn get_frame(&self, frame_id: usize) -> &Frame {
        &self.frames[frame_id]
    }

    pub fn get_frame_mut(&mut self, frame_id: usize) -> &mut Frame {
        &mut self.frames[frame_id]
    }

    pub fn disk_manager(&self) -> &Arc<DiskManager> {
        &self.disk_manager
    }

    /// 缓冲池运行指标快照。
    ///
    /// `pub(crate)`：外部经 `NervusDb::buffer_stats()` 获取（它持锁后调用本函数）。
    /// 此前是 `pub`，但全项目只有那一处调用——本文件之外没有第二个调用者。
    pub(crate) fn stats(&self) -> BufferStats {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let rate = if total > 0 {
            (hits as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        let dirty_count = self.frames.iter().filter(|f| f.is_dirty).count();
        let used_count = self.page_table.len();
        let wal_size_bytes = self.wal.as_ref().map(|w| w.len()).unwrap_or(0);
        let wal_fsync_count = self.wal.as_ref().map(|w| w.fsync_count()).unwrap_or(0);
        let wal_frames_written = self.wal.as_ref().map(|w| w.frames_written()).unwrap_or(0);

        BufferStats {
            capacity_frames: self.pool_size,
            used_frames: used_count,
            dirty_frames: dirty_count,
            cache_hits: hits,
            cache_misses: misses,
            hit_rate_percentage: rate,
            disk_reads: self.disk_manager.num_reads.load(Ordering::Relaxed),
            disk_writes: self.disk_manager.num_writes.load(Ordering::Relaxed),
            file_size_bytes: self.disk_manager.file_size(),
            wal_page_count: self.wal_pages.len(),
            spill_count: self.spill_count,
            wal_size_bytes,
            wal_fsync_count,
            wal_frames_written,
            evictions: self.evictions,
        }
    }
}
