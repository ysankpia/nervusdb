use crate::graph::GraphError;
use crate::page::{PageId, INVALID_PAGE_ID, PAGE_SIZE};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// 底层物理磁盘页管理器（直接面向单文件 {path} 进行 4KB 物理分页管理，支持 :memory: 纯内存模式）
pub struct DiskManager {
    file_path: PathBuf,
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
                file_path: PathBuf::from(":memory:"),
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
            file_path,
            is_memory: false,
            file: Mutex::new(Some(file)),
            memory_pages: Mutex::new(Vec::new()),
            num_pages: AtomicU64::new(num_pages),
            num_reads: AtomicU64::new(0),
            num_writes: AtomicU64::new(0),
        })
    }

    pub fn is_memory(&self) -> bool {
        self.is_memory
    }

    pub fn read_page(
        &self,
        page_id: PageId,
        buffer: &mut [u8; PAGE_SIZE],
    ) -> Result<(), GraphError> {
        if self.is_memory {
            let pages = self.memory_pages.lock().unwrap();
            let idx = page_id as usize;
            if idx < pages.len() {
                buffer.copy_from_slice(&pages[idx]);
            } else {
                buffer.fill(0);
            }
            self.num_reads.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let file_guard = self.file.lock().unwrap();
        let mut file = file_guard.as_ref().unwrap();
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
            let mut pages = self.memory_pages.lock().unwrap();
            let idx = page_id as usize;
            if idx >= pages.len() {
                pages.resize(idx + 1, [0u8; PAGE_SIZE]);
            }
            pages[idx].copy_from_slice(buffer);
            self.num_writes.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let file_guard = self.file.lock().unwrap();
        let mut file = file_guard.as_ref().unwrap();
        let offset = (page_id as u64) * (PAGE_SIZE as u64);
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(buffer)?;
        file.flush()?;
        self.num_writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn allocate_page(&self) -> Result<PageId, GraphError> {
        if self.is_memory {
            let mut pages = self.memory_pages.lock().unwrap();
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
        let file_guard = self.file.lock().unwrap();
        if let Some(ref file) = *file_guard {
            file.sync_all()?;
        }
        Ok(())
    }

    pub fn file_size(&self) -> u64 {
        if self.is_memory {
            return (self.memory_pages.lock().unwrap().len() * PAGE_SIZE) as u64;
        }
        let file_guard = self.file.lock().unwrap();
        if let Some(ref file) = *file_guard {
            file.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

/// 标准 LRU 页面置换淘汰器
pub struct LRUReplacer {
    elements: VecDeque<usize>, // 存储待淘汰的 frame_id
}

impl LRUReplacer {
    pub fn new() -> Self {
        Self {
            elements: VecDeque::new(),
        }
    }

    pub fn pin(&mut self, frame_id: usize) {
        if let Some(pos) = self.elements.iter().position(|&id| id == frame_id) {
            self.elements.remove(pos);
        }
    }

    pub fn unpin(&mut self, frame_id: usize) {
        if !self.elements.contains(&frame_id) {
            self.elements.push_back(frame_id);
        }
    }

    pub fn victim(&mut self) -> Option<usize> {
        self.elements.pop_front()
    }

    pub fn victim_filter<F>(&mut self, mut predicate: F) -> Option<usize>
    where
        F: FnMut(usize) -> bool,
    {
        if let Some(pos) = self.elements.iter().position(|&id| predicate(id)) {
            self.elements.remove(pos)
        } else {
            None
        }
    }

    pub fn size(&self) -> usize {
        self.elements.len()
    }
}

impl Default for LRUReplacer {
    fn default() -> Self {
        Self::new()
    }
}

/// Buffer Pool 中的物理页缓存帧（配备页级读写门闩 Page-Level Latch）
pub struct Frame {
    pub page_id: PageId,
    pub pin_count: usize,
    pub is_dirty: bool,
    pub data: [u8; PAGE_SIZE],
    pub latch: Arc<RwLock<()>>,
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
            latch: Arc::new(RwLock::new(())),
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
}

/// 4KB 缓冲池管理器 (BufferPoolManager)
/// 严格管理 Page Pin/Unpin 计数、Dirty 脏页标记、页级门闩与 LRU 换页淘汰
pub struct BufferPoolManager {
    disk_manager: Arc<DiskManager>,
    pool_size: usize,
    frames: Vec<Frame>,
    page_table: HashMap<PageId, usize>, // PageId -> FrameId
    free_list: Vec<usize>,              // 空闲 FrameId 列表
    replacer: LRUReplacer,
    uncommitted_pages: HashSet<PageId>, // 事务中修改的未提交脏页 (严格 NO-STEAL)
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

        Self {
            disk_manager,
            pool_size,
            frames,
            page_table: HashMap::new(),
            free_list,
            replacer: LRUReplacer::new(),
            uncommitted_pages: HashSet::new(),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }

    /// 获取或从磁盘调入指定物理页并 Pin 住（返回 frame_id）
    pub fn fetch_page(&mut self, page_id: PageId) -> Result<usize, GraphError> {
        if let Some(&frame_id) = self.page_table.get(&page_id) {
            self.frames[frame_id].pin_count += 1;
            self.replacer.pin(frame_id);
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(frame_id);
        }

        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        let frame_id = self.find_available_frame()?;

        // 如果被置换的旧页是脏页，写回物理磁盘
        let old_frame = &mut self.frames[frame_id];
        if old_frame.page_id != INVALID_PAGE_ID {
            if old_frame.is_dirty {
                self.disk_manager
                    .write_page(old_frame.page_id, &old_frame.data)?;
                old_frame.is_dirty = false;
            }
            self.page_table.remove(&old_frame.page_id);
        }

        // 从磁盘调入新页数据
        self.disk_manager.read_page(page_id, &mut old_frame.data)?;
        old_frame.page_id = page_id;
        old_frame.pin_count = 1;
        old_frame.is_dirty = false;

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
    pub fn new_page(&mut self) -> Result<(PageId, usize), GraphError> {
        let frame_id = self.find_available_frame()?;
        let page_id = self.disk_manager.allocate_page()?;

        let frame = &mut self.frames[frame_id];
        if frame.page_id != INVALID_PAGE_ID {
            if frame.is_dirty {
                self.disk_manager.write_page(frame.page_id, &frame.data)?;
                frame.is_dirty = false;
            }
            self.page_table.remove(&frame.page_id);
        }

        frame.data.fill(0);
        frame.page_id = page_id;
        frame.pin_count = 1;
        frame.is_dirty = true; // 新页待刷盘

        self.page_table.insert(page_id, frame_id);
        self.replacer.pin(frame_id);

        Ok((page_id, frame_id))
    }

    /// 强制刷盘单个页
    pub fn flush_page(&mut self, page_id: PageId) -> Result<(), GraphError> {
        if let Some(&frame_id) = self.page_table.get(&page_id) {
            let frame = &mut self.frames[frame_id];
            if frame.is_dirty {
                self.disk_manager.write_page(page_id, &frame.data)?;
                frame.is_dirty = false;
            }
        }
        Ok(())
    }

    /// 强制刷盘所有脏页
    pub fn flush_all_pages(&mut self) -> Result<(), GraphError> {
        for frame in &mut self.frames {
            if frame.page_id != INVALID_PAGE_ID && frame.is_dirty {
                self.disk_manager.write_page(frame.page_id, &frame.data)?;
                frame.is_dirty = false;
            }
        }
        self.disk_manager.sync_all()?;
        Ok(())
    }

    /// 提取当前所有脏页快照（供页级 WAL 刷盘使用）
    pub fn get_dirty_page_snapshots(&self) -> Vec<(PageId, [u8; PAGE_SIZE])> {
        let mut dirty = Vec::new();
        for frame in &self.frames {
            if frame.page_id != INVALID_PAGE_ID && frame.is_dirty {
                dirty.push((frame.page_id, frame.data));
            }
        }
        dirty
    }

    /// 寻找可用的帧：优先空闲链表，其次 LRU 淘汰（严格落实 NO-STEAL）
    fn find_available_frame(&mut self) -> Result<usize, GraphError> {
        if let Some(frame_id) = self.free_list.pop() {
            return Ok(frame_id);
        }

        // 严苛约束：凡是位于 uncommitted_pages 中的物理页帧，绝不允许被淘汰！
        if let Some(frame_id) = self.replacer.victim_filter(|fid| {
            let pid = self.frames[fid].page_id;
            !self.uncommitted_pages.contains(&pid)
        }) {
            return Ok(frame_id);
        }

        Err(GraphError::StorageError(
            "Buffer pool capacity exceeded: all frames are uncommitted dirty pages (NO-STEAL enforced)".into(),
        ))
    }

    /// 标记物理页为未提交事务修改页
    pub fn mark_page_uncommitted(&mut self, page_id: PageId) {
        self.uncommitted_pages.insert(page_id);
    }

    /// 事务提交后放行修改页，允许后续安全置换刷盘
    pub fn mark_pages_committed(&mut self, pages: &[PageId]) {
        for pid in pages {
            self.uncommitted_pages.remove(pid);
        }
    }

    /// 事务回滚时强制丢弃未提交脏页，重置内存帧，防止磁盘污染
    pub fn discard_uncommitted_pages(&mut self, pages: &[PageId]) {
        for &pid in pages {
            self.uncommitted_pages.remove(&pid);
            if let Some(&frame_id) = self.page_table.get(&pid) {
                let frame = &mut self.frames[frame_id];
                if self.disk_manager.read_page(pid, &mut frame.data).is_ok() {
                    frame.is_dirty = false;
                } else {
                    self.page_table.remove(&pid);
                    self.replacer.pin(frame_id);
                    frame.page_id = INVALID_PAGE_ID;
                    frame.pin_count = 0;
                    frame.is_dirty = false;
                    frame.data.fill(0);
                    self.free_list.push(frame_id);
                }
            }
        }
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

    pub fn stats(&self) -> BufferStats {
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
        }
    }
}
