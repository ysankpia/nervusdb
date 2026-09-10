use crate::buffer::BufferPoolManager;
use crate::graph::{Direction, Edge, GraphError, Node, Value};
use crate::page::{
    DirectoryPage, EdgeRecord, HeaderPage, NodeRecord, PageId, PropertyPage, SlottedPropPage,
    DIR_ENTRIES_PER_PAGE, EDGE_RECORDS_PER_PAGE, INVALID_PAGE_ID, NODE_RECORDS_PER_PAGE,
    SLOT_OVERFLOW,
};
use crate::sync_ext::MutexRecoverExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

/// Header 物理页编号
pub const HEADER_PAGE_ID: PageId = 0;

/// 节点复合持久化载荷（标签集合与动态属性字典完整物理存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeData {
    pub labels: HashSet<String>,
    pub properties: HashMap<String, Value>,
}

/// 批量织网的单条边插入请求
#[derive(Debug, Clone)]
pub struct EdgeInsert {
    pub edge_id: u64,
    pub src_id: u64,
    pub dst_id: u64,
    pub edge_type: String,
    pub properties: HashMap<String, Value>,
    pub weight: f64,
}

/// 批量织网的写放大阈值：段长达到该规模才走两阶段批量路径。
///
/// 小批量（如单条边的自动提交）逐条头插的开销可忽略，且能避免为几条边
/// 建立哈希表；只有大批量才值得换取「节点页每页只触碰一次」的收益。
pub const EDGE_BATCH_WEAVE_MIN: usize = 64;

/// 字符串字典管理器：将 Label 和 EdgeType 映射为 u32 ID
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StringDict {
    str_to_id: HashMap<String, u32>,
    id_to_str: HashMap<u32, String>,
    next_id: u32,
}

impl StringDict {
    pub fn new() -> Self {
        Self {
            str_to_id: HashMap::new(),
            id_to_str: HashMap::new(),
            next_id: 1, // 0 保留表示无/空
        }
    }

    pub fn get_or_intern(&mut self, s: &str) -> (u32, bool) {
        if s.is_empty() {
            return (0, false);
        }
        if let Some(&id) = self.str_to_id.get(s) {
            return (id, false);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.str_to_id.insert(s.to_string(), id);
        self.id_to_str.insert(id, s.to_string());
        (id, true)
    }

    pub fn resolve(&self, id: u32) -> Option<&str> {
        if id == 0 {
            return None;
        }
        self.id_to_str.get(&id).map(|s| s.as_str())
    }

    pub fn get_id(&self, s: &str) -> Option<u32> {
        if s.is_empty() {
            return Some(0);
        }
        self.str_to_id.get(s).copied()
    }
}

/// 内部物理页分配元数据
#[derive(Debug, Clone)]
pub struct AllocatorMeta {
    pub allocated_pages: PageId,
    pub first_free_page_id: PageId,
    pub first_free_overflow_page: PageId,
    /// 已腾空可整页复用的槽位属性页回收链
    pub first_free_prop_page: PageId,
    /// 最近分配过的槽位属性页（写入位点提示，避免每次写入都从头探测）
    pub last_prop_page_id: PageId,
    pub node_dir_page_id: PageId,
    pub edge_dir_page_id: PageId,
    pub direct_node_pages: [PageId; HeaderPage::DIRECT_NODE_PAGES_COUNT],
    pub direct_edge_pages: [PageId; HeaderPage::DIRECT_EDGE_PAGES_COUNT],
    pub node_page_cache: HashMap<usize, PageId>,
    pub edge_page_cache: HashMap<usize, PageId>,
}

/// 槽位属性页提示环容量：有界 O(1) 元数据，不承载任何图拓扑
pub const PROP_PAGE_HINT_CAPACITY: usize = 32;
/// 提示环未命中时向后探测的已分配页数上限
const PROP_PAGE_PROBE_LIMIT: u32 = 64;

/// 图轻量元数据快照：用于事务失败时把内存态元数据精确回拨，保证失败事务零残留。
///
/// 该快照只记录 O(1) 规模的标量元数据与字典/目录，不承载任何图拓扑数据，
/// 严格符合「DiskGraph 为唯一数据源」的纯外存架构约束。
#[derive(Debug, Clone)]
pub struct GraphMetaSnapshot {
    next_node_id: u64,
    next_edge_id: u64,
    node_count: usize,
    edge_count: usize,
    first_free_node_id: u64,
    first_free_edge_id: u64,
    dict: StringDict,
    dict_dirty: bool,
    dict_page_id: PageId,
    index_catalog_page_id: PageId,
    index_catalog: crate::index::IndexCatalog,
    allocator: AllocatorMeta,
}

/// 磁盘定长记录图存储驱动引擎
/// 全程依托 BufferPoolManager 换页，杜绝全量加载到 RAM
#[derive(Clone)]
pub struct DiskGraph {
    pub bpm: Arc<Mutex<BufferPoolManager>>,
    pub dict: StringDict,
    pub dict_dirty: bool,
    pub next_node_id: u64,
    pub next_edge_id: u64,
    pub node_count: usize,
    pub edge_count: usize,
    pub first_free_node_id: u64,
    pub first_free_edge_id: u64,
    pub dict_page_id: PageId,
    pub index_catalog_page_id: PageId,
    pub index_catalog: crate::index::IndexCatalog,
    pub tx_modified_pages: Arc<Mutex<HashSet<PageId>>>,
    /// 槽位属性页写入提示环（有界，仅页号，不含属性内容）
    prop_page_hint: VecDeque<PageId>,
    allocator: Arc<Mutex<AllocatorMeta>>,
}

impl DiskGraph {
    pub fn new(bpm: Arc<Mutex<BufferPoolManager>>) -> Result<Self, GraphError> {
        let mut graph = Self {
            bpm,
            dict: StringDict::new(),
            dict_dirty: true,
            next_node_id: 1,
            next_edge_id: 1,
            node_count: 0,
            edge_count: 0,
            first_free_node_id: 0,
            first_free_edge_id: 0,
            dict_page_id: INVALID_PAGE_ID,
            index_catalog_page_id: INVALID_PAGE_ID,
            index_catalog: crate::index::IndexCatalog::default(),
            tx_modified_pages: Arc::new(Mutex::new(HashSet::new())),
            prop_page_hint: VecDeque::new(),
            allocator: Arc::new(Mutex::new(AllocatorMeta {
                allocated_pages: 1,
                first_free_page_id: INVALID_PAGE_ID,
                first_free_overflow_page: INVALID_PAGE_ID,
                first_free_prop_page: INVALID_PAGE_ID,
                last_prop_page_id: INVALID_PAGE_ID,
                node_dir_page_id: INVALID_PAGE_ID,
                edge_dir_page_id: INVALID_PAGE_ID,
                direct_node_pages: [0; HeaderPage::DIRECT_NODE_PAGES_COUNT],
                direct_edge_pages: [0; HeaderPage::DIRECT_EDGE_PAGES_COUNT],
                node_page_cache: HashMap::new(),
                edge_page_cache: HashMap::new(),
            })),
        };
        graph.init_or_load_header()?;
        Ok(graph)
    }

    /// 初始化或加载 Page 0 元数据头
    fn init_or_load_header(&mut self) -> Result<(), GraphError> {
        let mut bpm = self.bpm.lock_recover();
        let frame_id = bpm.fetch_page(HEADER_PAGE_ID)?;
        let frame = bpm.get_frame(frame_id);

        let magic = &frame.data[0..4];
        if magic == crate::page::DB_PAGE_MAGIC || magic == crate::page::DB_PAGE_MAGIC_LEGACY {
            // 物理格式版本守卫：1.0 的「每实体独占整页」属性布局与 1.1 槽位页不兼容
            let file_version = u32::from_le_bytes(
                frame.data[HeaderPage::VERSION_OFFSET..HeaderPage::VERSION_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            if file_version < crate::page::DB_PAGE_VERSION {
                bpm.unpin_page(HEADER_PAGE_ID, false);
                return Err(GraphError::StorageError(format!(
                    "Database file format version {} is not supported by GraphLite 1.1 \
                     (current format version {}). Export the graph with GraphLite 1.0 via \
                     `graphlite-cli <old.db>` then `.dump <file>`, and re-import the script \
                     into a fresh database.",
                    file_version,
                    crate::page::DB_PAGE_VERSION
                )));
            }

            self.next_node_id = u64::from_le_bytes(
                frame.data[HeaderPage::NEXT_NODE_ID_OFFSET..HeaderPage::NEXT_NODE_ID_OFFSET + 8]
                    .try_into()
                    .unwrap(),
            );
            self.next_edge_id = u64::from_le_bytes(
                frame.data[HeaderPage::NEXT_EDGE_ID_OFFSET..HeaderPage::NEXT_EDGE_ID_OFFSET + 8]
                    .try_into()
                    .unwrap(),
            );
            self.node_count = u64::from_le_bytes(
                frame.data[HeaderPage::NODE_COUNT_OFFSET..HeaderPage::NODE_COUNT_OFFSET + 8]
                    .try_into()
                    .unwrap(),
            ) as usize;
            self.edge_count = u64::from_le_bytes(
                frame.data[HeaderPage::EDGE_COUNT_OFFSET..HeaderPage::EDGE_COUNT_OFFSET + 8]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let allocated_pages = u32::from_le_bytes(
                frame.data[HeaderPage::TOTAL_PAGES_OFFSET..HeaderPage::TOTAL_PAGES_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            self.first_free_node_id = u64::from_le_bytes(
                frame.data[HeaderPage::NODE_FREELIST_OFFSET..HeaderPage::NODE_FREELIST_OFFSET + 8]
                    .try_into()
                    .unwrap(),
            );
            self.first_free_edge_id = u64::from_le_bytes(
                frame.data[HeaderPage::EDGE_FREELIST_OFFSET..HeaderPage::EDGE_FREELIST_OFFSET + 8]
                    .try_into()
                    .unwrap(),
            );
            let free_page = u32::from_le_bytes(
                frame.data[HeaderPage::PAGE_FREELIST_OFFSET..HeaderPage::PAGE_FREELIST_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let free_prop_page = u32::from_le_bytes(
                frame.data[HeaderPage::PROP_FREELIST_OFFSET..HeaderPage::PROP_FREELIST_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let last_prop_page = u32::from_le_bytes(
                frame.data
                    [HeaderPage::LAST_PROP_PAGE_OFFSET..HeaderPage::LAST_PROP_PAGE_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            self.dict_page_id = u32::from_le_bytes(
                frame.data[HeaderPage::DICT_PAGE_OFFSET..HeaderPage::DICT_PAGE_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            self.index_catalog_page_id = u32::from_le_bytes(
                frame.data[HeaderPage::INDEX_CATALOG_PAGE_OFFSET
                    ..HeaderPage::INDEX_CATALOG_PAGE_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let node_dir = u32::from_le_bytes(
                frame.data[HeaderPage::NODE_DIR_OFFSET..HeaderPage::NODE_DIR_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let edge_dir = u32::from_le_bytes(
                frame.data[HeaderPage::EDGE_DIR_OFFSET..HeaderPage::EDGE_DIR_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let free_overflow = u32::from_le_bytes(
                frame.data[HeaderPage::OVERFLOW_FREELIST_OFFSET
                    ..HeaderPage::OVERFLOW_FREELIST_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let inline_dict_len = u32::from_le_bytes(
                frame.data
                    [HeaderPage::INLINE_DICT_LEN_OFFSET..HeaderPage::INLINE_DICT_LEN_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let inline_cat_len = u32::from_le_bytes(
                frame.data[HeaderPage::INLINE_CATALOG_LEN_OFFSET
                    ..HeaderPage::INLINE_CATALOG_LEN_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            ) as usize;

            let mut direct_node_pages = [0; HeaderPage::DIRECT_NODE_PAGES_COUNT];
            for (i, p) in direct_node_pages.iter_mut().enumerate() {
                let off = HeaderPage::DIRECT_NODE_PAGES_OFFSET + i * 4;
                *p = u32::from_le_bytes(frame.data[off..off + 4].try_into().unwrap());
            }

            let mut direct_edge_pages = [0; HeaderPage::DIRECT_EDGE_PAGES_COUNT];
            for (i, p) in direct_edge_pages.iter_mut().enumerate() {
                let off = HeaderPage::DIRECT_EDGE_PAGES_OFFSET + i * 4;
                *p = u32::from_le_bytes(frame.data[off..off + 4].try_into().unwrap());
            }

            {
                let mut alloc = self.allocator.lock_recover();
                alloc.allocated_pages = allocated_pages.max(1);
                alloc.node_dir_page_id = node_dir;
                alloc.edge_dir_page_id = edge_dir;
                alloc.first_free_overflow_page = free_overflow;
                alloc.first_free_page_id = free_page;
                alloc.first_free_prop_page = free_prop_page;
                alloc.last_prop_page_id = last_prop_page;
                alloc.direct_node_pages = direct_node_pages;
                alloc.direct_edge_pages = direct_edge_pages;
            }

            let header_bytes = frame.data;
            bpm.unpin_page(HEADER_PAGE_ID, false);

            // Page 0 与已存在的多级页目录页一律登记为置换豁免，
            // 保证寻址链在受限内存下常驻，不被普通数据页踢出。
            bpm.protect_page(HEADER_PAGE_ID);
            if node_dir != INVALID_PAGE_ID && node_dir != 0 {
                bpm.protect_page(node_dir);
            }
            if edge_dir != INVALID_PAGE_ID && edge_dir != 0 {
                bpm.protect_page(edge_dir);
            }

            if self.dict_page_id != INVALID_PAGE_ID && self.dict_page_id != 0 {
                let payload = Self::read_overflow_payload_internal(&mut bpm, self.dict_page_id)?;
                if let Ok(d) = bincode::deserialize::<StringDict>(&payload) {
                    self.dict = d;
                }
            } else if inline_dict_len > 0 && inline_dict_len <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE
            {
                let dict_slice = &header_bytes[HeaderPage::INLINE_PAYLOAD_OFFSET
                    ..HeaderPage::INLINE_PAYLOAD_OFFSET + inline_dict_len];
                if let Ok(d) = bincode::deserialize::<StringDict>(dict_slice) {
                    self.dict = d;
                }
            }

            if self.index_catalog_page_id != INVALID_PAGE_ID && self.index_catalog_page_id != 0 {
                if let Ok(payload) =
                    Self::read_overflow_payload_internal(&mut bpm, self.index_catalog_page_id)
                {
                    if let Ok(cat) = bincode::deserialize::<crate::index::IndexCatalog>(&payload) {
                        self.index_catalog = cat;
                    }
                }
            } else if inline_cat_len > 0
                && inline_dict_len + inline_cat_len <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE
            {
                let cat_start = HeaderPage::INLINE_PAYLOAD_OFFSET + inline_dict_len;
                let cat_slice = &header_bytes[cat_start..cat_start + inline_cat_len];
                if let Ok(cat) = bincode::deserialize::<crate::index::IndexCatalog>(cat_slice) {
                    self.index_catalog = cat;
                }
            }
        } else {
            bpm.unpin_page(HEADER_PAGE_ID, false);
            drop(bpm);
            self.sync_header()?;
        }
        Ok(())
    }

    /// 底层原始物理页分配
    fn raw_allocate_page(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
    ) -> Result<PageId, GraphError> {
        let mut alloc = allocator.lock_recover();
        if alloc.first_free_page_id != INVALID_PAGE_ID && alloc.first_free_page_id != 0 {
            let pid = alloc.first_free_page_id;
            let fid = bpm.fetch_page(pid)?;
            let frame = bpm.get_frame(fid);
            let next_free = u32::from_le_bytes(frame.data[0..4].try_into().unwrap());
            bpm.unpin_page(pid, false);
            alloc.first_free_page_id = next_free;

            let fid = bpm.fetch_page(pid)?;
            let frame = bpm.get_frame_mut(fid);
            frame.data.fill(0);
            bpm.unpin_page(pid, true);

            if let Ok(mut set) = tx_modified.lock() {
                set.insert(pid);
            }
            bpm.mark_page_uncommitted(pid);
            Ok(pid)
        } else {
            let (pid, fid) = bpm.new_page()?;
            let frame = bpm.get_frame_mut(fid);
            frame.data.fill(0);
            bpm.unpin_page(pid, true);

            if pid >= alloc.allocated_pages {
                alloc.allocated_pages = pid + 1;
            }
            if let Ok(mut set) = tx_modified.lock() {
                set.insert(pid);
            }
            bpm.mark_page_uncommitted(pid);
            Ok(pid)
        }
    }

    /// 溢出页分配（优先复用溢出空闲链表）
    fn raw_allocate_overflow_page(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
    ) -> Result<PageId, GraphError> {
        let mut alloc = allocator.lock_recover();
        if alloc.first_free_overflow_page != INVALID_PAGE_ID && alloc.first_free_overflow_page != 0
        {
            let pid = alloc.first_free_overflow_page;
            let fid = bpm.fetch_page(pid)?;
            let frame = bpm.get_frame(fid);
            let next_free = u32::from_le_bytes(frame.data[0..4].try_into().unwrap());
            bpm.unpin_page(pid, false);
            alloc.first_free_overflow_page = next_free;

            let fid = bpm.fetch_page(pid)?;
            let frame = bpm.get_frame_mut(fid);
            frame.data.fill(0);
            bpm.unpin_page(pid, true);

            if let Ok(mut set) = tx_modified.lock() {
                set.insert(pid);
            }
            bpm.mark_page_uncommitted(pid);
            Ok(pid)
        } else {
            drop(alloc);
            Self::raw_allocate_page(bpm, allocator, tx_modified)
        }
    }

    /// 回收溢出页链表
    fn raw_free_overflow_chain(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        start_pid: PageId,
    ) -> Result<(), GraphError> {
        let mut curr = start_pid;
        let mut visited = HashSet::new();
        while curr != INVALID_PAGE_ID && curr != 0 && visited.insert(curr) {
            let fid = bpm.fetch_page(curr)?;
            let frame = bpm.get_frame(fid);
            let (next_pid, _) = PropertyPage::decode(&frame.data);
            bpm.unpin_page(curr, false);

            let mut alloc = allocator.lock_recover();
            let old_free = alloc.first_free_overflow_page;
            alloc.first_free_overflow_page = curr;
            drop(alloc);

            let fid = bpm.fetch_page(curr)?;
            let frame = bpm.get_frame_mut(fid);
            frame.data.fill(0);
            frame.data[0..4].copy_from_slice(&old_free.to_le_bytes());
            bpm.unpin_page(curr, true);

            if let Ok(mut set) = tx_modified.lock() {
                set.insert(curr);
            }
            bpm.mark_page_uncommitted(curr);

            curr = next_pid;
        }
        Ok(())
    }

    /// 分配一张全新的槽位属性页（优先复用已腾空的整页回收链，否则新分配并记入写入位点）
    fn raw_allocate_prop_page(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
    ) -> Result<PageId, GraphError> {
        let recycled = {
            let alloc = allocator.lock_recover();
            let pid = alloc.first_free_prop_page;
            if pid == INVALID_PAGE_ID || pid == 0 {
                None
            } else {
                Some(pid)
            }
        };

        let pid = match recycled {
            Some(pid) => {
                // 从回收链头部弹出，并读出后继
                let fid = bpm.fetch_page(pid)?;
                let next_free = {
                    let frame = bpm.get_frame(fid);
                    u32::from_le_bytes(frame.data[0..4].try_into().unwrap())
                };
                bpm.unpin_page(pid, false);
                allocator.lock_recover().first_free_prop_page = next_free;
                pid
            }
            None => Self::raw_allocate_page(bpm, allocator, tx_modified)?,
        };

        // 初始化为空槽位页
        let fid = bpm.fetch_page(pid)?;
        {
            let frame = bpm.get_frame_mut(fid);
            SlottedPropPage::init(&mut frame.data);
        }
        bpm.unpin_page(pid, true);

        {
            let mut alloc = allocator.lock_recover();
            alloc.last_prop_page_id = pid;
            if pid >= alloc.allocated_pages {
                alloc.allocated_pages = pid + 1;
            }
        }

        if let Ok(mut set) = tx_modified.lock() {
            set.insert(pid);
        }
        bpm.mark_page_uncommitted(pid);
        Ok(pid)
    }

    /// 整页回收一张已腾空的槽位属性页，挂入回收链
    fn raw_free_prop_page(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        pid: PageId,
        prop_page_hint: &mut VecDeque<PageId>,
    ) -> Result<(), GraphError> {
        if pid == INVALID_PAGE_ID || pid == 0 {
            return Ok(());
        }

        let old_free = {
            let mut alloc = allocator.lock_recover();
            let old = alloc.first_free_prop_page;
            alloc.first_free_prop_page = pid;
            if alloc.last_prop_page_id == pid {
                alloc.last_prop_page_id = INVALID_PAGE_ID;
            }
            old
        };

        let fid = bpm.fetch_page(pid)?;
        {
            let frame = bpm.get_frame_mut(fid);
            frame.data.fill(0);
            frame.data[0..4].copy_from_slice(&old_free.to_le_bytes());
        }
        bpm.unpin_page(pid, true);

        if let Ok(mut set) = tx_modified.lock() {
            set.insert(pid);
        }
        bpm.mark_page_uncommitted(pid);

        prop_page_hint.retain(|&candidate| candidate != pid);
        Ok(())
    }

    /// 把页号压入有界提示环（去重，超出容量淘汰最旧项）
    fn push_prop_hint(hint: &mut VecDeque<PageId>, pid: PageId) {
        if pid == INVALID_PAGE_ID || pid == 0 {
            return;
        }
        hint.retain(|&candidate| candidate != pid);
        if hint.len() >= PROP_PAGE_HINT_CAPACITY {
            hint.pop_front();
        }
        hint.push_back(pid);
    }

    /// 尝试把记录插入指定槽位属性页；返回槽位号（不可容纳时返回 `None`）
    fn try_insert_into_prop_page(
        bpm: &mut BufferPoolManager,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        pid: PageId,
        payload: &[u8],
    ) -> Result<Option<u8>, GraphError> {
        if pid == INVALID_PAGE_ID || pid == 0 {
            return Ok(None);
        }
        let fid = bpm.fetch_page(pid)?;
        if !SlottedPropPage::is_slotted(&bpm.get_frame(fid).data) {
            // 该页仍承载溢出链内容（字典/目录等），不可混用
            bpm.unpin_page(pid, false);
            return Ok(None);
        }
        let slot = {
            let frame = bpm.get_frame_mut(fid);
            SlottedPropPage::insert(&mut frame.data, payload)
        };
        match slot {
            Some(slot) => {
                bpm.unpin_page(pid, true);
                bpm.mark_page_uncommitted(pid);
                if let Ok(mut set) = tx_modified.lock() {
                    set.insert(pid);
                }
                Ok(Some(slot))
            }
            None => {
                bpm.unpin_page(pid, false);
                Ok(None)
            }
        }
    }

    /// 查找或新建一张可容纳该记录的槽位属性页
    fn locate_prop_page_for(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        hint: &mut VecDeque<PageId>,
        payload: &[u8],
    ) -> Result<PageId, GraphError> {
        let record_len = payload.len();

        // 1. 提示环：最近写入过的页最可能仍有空间（O(1) 命中热页）
        let hints: Vec<PageId> = hint.iter().copied().collect();
        for pid in hints.into_iter().rev() {
            let fid = bpm.fetch_page(pid)?;
            let fits = {
                let frame = bpm.get_frame(fid);
                SlottedPropPage::is_slotted(&frame.data)
                    && SlottedPropPage::can_fit(&frame.data, record_len)
            };
            bpm.unpin_page(pid, false);
            if fits {
                return Ok(pid);
            }
        }

        // 2. 向后探测：从写入位点起最多扫描 PROP_PAGE_PROBE_LIMIT 张已分配页
        let mut probe = {
            let alloc = allocator.lock_recover();
            let start = if alloc.last_prop_page_id == INVALID_PAGE_ID {
                1
            } else {
                alloc.last_prop_page_id + 1
            };
            (start, alloc.allocated_pages)
        };
        if probe.1 <= probe.0 {
            let alloc = allocator.lock_recover();
            probe = (1, alloc.allocated_pages);
        }

        let upper = probe.0.saturating_add(PROP_PAGE_PROBE_LIMIT);
        let mut candidate = probe.0;
        while candidate < probe.1 && candidate < upper {
            if candidate != 0 && candidate != HEADER_PAGE_ID {
                let fid = bpm.fetch_page(candidate)?;
                let fits = {
                    let frame = bpm.get_frame(fid);
                    SlottedPropPage::is_slotted(&frame.data)
                        && SlottedPropPage::can_fit(&frame.data, record_len)
                };
                bpm.unpin_page(candidate, false);
                if fits {
                    Self::push_prop_hint(hint, candidate);
                    return Ok(candidate);
                }
            }
            candidate += 1;
        }

        // 3. 全部未命中：分配新页
        let pid = Self::raw_allocate_prop_page(bpm, allocator, tx_modified)?;
        Self::push_prop_hint(hint, pid);
        Ok(pid)
    }

    /// 写入一条属性记录，返回打包后的属性指针
    fn write_prop_record(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        hint: &mut VecDeque<PageId>,
        payload: &[u8],
    ) -> Result<u32, GraphError> {
        if payload.is_empty() {
            return Ok(crate::page::PROP_PTR_NONE);
        }

        // 超过 1KB 的行记录走溢出页链
        if payload.len() > crate::page::INLINE_RECORD_MAX {
            let root = {
                let chunk_size = PropertyPage::MAX_PAYLOAD;
                let num_chunks = payload.len().div_ceil(chunk_size);
                let mut pids = Vec::with_capacity(num_chunks);
                for _ in 0..num_chunks {
                    pids.push(Self::raw_allocate_overflow_page(
                        bpm,
                        allocator,
                        tx_modified,
                    )?);
                }
                for i in 0..num_chunks {
                    let pid = pids[i];
                    let next_pid = if i + 1 < num_chunks {
                        pids[i + 1]
                    } else {
                        INVALID_PAGE_ID
                    };
                    let start = i * chunk_size;
                    let end = (start + chunk_size).min(payload.len());
                    let encoded = PropertyPage::encode(next_pid, &payload[start..end]);

                    let fid = bpm.fetch_page(pid)?;
                    {
                        let frame = bpm.get_frame_mut(fid);
                        frame.data.copy_from_slice(&encoded);
                    }
                    bpm.unpin_page(pid, true);
                    if let Ok(mut set) = tx_modified.lock() {
                        set.insert(pid);
                    }
                    bpm.mark_page_uncommitted(pid);
                }
                pids[0]
            };
            return Ok(crate::page::pack_prop_ptr(root, SLOT_OVERFLOW));
        }

        let pid = Self::locate_prop_page_for(bpm, allocator, tx_modified, hint, payload)?;
        match Self::try_insert_into_prop_page(bpm, tx_modified, pid, payload)? {
            Some(slot) => Ok(crate::page::pack_prop_ptr(pid, slot)),
            None => {
                // 定位阶段的判断与实际插入之间存在竞争（同页并发写），退化为新页
                let pid = Self::raw_allocate_prop_page(bpm, allocator, tx_modified)?;
                Self::push_prop_hint(hint, pid);
                match Self::try_insert_into_prop_page(bpm, tx_modified, pid, payload)? {
                    Some(slot) => Ok(crate::page::pack_prop_ptr(pid, slot)),
                    None => Err(GraphError::StorageError(
                        "Failed to insert property record into a freshly allocated slotted page"
                            .into(),
                    )),
                }
            }
        }
    }

    /// 读取属性指针指向的属性记录（自动区分槽位页与溢出链）
    fn read_prop_record(bpm: &mut BufferPoolManager, ptr: u32) -> Result<Vec<u8>, GraphError> {
        if crate::page::is_none_ptr(ptr) {
            return Ok(Vec::new());
        }
        let (page_id, slot) = crate::page::unpack_prop_ptr(ptr);

        if crate::page::is_overflow_ptr(ptr) {
            return Self::read_overflow_payload_internal(bpm, page_id);
        }

        let fid = bpm.fetch_page(page_id)?;
        let data = {
            let frame = bpm.get_frame(fid);
            if !SlottedPropPage::is_slotted(&frame.data) {
                None
            } else {
                SlottedPropPage::read(&frame.data, slot)
            }
        };
        bpm.unpin_page(page_id, false);
        match data {
            Some(bytes) => Ok(bytes),
            None => Err(GraphError::StorageError(format!(
                "Slotted property record missing: page {} slot {}",
                page_id, slot
            ))),
        }
    }

    /// 释放属性指针占用的空间（槽位页标死槽并按需整页回收；溢出链整体回收）
    fn free_prop_record(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        hint: &mut VecDeque<PageId>,
        ptr: u32,
    ) -> Result<(), GraphError> {
        if crate::page::is_none_ptr(ptr) {
            return Ok(());
        }
        let (page_id, slot) = crate::page::unpack_prop_ptr(ptr);

        if crate::page::is_overflow_ptr(ptr) {
            return Self::raw_free_overflow_chain(bpm, allocator, tx_modified, page_id);
        }

        let fid = bpm.fetch_page(page_id)?;
        let became_empty = {
            let frame = bpm.get_frame_mut(fid);
            if SlottedPropPage::is_slotted(&frame.data) {
                let _ = SlottedPropPage::remove(&mut frame.data, slot);
                SlottedPropPage::is_empty(&frame.data)
            } else {
                false
            }
        };
        bpm.unpin_page(page_id, true);
        bpm.mark_page_uncommitted(page_id);
        if let Ok(mut set) = tx_modified.lock() {
            set.insert(page_id);
        }

        if became_empty {
            Self::raw_free_prop_page(bpm, allocator, tx_modified, page_id, hint)?;
        }
        Ok(())
    }

    /// 通过直接页槽位或间接页目录获取/分配节点数据页
    fn get_or_allocate_node_page(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        logical_page: usize,
        is_write: bool,
    ) -> Result<Option<PageId>, GraphError> {
        if logical_page < HeaderPage::DIRECT_NODE_PAGES_COUNT {
            let alloc = allocator.lock_recover();
            let pid = alloc.direct_node_pages[logical_page];
            if pid != 0 && pid != INVALID_PAGE_ID {
                return Ok(Some(pid));
            }
            if !is_write {
                return Ok(None);
            }
            drop(alloc);
            let new_pid = Self::raw_allocate_page(bpm, allocator, tx_modified)?;
            let mut alloc = allocator.lock_recover();
            alloc.direct_node_pages[logical_page] = new_pid;
            if let Ok(mut set) = tx_modified.lock() {
                set.insert(HEADER_PAGE_ID);
            }
            bpm.mark_page_uncommitted(HEADER_PAGE_ID);
            return Ok(Some(new_pid));
        }

        {
            let alloc = allocator.lock_recover();
            if let Some(&pid) = alloc.node_page_cache.get(&logical_page) {
                return Ok(Some(pid));
            }
        }

        let indirect_logical = logical_page - HeaderPage::DIRECT_NODE_PAGES_COUNT;
        let mut root_dir = allocator.lock_recover().node_dir_page_id;
        if root_dir == INVALID_PAGE_ID || root_dir == 0 {
            if !is_write {
                return Ok(None);
            }
            let dir_pid = Self::raw_allocate_page(bpm, allocator, tx_modified)?;
            let fid = bpm.fetch_page(dir_pid)?;
            let f = bpm.get_frame_mut(fid);
            f.data.fill(0);
            DirectoryPage::set_next_dir(&mut f.data, INVALID_PAGE_ID);
            bpm.unpin_page(dir_pid, true);
            bpm.mark_page_uncommitted(dir_pid);

            let mut alloc = allocator.lock_recover();
            alloc.node_dir_page_id = dir_pid;
            if let Ok(mut set) = tx_modified.lock() {
                set.insert(HEADER_PAGE_ID);
            }
            bpm.mark_page_uncommitted(HEADER_PAGE_ID);
            // 页目录页承载每次寻址穿透，登记为置换豁免以杜绝反复重读
            bpm.protect_page(dir_pid);
            root_dir = dir_pid;
        }
        let res = Self::get_or_allocate_page_from_dir(
            bpm,
            allocator,
            tx_modified,
            root_dir,
            indirect_logical,
            is_write,
        )?;
        if let Some(pid) = res {
            allocator
                .lock()
                .unwrap()
                .node_page_cache
                .insert(logical_page, pid);
        }
        Ok(res)
    }

    /// 通过直接页槽位或间接页目录获取/分配边数据页
    fn get_or_allocate_edge_page(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        logical_page: usize,
        is_write: bool,
    ) -> Result<Option<PageId>, GraphError> {
        if logical_page < HeaderPage::DIRECT_EDGE_PAGES_COUNT {
            let alloc = allocator.lock_recover();
            let pid = alloc.direct_edge_pages[logical_page];
            if pid != 0 && pid != INVALID_PAGE_ID {
                return Ok(Some(pid));
            }
            if !is_write {
                return Ok(None);
            }
            drop(alloc);
            let new_pid = Self::raw_allocate_page(bpm, allocator, tx_modified)?;
            let mut alloc = allocator.lock_recover();
            alloc.direct_edge_pages[logical_page] = new_pid;
            if let Ok(mut set) = tx_modified.lock() {
                set.insert(HEADER_PAGE_ID);
            }
            bpm.mark_page_uncommitted(HEADER_PAGE_ID);
            return Ok(Some(new_pid));
        }

        {
            let alloc = allocator.lock_recover();
            if let Some(&pid) = alloc.edge_page_cache.get(&logical_page) {
                return Ok(Some(pid));
            }
        }

        let indirect_logical = logical_page - HeaderPage::DIRECT_EDGE_PAGES_COUNT;
        let mut root_dir = allocator.lock_recover().edge_dir_page_id;
        if root_dir == INVALID_PAGE_ID || root_dir == 0 {
            if !is_write {
                return Ok(None);
            }
            let dir_pid = Self::raw_allocate_page(bpm, allocator, tx_modified)?;
            let fid = bpm.fetch_page(dir_pid)?;
            let f = bpm.get_frame_mut(fid);
            f.data.fill(0);
            DirectoryPage::set_next_dir(&mut f.data, INVALID_PAGE_ID);
            bpm.unpin_page(dir_pid, true);
            bpm.mark_page_uncommitted(dir_pid);

            let mut alloc = allocator.lock_recover();
            alloc.edge_dir_page_id = dir_pid;
            if let Ok(mut set) = tx_modified.lock() {
                set.insert(HEADER_PAGE_ID);
            }
            bpm.mark_page_uncommitted(HEADER_PAGE_ID);
            // 页目录页承载每次寻址穿透，登记为置换豁免以杜绝反复重读
            bpm.protect_page(dir_pid);
            root_dir = dir_pid;
        }
        let res = Self::get_or_allocate_page_from_dir(
            bpm,
            allocator,
            tx_modified,
            root_dir,
            indirect_logical,
            is_write,
        )?;
        if let Some(pid) = res {
            allocator
                .lock()
                .unwrap()
                .edge_page_cache
                .insert(logical_page, pid);
        }
        Ok(res)
    }

    /// 通过页目录检索或动态分配物理数据页
    fn get_or_allocate_page_from_dir(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        root_dir: PageId,
        logical_page: usize,
        is_write: bool,
    ) -> Result<Option<PageId>, GraphError> {
        let dir_steps = logical_page / DIR_ENTRIES_PER_PAGE;
        let entry_idx = logical_page % DIR_ENTRIES_PER_PAGE;

        let mut current_dir = root_dir;
        for _ in 0..dir_steps {
            let fid = bpm.fetch_page(current_dir)?;
            let frame = bpm.get_frame(fid);
            let next_dir = DirectoryPage::get_next_dir(&frame.data);
            bpm.unpin_page(current_dir, false);

            if next_dir == INVALID_PAGE_ID || next_dir == 0 {
                if !is_write {
                    return Ok(None);
                }
                let new_dir_pid = Self::raw_allocate_page(bpm, allocator, tx_modified)?;
                let new_fid = bpm.fetch_page(new_dir_pid)?;
                let new_frame = bpm.get_frame_mut(new_fid);
                new_frame.data.fill(0);
                DirectoryPage::set_next_dir(&mut new_frame.data, INVALID_PAGE_ID);
                bpm.unpin_page(new_dir_pid, true);

                let p_fid = bpm.fetch_page(current_dir)?;
                let p_frame = bpm.get_frame_mut(p_fid);
                DirectoryPage::set_next_dir(&mut p_frame.data, new_dir_pid);
                bpm.unpin_page(current_dir, true);

                // 目录链上每一环都登记为置换豁免
                bpm.protect_page(new_dir_pid);
                if let Ok(mut set) = tx_modified.lock() {
                    set.insert(current_dir);
                    set.insert(new_dir_pid);
                }
                bpm.mark_page_uncommitted(current_dir);
                bpm.mark_page_uncommitted(new_dir_pid);

                current_dir = new_dir_pid;
            } else {
                current_dir = next_dir;
            }
        }

        let fid = bpm.fetch_page(current_dir)?;
        let frame = bpm.get_frame(fid);
        let target_page_id = DirectoryPage::get_entry(&frame.data, entry_idx);
        bpm.unpin_page(current_dir, false);

        if target_page_id != 0 && target_page_id != INVALID_PAGE_ID {
            Ok(Some(target_page_id))
        } else if is_write {
            let new_data_pid = Self::raw_allocate_page(bpm, allocator, tx_modified)?;

            let fid = bpm.fetch_page(current_dir)?;
            let frame = bpm.get_frame_mut(fid);
            DirectoryPage::set_entry(&mut frame.data, entry_idx, new_data_pid);
            bpm.unpin_page(current_dir, true);

            if let Ok(mut set) = tx_modified.lock() {
                set.insert(current_dir);
                set.insert(new_data_pid);
            }
            bpm.mark_page_uncommitted(current_dir);
            bpm.mark_page_uncommitted(new_data_pid);

            Ok(Some(new_data_pid))
        } else {
            Ok(None)
        }
    }

    /// 多页读取变长属性载荷
    fn read_overflow_payload_internal(
        bpm: &mut BufferPoolManager,
        start_pid: PageId,
    ) -> Result<Vec<u8>, GraphError> {
        if start_pid == INVALID_PAGE_ID || start_pid == 0 {
            return Ok(Vec::new());
        }
        let mut data = Vec::new();
        let mut curr = start_pid;
        let mut visited = HashSet::new();

        while curr != INVALID_PAGE_ID && curr != 0 && visited.insert(curr) {
            let fid = bpm.fetch_page(curr)?;
            let frame = bpm.get_frame(fid);
            let (next_pid, chunk) = PropertyPage::decode(&frame.data);
            data.extend_from_slice(&chunk);
            bpm.unpin_page(curr, false);
            curr = next_pid;
        }
        Ok(data)
    }

    /// 同步元数据到 Page 0 (包含直接页槽位与紧凑内联字典与索引目录)
    pub fn sync_header(&mut self) -> Result<(), GraphError> {
        let mut bpm = self.bpm.lock_recover();

        let dict_bytes = bincode::serialize(&self.dict).unwrap_or_default();
        let cat_bytes = bincode::serialize(&self.index_catalog).unwrap_or_default();

        let mut inline_dict_len: u32 = 0;
        let mut inline_cat_len: u32 = 0;

        if dict_bytes.len() + cat_bytes.len() <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE {
            inline_dict_len = dict_bytes.len() as u32;
            inline_cat_len = cat_bytes.len() as u32;
        } else {
            if dict_bytes.len() <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE / 2 {
                inline_dict_len = dict_bytes.len() as u32;
            } else {
                if self.dict_page_id == INVALID_PAGE_ID || self.dict_page_id == 0 {
                    if let Ok(pid) = Self::raw_allocate_overflow_page(
                        &mut bpm,
                        &self.allocator,
                        &self.tx_modified_pages,
                    ) {
                        self.dict_page_id = pid;
                    }
                }
                if self.dict_page_id != INVALID_PAGE_ID && self.dict_page_id != 0 {
                    let encoded = PropertyPage::encode(INVALID_PAGE_ID, &dict_bytes);
                    if let Ok(dict_fid) = bpm.fetch_page(self.dict_page_id) {
                        let dict_frame = bpm.get_frame_mut(dict_fid);
                        dict_frame.data.copy_from_slice(&encoded);
                        bpm.unpin_page(self.dict_page_id, true);
                        bpm.mark_page_uncommitted(self.dict_page_id);
                    }
                }
            }

            if cat_bytes.len() + (inline_dict_len as usize) <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE {
                inline_cat_len = cat_bytes.len() as u32;
            } else {
                if self.index_catalog_page_id == INVALID_PAGE_ID || self.index_catalog_page_id == 0
                {
                    if let Ok(pid) = Self::raw_allocate_overflow_page(
                        &mut bpm,
                        &self.allocator,
                        &self.tx_modified_pages,
                    ) {
                        self.index_catalog_page_id = pid;
                    }
                }
                if self.index_catalog_page_id != INVALID_PAGE_ID && self.index_catalog_page_id != 0
                {
                    let encoded = PropertyPage::encode(INVALID_PAGE_ID, &cat_bytes);
                    if let Ok(fid) = bpm.fetch_page(self.index_catalog_page_id) {
                        let f = bpm.get_frame_mut(fid);
                        f.data.copy_from_slice(&encoded);
                        bpm.unpin_page(self.index_catalog_page_id, true);
                        bpm.mark_page_uncommitted(self.index_catalog_page_id);
                    }
                }
            }
        }

        self.dict_dirty = false;

        let alloc = self.allocator.lock_recover().clone();

        let frame_id = bpm.fetch_page(HEADER_PAGE_ID)?;
        let frame = bpm.get_frame_mut(frame_id);

        frame.data[HeaderPage::MAGIC_OFFSET..HeaderPage::MAGIC_OFFSET + 4]
            .copy_from_slice(crate::page::DB_PAGE_MAGIC);
        frame.data[HeaderPage::VERSION_OFFSET..HeaderPage::VERSION_OFFSET + 4]
            .copy_from_slice(&crate::page::DB_PAGE_VERSION.to_le_bytes());
        frame.data[HeaderPage::PAGE_SIZE_OFFSET..HeaderPage::PAGE_SIZE_OFFSET + 4]
            .copy_from_slice(&(crate::page::PAGE_SIZE as u32).to_le_bytes());
        frame.data[HeaderPage::TOTAL_PAGES_OFFSET..HeaderPage::TOTAL_PAGES_OFFSET + 4]
            .copy_from_slice(&alloc.allocated_pages.to_le_bytes());

        frame.data[HeaderPage::NODE_FREELIST_OFFSET..HeaderPage::NODE_FREELIST_OFFSET + 8]
            .copy_from_slice(&self.first_free_node_id.to_le_bytes());
        frame.data[HeaderPage::EDGE_FREELIST_OFFSET..HeaderPage::EDGE_FREELIST_OFFSET + 8]
            .copy_from_slice(&self.first_free_edge_id.to_le_bytes());
        frame.data[HeaderPage::PAGE_FREELIST_OFFSET..HeaderPage::PAGE_FREELIST_OFFSET + 4]
            .copy_from_slice(&alloc.first_free_page_id.to_le_bytes());

        frame.data[HeaderPage::DICT_PAGE_OFFSET..HeaderPage::DICT_PAGE_OFFSET + 4]
            .copy_from_slice(&self.dict_page_id.to_le_bytes());
        frame.data
            [HeaderPage::INDEX_CATALOG_PAGE_OFFSET..HeaderPage::INDEX_CATALOG_PAGE_OFFSET + 4]
            .copy_from_slice(&self.index_catalog_page_id.to_le_bytes());
        frame.data[HeaderPage::NODE_COUNT_OFFSET..HeaderPage::NODE_COUNT_OFFSET + 8]
            .copy_from_slice(&(self.node_count as u64).to_le_bytes());
        frame.data[HeaderPage::EDGE_COUNT_OFFSET..HeaderPage::EDGE_COUNT_OFFSET + 8]
            .copy_from_slice(&(self.edge_count as u64).to_le_bytes());

        frame.data[HeaderPage::NEXT_NODE_ID_OFFSET..HeaderPage::NEXT_NODE_ID_OFFSET + 8]
            .copy_from_slice(&self.next_node_id.to_le_bytes());
        frame.data[HeaderPage::NEXT_EDGE_ID_OFFSET..HeaderPage::NEXT_EDGE_ID_OFFSET + 8]
            .copy_from_slice(&self.next_edge_id.to_le_bytes());

        frame.data[HeaderPage::NODE_DIR_OFFSET..HeaderPage::NODE_DIR_OFFSET + 4]
            .copy_from_slice(&alloc.node_dir_page_id.to_le_bytes());
        frame.data[HeaderPage::EDGE_DIR_OFFSET..HeaderPage::EDGE_DIR_OFFSET + 4]
            .copy_from_slice(&alloc.edge_dir_page_id.to_le_bytes());
        frame.data[HeaderPage::OVERFLOW_FREELIST_OFFSET..HeaderPage::OVERFLOW_FREELIST_OFFSET + 4]
            .copy_from_slice(&alloc.first_free_overflow_page.to_le_bytes());
        frame.data[HeaderPage::PROP_FREELIST_OFFSET..HeaderPage::PROP_FREELIST_OFFSET + 4]
            .copy_from_slice(&alloc.first_free_prop_page.to_le_bytes());
        frame.data[HeaderPage::LAST_PROP_PAGE_OFFSET..HeaderPage::LAST_PROP_PAGE_OFFSET + 4]
            .copy_from_slice(&alloc.last_prop_page_id.to_le_bytes());

        frame.data[HeaderPage::INLINE_DICT_LEN_OFFSET..HeaderPage::INLINE_DICT_LEN_OFFSET + 4]
            .copy_from_slice(&inline_dict_len.to_le_bytes());
        frame.data
            [HeaderPage::INLINE_CATALOG_LEN_OFFSET..HeaderPage::INLINE_CATALOG_LEN_OFFSET + 4]
            .copy_from_slice(&inline_cat_len.to_le_bytes());

        if inline_dict_len > 0 {
            let start = HeaderPage::INLINE_PAYLOAD_OFFSET;
            let end = start + inline_dict_len as usize;
            frame.data[start..end].copy_from_slice(&dict_bytes);
        }
        if inline_cat_len > 0 {
            let start = HeaderPage::INLINE_PAYLOAD_OFFSET + inline_dict_len as usize;
            let end = start + inline_cat_len as usize;
            frame.data[start..end].copy_from_slice(&cat_bytes);
        }

        for (i, &pid) in alloc.direct_node_pages.iter().enumerate() {
            let off = HeaderPage::DIRECT_NODE_PAGES_OFFSET + i * 4;
            frame.data[off..off + 4].copy_from_slice(&pid.to_le_bytes());
        }

        for (i, &pid) in alloc.direct_edge_pages.iter().enumerate() {
            let off = HeaderPage::DIRECT_EDGE_PAGES_OFFSET + i * 4;
            frame.data[off..off + 4].copy_from_slice(&pid.to_le_bytes());
        }

        bpm.unpin_page(HEADER_PAGE_ID, true);
        bpm.mark_page_uncommitted(HEADER_PAGE_ID);

        if let Ok(mut set) = self.tx_modified_pages.lock() {
            set.insert(HEADER_PAGE_ID);
            if self.dict_page_id != INVALID_PAGE_ID && self.dict_page_id != 0 {
                set.insert(self.dict_page_id);
            }
            if self.index_catalog_page_id != INVALID_PAGE_ID && self.index_catalog_page_id != 0 {
                set.insert(self.index_catalog_page_id);
            }
        }
        Ok(())
    }

    /// 读取原始节点记录（不校验 in_use，供 Freelist 使用）
    pub fn read_node_record_raw(&self, node_id: u64) -> Result<Option<NodeRecord>, GraphError> {
        if node_id == 0 {
            return Ok(None);
        }
        let logical_page = (node_id - 1) as usize / NODE_RECORDS_PER_PAGE;
        let offset = ((node_id - 1) as usize % NODE_RECORDS_PER_PAGE) * NodeRecord::RECORD_SIZE;

        let mut bpm = self.bpm.lock_recover();
        let physical_page = match Self::get_or_allocate_node_page(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            logical_page,
            false,
        )? {
            Some(pid) => pid,
            None => return Ok(None),
        };

        let frame_id = bpm.fetch_page(physical_page)?;
        let frame = bpm.get_frame(frame_id);

        let mut bytes = [0u8; NodeRecord::RECORD_SIZE];
        bytes.copy_from_slice(&frame.data[offset..offset + NodeRecord::RECORD_SIZE]);
        bpm.unpin_page(physical_page, false);

        Ok(Some(NodeRecord::from_bytes(&bytes)))
    }

    /// 读取已使用的节点记录
    pub fn read_node_record(&self, node_id: u64) -> Result<Option<NodeRecord>, GraphError> {
        match self.read_node_record_raw(node_id)? {
            Some(record) if record.in_use == 1 => Ok(Some(record)),
            _ => Ok(None),
        }
    }

    /// 寻址写入节点记录
    pub fn write_node_record(&self, node_id: u64, record: &NodeRecord) -> Result<(), GraphError> {
        if node_id == 0 {
            return Err(GraphError::General("NodeId cannot be 0".into()));
        }
        let logical_page = (node_id - 1) as usize / NODE_RECORDS_PER_PAGE;
        let offset = ((node_id - 1) as usize % NODE_RECORDS_PER_PAGE) * NodeRecord::RECORD_SIZE;

        let mut bpm = self.bpm.lock_recover();
        let physical_page = Self::get_or_allocate_node_page(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            logical_page,
            true,
        )?
        .ok_or_else(|| GraphError::General("Failed to allocate node page".into()))?;

        let frame_id = bpm.fetch_page(physical_page)?;
        let frame = bpm.get_frame_mut(frame_id);

        let bytes = record.to_bytes();
        frame.data[offset..offset + NodeRecord::RECORD_SIZE].copy_from_slice(&bytes);
        bpm.unpin_page(physical_page, true);
        bpm.mark_page_uncommitted(physical_page);

        if let Ok(mut set) = self.tx_modified_pages.lock() {
            set.insert(physical_page);
        }
        Ok(())
    }

    /// 读取原始边记录
    pub fn read_edge_record_raw(&self, edge_id: u64) -> Result<Option<EdgeRecord>, GraphError> {
        if edge_id == 0 {
            return Ok(None);
        }
        let logical_page = (edge_id - 1) as usize / EDGE_RECORDS_PER_PAGE;
        let offset = ((edge_id - 1) as usize % EDGE_RECORDS_PER_PAGE) * EdgeRecord::RECORD_SIZE;

        let mut bpm = self.bpm.lock_recover();
        let physical_page = match Self::get_or_allocate_edge_page(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            logical_page,
            false,
        )? {
            Some(pid) => pid,
            None => return Ok(None),
        };

        let frame_id = bpm.fetch_page(physical_page)?;
        let frame = bpm.get_frame(frame_id);

        let mut bytes = [0u8; EdgeRecord::RECORD_SIZE];
        bytes.copy_from_slice(&frame.data[offset..offset + EdgeRecord::RECORD_SIZE]);
        bpm.unpin_page(physical_page, false);

        Ok(Some(EdgeRecord::from_bytes(&bytes)))
    }

    /// 读取已使用的边记录
    pub fn read_edge_record(&self, edge_id: u64) -> Result<Option<EdgeRecord>, GraphError> {
        match self.read_edge_record_raw(edge_id)? {
            Some(record) if record.in_use == 1 => Ok(Some(record)),
            _ => Ok(None),
        }
    }

    /// 寻址写入边记录
    pub fn write_edge_record(&self, edge_id: u64, record: &EdgeRecord) -> Result<(), GraphError> {
        if edge_id == 0 {
            return Err(GraphError::General("EdgeId cannot be 0".into()));
        }
        let logical_page = (edge_id - 1) as usize / EDGE_RECORDS_PER_PAGE;
        let offset = ((edge_id - 1) as usize % EDGE_RECORDS_PER_PAGE) * EdgeRecord::RECORD_SIZE;

        let mut bpm = self.bpm.lock_recover();
        let physical_page = Self::get_or_allocate_edge_page(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            logical_page,
            true,
        )?
        .ok_or_else(|| GraphError::General("Failed to allocate edge page".into()))?;

        let frame_id = bpm.fetch_page(physical_page)?;
        let frame = bpm.get_frame_mut(frame_id);

        let bytes = record.to_bytes();
        frame.data[offset..offset + EdgeRecord::RECORD_SIZE].copy_from_slice(&bytes);
        bpm.unpin_page(physical_page, true);
        bpm.mark_page_uncommitted(physical_page);

        if let Ok(mut set) = self.tx_modified_pages.lock() {
            set.insert(physical_page);
        }
        Ok(())
    }

    /// 写入节点载荷（标签集合 + 属性字典）为**单条紧凑记录**，返回属性指针。
    ///
    /// 记录 ≤1KB 时紧凑打包进共享槽位页（多条记录共处一页）；>1KB 时走溢出页链。
    pub fn write_node_data(&mut self, data: &NodeData) -> Result<u32, GraphError> {
        let payload = Self::encode_node_data(data)?;

        let mut bpm = self.bpm.lock_recover();
        Self::write_prop_record(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            &mut self.prop_page_hint,
            &payload,
        )
    }

    /// 读取节点载荷
    pub fn read_node_data(&self, ptr: u32) -> Result<NodeData, GraphError> {
        if crate::page::is_none_ptr(ptr) {
            return Ok(NodeData {
                labels: HashSet::new(),
                properties: HashMap::new(),
            });
        }

        let mut bpm = self.bpm.lock_recover();
        let payload = Self::read_prop_record(&mut bpm, ptr)?;
        if payload.is_empty() {
            return Ok(NodeData {
                labels: HashSet::new(),
                properties: HashMap::new(),
            });
        }
        Self::decode_node_data(&payload)
    }

    /// 释放节点载荷占用的存储（槽位退还或溢出链整体回收）
    pub fn free_node_data(&mut self, ptr: u32) -> Result<(), GraphError> {
        let mut bpm = self.bpm.lock_recover();
        Self::free_prop_record(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            &mut self.prop_page_hint,
            ptr,
        )
    }

    /// 写入边属性映射为单条紧凑记录，返回属性指针
    pub fn write_edge_properties(
        &mut self,
        props: &HashMap<String, Value>,
    ) -> Result<u32, GraphError> {
        if props.is_empty() {
            return Ok(crate::page::PROP_PTR_NONE);
        }
        let payload = crate::page::encode_props(props);

        let mut bpm = self.bpm.lock_recover();
        Self::write_prop_record(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            &mut self.prop_page_hint,
            &payload,
        )
    }

    /// 读取边属性映射
    pub fn read_edge_properties(&self, ptr: u32) -> Result<HashMap<String, Value>, GraphError> {
        if crate::page::is_none_ptr(ptr) {
            return Ok(HashMap::new());
        }

        let mut bpm = self.bpm.lock_recover();
        let payload = Self::read_prop_record(&mut bpm, ptr)?;
        if payload.is_empty() {
            return Ok(HashMap::new());
        }
        crate::page::decode_props(&payload)
            .ok_or_else(|| GraphError::SerializationError("corrupted edge property record".into()))
    }

    /// 释放边属性记录占用的存储
    pub fn free_edge_properties(&mut self, ptr: u32) -> Result<(), GraphError> {
        let mut bpm = self.bpm.lock_recover();
        Self::free_prop_record(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            &mut self.prop_page_hint,
            ptr,
        )
    }

    /// 编码节点载荷：`varint(label_count) | label... | encode_props(properties)`
    fn encode_node_data(data: &NodeData) -> Result<Vec<u8>, GraphError> {
        let mut codec = crate::page::PropCodec::new();
        codec.push_varint(data.labels.len() as u64);
        for label in &data.labels {
            codec.push_key(label);
        }
        codec.push_varint(data.properties.len() as u64);
        for (key, value) in &data.properties {
            codec.push_key(key);
            codec.push_value(value);
        }
        Ok(codec.into_bytes())
    }

    /// 解码节点载荷
    fn decode_node_data(payload: &[u8]) -> Result<NodeData, GraphError> {
        let mut reader = crate::page::PropReader::new(payload);

        let label_count = reader
            .read_varint()
            .ok_or_else(|| GraphError::SerializationError("corrupted node label count".into()))?
            as usize;
        let mut labels = HashSet::with_capacity(label_count);
        for _ in 0..label_count {
            let label = reader
                .read_key()
                .ok_or_else(|| GraphError::SerializationError("corrupted node label".into()))?;
            labels.insert(label);
        }

        let prop_count = reader
            .read_varint()
            .ok_or_else(|| GraphError::SerializationError("corrupted node property count".into()))?
            as usize;
        let mut properties = HashMap::with_capacity(prop_count);
        for _ in 0..prop_count {
            let key = reader.read_key().ok_or_else(|| {
                GraphError::SerializationError("corrupted node property key".into())
            })?;
            let value = reader.read_value().ok_or_else(|| {
                GraphError::SerializationError("corrupted node property value".into())
            })?;
            properties.insert(key, value);
        }

        Ok(NodeData { labels, properties })
    }

    /// 分配下一个有效的节点 ID（优先弹出 Freelist，否则自增）
    pub fn allocate_next_node_id(&mut self) -> Result<u64, GraphError> {
        if self.first_free_node_id != 0 {
            let free_id = self.first_free_node_id;
            let free_rec = self.read_node_record_raw(free_id)?;
            if let Some(r) = free_rec {
                self.first_free_node_id = r.first_outgoing_edge_id;
            }
            Ok(free_id)
        } else {
            let id = self.next_node_id;
            self.next_node_id += 1;
            Ok(id)
        }
    }

    /// 分配下一个有效的边 ID（优先弹出 Freelist，否则自增）
    pub fn allocate_next_edge_id(&mut self) -> Result<u64, GraphError> {
        if self.first_free_edge_id != 0 {
            let free_id = self.first_free_edge_id;
            let free_rec = self.read_edge_record_raw(free_id)?;
            if let Some(r) = free_rec {
                self.first_free_edge_id = r.src_next_edge_id;
            }
            Ok(free_id)
        } else {
            let id = self.next_edge_id;
            self.next_edge_id += 1;
            Ok(id)
        }
    }

    /// 添加节点：优先复用 Freelist
    pub fn add_node(
        &mut self,
        labels: HashSet<String>,
        properties: HashMap<String, Value>,
    ) -> Result<u64, GraphError> {
        let node_id = self.allocate_next_node_id()?;
        self.insert_node_with_id_exact(node_id, labels, properties)?;
        self.sync_header()?;
        Ok(node_id)
    }

    /// 插入指定 ID 的节点
    pub fn insert_node_with_id_exact(
        &mut self,
        node_id: u64,
        labels: HashSet<String>,
        properties: HashMap<String, Value>,
    ) -> Result<(), GraphError> {
        if node_id >= self.next_node_id {
            self.next_node_id = node_id + 1;
        }
        self.node_count += 1;

        let label_id = if let Some(first_label) = labels.iter().next() {
            let (lid, newly) = self.dict.get_or_intern(first_label);
            if newly {
                self.dict_dirty = true;
            }
            lid
        } else {
            0
        };

        let node_data = NodeData { labels, properties };
        let prop_page_id = self.write_node_data(&node_data)?;

        let (out_id, in_id) = if let Ok(Some(old)) = self.read_node_record_raw(node_id) {
            (
                if old.in_use == 1 {
                    old.first_outgoing_edge_id
                } else {
                    0
                },
                if old.in_use == 1 {
                    old.first_incoming_edge_id
                } else {
                    0
                },
            )
        } else {
            (0, 0)
        };

        let record = NodeRecord {
            in_use: 1,
            reserved: [0; 3],
            label_id,
            first_outgoing_edge_id: out_id,
            first_incoming_edge_id: in_id,
            prop_page_id,
            inline_prop_val: 0,
        };

        self.write_node_record(node_id, &record)?;
        Ok(())
    }

    /// 更新节点载荷（标签集合 + 属性字典），不改变节点计数与自增序列。
    ///
    /// 供 `SET n:Label` 等「原地改写既有节点」的算子使用，避免误触发新节点分配。
    pub fn update_node_payload(
        &mut self,
        node_id: u64,
        labels: HashSet<String>,
        properties: HashMap<String, Value>,
    ) -> Result<(), GraphError> {
        let mut record = self
            .read_node_record(node_id)?
            .ok_or(GraphError::NodeNotFound(node_id))?;

        let old_ptr = record.prop_page_id;
        let node_data = NodeData { labels, properties };
        let new_ptr = self.write_node_data(&node_data)?;
        record.prop_page_id = new_ptr;

        // 先写新载荷再释放旧载荷，避免中间态丢数据
        if !crate::page::is_none_ptr(old_ptr) && old_ptr != new_ptr {
            self.free_node_data(old_ptr)?;
        }

        self.write_node_record(node_id, &record)?;
        Ok(())
    }

    /// 获取完整 Node 结构体（按需通过 Buffer Pool 调入）
    pub fn get_node(&self, node_id: u64) -> Result<Option<Node>, GraphError> {
        let record = match self.read_node_record(node_id)? {
            Some(r) => r,
            None => return Ok(None),
        };

        let node_data = self.read_node_data(record.prop_page_id)?;
        let outgoing = self.collect_outgoing_edge_ids(record.first_outgoing_edge_id)?;
        let incoming = self.collect_incoming_edge_ids(record.first_incoming_edge_id)?;

        let mut node = Node::new(node_id, node_data.labels, node_data.properties);
        node.outgoing = outgoing;
        node.incoming = incoming;
        Ok(Some(node))
    }

    /// 沿磁盘出边链遍历收集出边 ID (Index-Free Adjacency)
    pub fn collect_outgoing_edge_ids(&self, first_edge_id: u64) -> Result<Vec<u64>, GraphError> {
        let mut ids = Vec::new();
        let mut curr = first_edge_id;
        let mut seen = HashSet::new();
        while curr != 0 && seen.insert(curr) {
            if let Some(edge) = self.read_edge_record(curr)? {
                ids.push(curr);
                curr = edge.src_next_edge_id;
            } else {
                break;
            }
        }
        Ok(ids)
    }

    /// 沿磁盘入边链遍历收集入边 ID
    pub fn collect_incoming_edge_ids(&self, first_edge_id: u64) -> Result<Vec<u64>, GraphError> {
        let mut ids = Vec::new();
        let mut curr = first_edge_id;
        let mut seen = HashSet::new();
        while curr != 0 && seen.insert(curr) {
            if let Some(edge) = self.read_edge_record(curr)? {
                ids.push(curr);
                curr = edge.dst_next_edge_id;
            } else {
                break;
            }
        }
        Ok(ids)
    }

    /// 添加单条有向属性边
    pub fn add_edge(
        &mut self,
        src_id: u64,
        dst_id: u64,
        edge_type: &str,
        properties: HashMap<String, Value>,
        weight: f64,
    ) -> Result<u64, GraphError> {
        if weight < 0.0 || weight.is_nan() {
            return Err(GraphError::InvalidWeight(weight));
        }

        let edge_id = self.allocate_next_edge_id()?;
        self.insert_edge_with_id_exact(edge_id, src_id, dst_id, edge_type, properties, weight)?;
        self.sync_header()?;
        Ok(edge_id)
    }

    /// 插入指定 ID 的边
    pub fn insert_edge_with_id_exact(
        &mut self,
        edge_id: u64,
        src_id: u64,
        dst_id: u64,
        edge_type: &str,
        properties: HashMap<String, Value>,
        weight: f64,
    ) -> Result<(), GraphError> {
        if weight < 0.0 || weight.is_nan() {
            return Err(GraphError::InvalidWeight(weight));
        }

        let mut src_node = self
            .read_node_record(src_id)?
            .ok_or(GraphError::NodeNotFound(src_id))?;
        let mut dst_node = self
            .read_node_record(dst_id)?
            .ok_or(GraphError::NodeNotFound(dst_id))?;

        if edge_id >= self.next_edge_id {
            self.next_edge_id = edge_id + 1;
        }
        self.edge_count += 1;

        let (edge_type_id, newly) = self.dict.get_or_intern(edge_type);
        if newly {
            self.dict_dirty = true;
        }
        if !edge_type.is_empty() {
            self.index_catalog.edge_types.insert(edge_type.to_string());
        }

        // 1. 头插法插入源节点出边链
        let old_src_first = src_node.first_outgoing_edge_id;
        let prop_page_id = if !properties.is_empty() {
            self.write_edge_properties(&properties)?
        } else {
            INVALID_PAGE_ID
        };

        let new_edge = EdgeRecord {
            in_use: 1,
            reserved: [0; 3],
            edge_type_id,
            prop_page_id,
            reserved2: [0; 4],
            src_id,
            dst_id,
            weight,
            src_prev_edge_id: 0,
            src_next_edge_id: old_src_first,
            dst_next_edge_id: dst_node.first_incoming_edge_id,
        };

        if old_src_first != 0 {
            if let Some(mut old_edge) = self.read_edge_record(old_src_first)? {
                old_edge.src_prev_edge_id = edge_id;
                self.write_edge_record(old_src_first, &old_edge)?;
            }
        }
        src_node.first_outgoing_edge_id = edge_id;
        if src_id == dst_id {
            // 自环：src 与 dst 是同一节点，出边头与入边头必须合并为一次写入，
            // 否则第二次写入会用陈旧副本覆盖掉第一次的链头更新。
            src_node.first_incoming_edge_id = edge_id;
            self.write_node_record(src_id, &src_node)?;
        } else {
            self.write_node_record(src_id, &src_node)?;
            // 2. 头插法插入目标节点入边链
            dst_node.first_incoming_edge_id = edge_id;
            self.write_node_record(dst_id, &dst_node)?;
        }

        // 3. 写入当前边记录
        self.write_edge_record(edge_id, &new_edge)?;
        Ok(())
    }

    /// 批量插入边：以「两阶段织网」消除受限内存下的缓存抖动。
    ///
    /// 逐条头插的代价是每条边要对源节点页、目标节点页、旧首边页各访问一次；
    /// 大图离散写入时这些页远超缓冲池容量，同一页在一个批次内被反复换出/读入，
    /// 每个 miss 还会触发一次 4KB 的 STEAL 溢出写入。
    ///
    /// 本方法把织网拆成三步，使每张页在整个批次内**只被触碰一次**：
    /// 1. **规划**：完全按传入顺序在内存中推导双向链指针（与逐条头插逐位等价）：
    ///    同一源节点的链序为「逆插入序」，故某边的 `src_next` 是它在同源序列中的前驱，
    ///    `src_prev` 是同源序列中的后继，链头为同源序列的最后一条；
    ///    入边链同理，仅使用 `dst_next`（与 `EdgeRecord` 字段定义一致）。
    /// 2. **顺序写边记录**：按 `edge_id` 升序写入，使边记录页接近顺序 I/O。
    /// 3. **分簇写节点头**：按节点页把源/目标头指针合并后每节点只写一次。
    ///
    /// 语义与逐条 `insert_edge_with_id_exact` 完全一致，且额外修正了自环场景下
    /// 「先写源节点、再以陈旧副本覆盖写目标节点」导致出边链头被抹掉的隐患。
    pub fn insert_edges_batch(&mut self, requests: &[EdgeInsert]) -> Result<(), GraphError> {
        use crate::page::PROP_PTR_NONE;

        if requests.is_empty() {
            return Ok(());
        }

        // ---------- 阶段 0：校验与去重读取（每张节点页只读一次） ----------
        for r in requests {
            if r.weight < 0.0 || r.weight.is_nan() {
                return Err(GraphError::InvalidWeight(r.weight));
            }
            if r.edge_id == 0 {
                return Err(GraphError::General("EdgeId cannot be 0".into()));
            }
        }

        // node_cache 保存的是**批次前**的节点记录，其链头即批次前的旧链头。
        //
        // 读取顺序按节点逻辑页排序：节点页数量在真实大图上远超缓冲池容量，
        // 若按请求顺序随机读取，每个节点都要付一次页缺失；按页排序后同一张页
        // 的 ≤128 个节点连续读取，使节点页的调页次数从「触达节点数」降到「触达页数」。
        let mut distinct_nodes: Vec<u64> = Vec::with_capacity(requests.len() * 2);
        for r in requests {
            distinct_nodes.push(r.src_id);
            distinct_nodes.push(r.dst_id);
        }
        distinct_nodes.sort_unstable();
        distinct_nodes.dedup();
        distinct_nodes.sort_unstable_by_key(|nid| Self::node_logical_page(*nid));

        let mut node_cache: HashMap<u64, NodeRecord> = HashMap::with_capacity(distinct_nodes.len());
        for nid in distinct_nodes {
            let record = self
                .read_node_record(nid)?
                .ok_or(GraphError::NodeNotFound(nid))?;
            node_cache.insert(nid, record);
        }

        // ---------- 阶段 1：按传入顺序规划链指针 ----------
        // 链结构为「逆插入序」：list.last() 成为新链头，list[0] 的下游接批次前旧链头。
        //
        // 每个 (节点, 边) 的序号用 HashMap 预先建索引：链指针推导从「列表内线性查找」
        // 降为 O(1) 哈希查找，避免同一目标被大量边共享时退化为 O(n²)。
        let mut src_seq: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut dst_seq: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut meta: HashMap<u64, (u64, u64, u32, u32, f64)> =
            HashMap::with_capacity(requests.len());

        for r in requests {
            if r.edge_id >= self.next_edge_id {
                self.next_edge_id = r.edge_id + 1;
            }
            self.edge_count += 1;

            let (edge_type_id, newly) = self.dict.get_or_intern(&r.edge_type);
            if newly {
                self.dict_dirty = true;
            }
            if !r.edge_type.is_empty() {
                self.index_catalog.edge_types.insert(r.edge_type.clone());
            }

            let prop_ptr = if !r.properties.is_empty() {
                self.write_edge_properties(&r.properties)?
            } else {
                PROP_PTR_NONE
            };

            src_seq.entry(r.src_id).or_default().push(r.edge_id);
            dst_seq.entry(r.dst_id).or_default().push(r.edge_id);
            meta.insert(
                r.edge_id,
                (r.src_id, r.dst_id, edge_type_id, prop_ptr, r.weight),
            );
        }

        // (节点, 边) -> 在该节点序列中的位置，供 O(1) 相邻查找
        let mut dst_pos: HashMap<(u64, u64), usize> = HashMap::with_capacity(requests.len());
        for (&node, list) in &dst_seq {
            for (i, &eid) in list.iter().enumerate() {
                dst_pos.insert((node, eid), i);
            }
        }

        // 边记录写入集合：新边与其 id 单调；旧链头回写单独分组，
        // 避免两者混排后在同一批内来回跨越数千页（会冲刷 LRU 并制造假缺失）。
        let mut new_records: Vec<(u64, EdgeRecord)> = Vec::with_capacity(requests.len());
        let mut head_fixups: Vec<(u64, EdgeRecord)> = Vec::new();
        let mut new_src_head: HashMap<u64, u64> = HashMap::with_capacity(src_seq.len());
        let mut new_dst_head: HashMap<u64, u64> = HashMap::with_capacity(dst_seq.len());

        for (&src_id, list) in &src_seq {
            let old_head = node_cache[&src_id].first_outgoing_edge_id;
            new_src_head.insert(src_id, *list.last().unwrap_or(&0));

            for (idx, &eid) in list.iter().enumerate() {
                let (_, dst_id, edge_type_id, prop_ptr, weight) = meta[&eid];
                let src_next = if idx == 0 { old_head } else { list[idx - 1] };
                let src_prev = if idx + 1 < list.len() {
                    list[idx + 1]
                } else {
                    0
                };
                // 目标入边链的下游：同目标序列中的前一条；首条指向批次前旧链头
                let dst_next = match dst_pos.get(&(dst_id, eid)) {
                    Some(&0) => node_cache[&dst_id].first_incoming_edge_id,
                    Some(&p) => dst_seq[&dst_id][p - 1],
                    None => 0,
                };

                new_records.push((
                    eid,
                    EdgeRecord {
                        in_use: 1,
                        reserved: [0; 3],
                        edge_type_id,
                        prop_page_id: prop_ptr,
                        reserved2: [0; 4],
                        src_id,
                        dst_id,
                        weight,
                        src_prev_edge_id: src_prev,
                        src_next_edge_id: src_next,
                        dst_next_edge_id: dst_next,
                    },
                ));
            }

            // 批次前的旧链头不再是链首，其 src_prev 必须指向本批次最早的一条，
            // 否则删除旧链头会截断整条出边链。
            if old_head != 0 {
                let mut rec = self
                    .read_edge_record(old_head)?
                    .ok_or(GraphError::EdgeNotFound(old_head))?;
                rec.src_prev_edge_id = list[0];
                head_fixups.push((old_head, rec));
            }
        }

        for (&dst_id, list) in &dst_seq {
            new_dst_head.insert(dst_id, *list.last().unwrap_or(&0));
        }

        // ---------- 阶段 2：写边记录 ----------
        // 关键：旧链头位于**低 id 区**，新边位于**高 id 区**，两者在批次规模累积后
        // 相距可达上万页，远超缓冲池容量。因此必须「先低后高」地顺序写完一个区
        // 再进入另一个区，任何交错都会让两个区互相驱逐（正是此前的假缺失来源）。
        head_fixups.sort_unstable_by_key(|(eid, _)| *eid);
        for (eid, record) in &head_fixups {
            self.write_edge_record(*eid, record)?;
        }
        new_records.sort_unstable_by_key(|(eid, _)| *eid);
        for (eid, record) in &new_records {
            self.write_edge_record(*eid, record)?;
        }

        // ---------- 阶段 3：按节点页分簇写头指针（每节点只写一次） ----------
        // 以节点所属逻辑页排序，让同一物理页的节点记录在单次页面持有内连续更新。
        let mut touched: Vec<u64> = node_cache.keys().copied().collect();
        touched.sort_unstable_by_key(|nid| Self::node_logical_page(*nid));
        for nid in touched {
            let mut record = node_cache[&nid];
            let mut changed = false;
            if let Some(&head) = new_src_head.get(&nid) {
                if record.first_outgoing_edge_id != head {
                    record.first_outgoing_edge_id = head;
                    changed = true;
                }
            }
            if let Some(&head) = new_dst_head.get(&nid) {
                if record.first_incoming_edge_id != head {
                    record.first_incoming_edge_id = head;
                    changed = true;
                }
            }
            if changed {
                self.write_node_record(nid, &record)?;
            }
        }

        Ok(())
    }

    /// 节点记录所属逻辑页号（纯算术，零磁盘访问）
    fn node_logical_page(node_id: u64) -> usize {
        (node_id.saturating_sub(1)) as usize / NODE_RECORDS_PER_PAGE
    }

    /// 获取指定边实体
    pub fn get_edge(&self, edge_id: u64) -> Result<Option<Edge>, GraphError> {
        let record = match self.read_edge_record(edge_id)? {
            Some(r) => r,
            None => return Ok(None),
        };

        let edge_type = self
            .dict
            .resolve(record.edge_type_id)
            .unwrap_or("RELATED")
            .to_string();
        let properties = self.read_edge_properties(record.prop_page_id)?;

        Ok(Some(Edge::new(
            edge_id,
            record.src_id,
            record.dst_id,
            edge_type,
            properties,
            record.weight,
        )))
    }

    /// 删除单条边（脱链并回收槽位与溢出页）
    pub fn remove_edge(&mut self, edge_id: u64) -> Result<Edge, GraphError> {
        let mut edge = self
            .read_edge_record(edge_id)?
            .ok_or(GraphError::EdgeNotFound(edge_id))?;

        let src_id = edge.src_id;
        let dst_id = edge.dst_id;

        // 1. 从源节点出边链中脱链
        if edge.src_prev_edge_id != 0 {
            if let Some(mut prev) = self.read_edge_record(edge.src_prev_edge_id)? {
                prev.src_next_edge_id = edge.src_next_edge_id;
                self.write_edge_record(edge.src_prev_edge_id, &prev)?;
            }
        } else if let Some(mut src_node) = self.read_node_record(src_id)? {
            src_node.first_outgoing_edge_id = edge.src_next_edge_id;
            self.write_node_record(src_id, &src_node)?;
        }

        if edge.src_next_edge_id != 0 {
            if let Some(mut next) = self.read_edge_record(edge.src_next_edge_id)? {
                next.src_prev_edge_id = edge.src_prev_edge_id;
                self.write_edge_record(edge.src_next_edge_id, &next)?;
            }
        }

        // 2. 从目标节点入边链中脱链
        if let Some(mut dst_node) = self.read_node_record(dst_id)? {
            let mut curr = dst_node.first_incoming_edge_id;
            let mut prev = 0;
            while curr != 0 {
                if curr == edge_id {
                    if prev != 0 {
                        if let Some(mut p) = self.read_edge_record(prev)? {
                            p.dst_next_edge_id = edge.dst_next_edge_id;
                            self.write_edge_record(prev, &p)?;
                        }
                    } else {
                        dst_node.first_incoming_edge_id = edge.dst_next_edge_id;
                        self.write_node_record(dst_id, &dst_node)?;
                    }
                    break;
                }
                prev = curr;
                if let Some(c) = self.read_edge_record(curr)? {
                    curr = c.dst_next_edge_id;
                } else {
                    break;
                }
            }
        }

        let edge_type = self
            .dict
            .resolve(edge.edge_type_id)
            .unwrap_or("RELATED")
            .to_string();
        let properties = self.read_edge_properties(edge.prop_page_id)?;

        // 回收边属性记录（槽位退还或溢出链整体回收）
        if !crate::page::is_none_ptr(edge.prop_page_id) {
            self.free_edge_properties(edge.prop_page_id)?;
        }

        // 3. 边记录标记为未用并串入 Edge Freelist
        edge.in_use = 0;
        edge.prop_page_id = crate::page::PROP_PTR_NONE;
        edge.src_next_edge_id = self.first_free_edge_id;
        self.first_free_edge_id = edge_id;

        self.write_edge_record(edge_id, &edge)?;
        self.edge_count = self.edge_count.saturating_sub(1);
        self.sync_header()?;

        Ok(Edge::new(
            edge_id,
            src_id,
            dst_id,
            edge_type,
            properties,
            edge.weight,
        ))
    }

    /// 删除节点（级联删除所有邻接边，回收槽位与溢出页）
    pub fn remove_node(&mut self, node_id: u64) -> Result<Node, GraphError> {
        let mut node_record = self
            .read_node_record(node_id)?
            .ok_or(GraphError::NodeNotFound(node_id))?;

        let node_data = self.read_node_data(node_record.prop_page_id)?;

        // 1. 级联删除所有出边
        let mut out_edges = Vec::new();
        let mut curr = node_record.first_outgoing_edge_id;
        let mut seen = HashSet::new();
        while curr != 0 && seen.insert(curr) {
            out_edges.push(curr);
            if let Some(e) = self.read_edge_record(curr)? {
                curr = e.src_next_edge_id;
            } else {
                break;
            }
        }
        for eid in out_edges {
            let _ = self.remove_edge(eid);
        }

        // 2. 级联删除所有入边
        let mut in_edges = Vec::new();
        curr = node_record.first_incoming_edge_id;
        seen.clear();
        while curr != 0 && seen.insert(curr) {
            in_edges.push(curr);
            if let Some(e) = self.read_edge_record(curr)? {
                curr = e.dst_next_edge_id;
            } else {
                break;
            }
        }
        for eid in in_edges {
            let _ = self.remove_edge(eid);
        }

        // 回收节点属性记录（槽位退还或溢出链整体回收）
        if !crate::page::is_none_ptr(node_record.prop_page_id) {
            self.free_node_data(node_record.prop_page_id)?;
        }

        // 3. 标记未用并串入 Node Freelist
        node_record.in_use = 0;
        node_record.prop_page_id = crate::page::PROP_PTR_NONE;
        node_record.first_outgoing_edge_id = self.first_free_node_id;
        self.first_free_node_id = node_id;

        self.write_node_record(node_id, &node_record)?;
        self.node_count = self.node_count.saturating_sub(1);
        self.sync_header()?;

        Ok(Node::new(node_id, node_data.labels, node_data.properties))
    }

    /// 更新节点属性（回收旧溢出页，分配新溢出页）
    pub fn update_node_property(
        &mut self,
        node_id: u64,
        key: String,
        value: Value,
    ) -> Result<(), GraphError> {
        let mut record = self
            .read_node_record(node_id)?
            .ok_or(GraphError::NodeNotFound(node_id))?;

        let old_ptr = record.prop_page_id;
        let mut node_data = self.read_node_data(old_ptr)?;
        node_data.properties.insert(key, value);

        let new_ptr = self.write_node_data(&node_data)?;
        record.prop_page_id = new_ptr;

        // 先写新记录再释放旧记录，避免中间态丢数据
        if !crate::page::is_none_ptr(old_ptr) && old_ptr != new_ptr {
            self.free_node_data(old_ptr)?;
        }

        self.write_node_record(node_id, &record)?;
        Ok(())
    }

    /// 更新边属性
    pub fn update_edge_property(
        &mut self,
        edge_id: u64,
        key: String,
        value: Value,
    ) -> Result<(), GraphError> {
        let mut record = self
            .read_edge_record(edge_id)?
            .ok_or(GraphError::EdgeNotFound(edge_id))?;

        let old_ptr = record.prop_page_id;
        let mut props = self.read_edge_properties(old_ptr)?;
        props.insert(key, value);

        let new_ptr = self.write_edge_properties(&props)?;
        record.prop_page_id = new_ptr;

        // 先写新记录再释放旧记录，避免中间态丢数据
        if !crate::page::is_none_ptr(old_ptr) && old_ptr != new_ptr {
            self.free_edge_properties(old_ptr)?;
        }

        self.write_edge_record(edge_id, &record)?;
        Ok(())
    }

    /// 查询节点邻居
    pub fn neighbors(&self, node_id: u64, direction: Direction) -> Result<Vec<u64>, GraphError> {
        let node_record = self
            .read_node_record(node_id)?
            .ok_or(GraphError::NodeNotFound(node_id))?;

        let mut neighbors = Vec::new();

        if direction == Direction::Outgoing || direction == Direction::Both {
            let mut curr = node_record.first_outgoing_edge_id;
            let mut seen = HashSet::new();
            while curr != 0 && seen.insert(curr) {
                if let Some(edge) = self.read_edge_record(curr)? {
                    neighbors.push(edge.dst_id);
                    curr = edge.src_next_edge_id;
                } else {
                    break;
                }
            }
        }

        if direction == Direction::Incoming || direction == Direction::Both {
            let mut curr = node_record.first_incoming_edge_id;
            let mut seen = HashSet::new();
            while curr != 0 && seen.insert(curr) {
                if let Some(edge) = self.read_edge_record(curr)? {
                    neighbors.push(edge.src_id);
                    curr = edge.dst_next_edge_id;
                } else {
                    break;
                }
            }
        }

        Ok(neighbors)
    }

    /// 获取所有出边实体
    pub fn outgoing_edges(&self, node_id: u64) -> Result<Vec<Edge>, GraphError> {
        let node_record = self
            .read_node_record(node_id)?
            .ok_or(GraphError::NodeNotFound(node_id))?;

        let mut edges = Vec::new();
        let mut curr = node_record.first_outgoing_edge_id;
        let mut seen = HashSet::new();
        while curr != 0 && seen.insert(curr) {
            if let Some(edge) = self.get_edge(curr)? {
                edges.push(edge);
                if let Some(rec) = self.read_edge_record(curr)? {
                    curr = rec.src_next_edge_id;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        Ok(edges)
    }

    /// 获取所有入边实体
    pub fn incoming_edges(&self, node_id: u64) -> Result<Vec<Edge>, GraphError> {
        let node_record = self
            .read_node_record(node_id)?
            .ok_or(GraphError::NodeNotFound(node_id))?;

        let mut edges = Vec::new();
        let mut curr = node_record.first_incoming_edge_id;
        let mut seen = HashSet::new();
        while curr != 0 && seen.insert(curr) {
            if let Some(edge) = self.get_edge(curr)? {
                edges.push(edge);
                if let Some(rec) = self.read_edge_record(curr)? {
                    curr = rec.dst_next_edge_id;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        Ok(edges)
    }

    /// 获取全图所有活跃有效节点 ID
    pub fn all_node_ids(&self) -> Result<Vec<u64>, GraphError> {
        let mut node_ids = Vec::with_capacity(self.node_count);
        if self.node_count == 0 {
            return Ok(node_ids);
        }

        let mut bpm = self.bpm.lock_recover();

        // 1. 扫描 Page 0 直属节点页
        let direct_pages = self.allocator.lock_recover().direct_node_pages;
        for (page_idx, &pid) in direct_pages.iter().enumerate() {
            if pid == 0 || pid == INVALID_PAGE_ID {
                continue;
            }
            if let Ok(fid) = bpm.fetch_page(pid) {
                let frame = bpm.get_frame(fid);
                for slot in 0..NODE_RECORDS_PER_PAGE {
                    let offset = slot * NodeRecord::RECORD_SIZE;
                    if frame.data[offset] == 1 {
                        let node_id = (page_idx * NODE_RECORDS_PER_PAGE + slot + 1) as u64;
                        node_ids.push(node_id);
                        if node_ids.len() >= self.node_count {
                            bpm.unpin_page(pid, false);
                            return Ok(node_ids);
                        }
                    }
                }
                bpm.unpin_page(pid, false);
            }
        }

        // 2. 扫描间接目录页
        let mut current_dir = self.allocator.lock_recover().node_dir_page_id;
        let mut dir_order = 0;

        while current_dir != INVALID_PAGE_ID && current_dir != 0 {
            let fid = bpm.fetch_page(current_dir)?;
            let frame = bpm.get_frame(fid);
            let next_dir = DirectoryPage::get_next_dir(&frame.data);

            let mut page_ids = Vec::with_capacity(DIR_ENTRIES_PER_PAGE);
            for entry_idx in 0..DIR_ENTRIES_PER_PAGE {
                let pid = DirectoryPage::get_entry(&frame.data, entry_idx);
                page_ids.push(pid);
            }
            bpm.unpin_page(current_dir, false);

            for (entry_idx, pid) in page_ids.into_iter().enumerate() {
                if pid == 0 || pid == INVALID_PAGE_ID {
                    continue;
                }
                if let Ok(data_fid) = bpm.fetch_page(pid) {
                    let data_frame = bpm.get_frame(data_fid);
                    for slot in 0..NODE_RECORDS_PER_PAGE {
                        let offset = slot * NodeRecord::RECORD_SIZE;
                        if data_frame.data[offset] == 1 {
                            let logical_page = HeaderPage::DIRECT_NODE_PAGES_COUNT
                                + dir_order * DIR_ENTRIES_PER_PAGE
                                + entry_idx;
                            let node_id = (logical_page * NODE_RECORDS_PER_PAGE + slot + 1) as u64;
                            node_ids.push(node_id);
                            if node_ids.len() >= self.node_count {
                                bpm.unpin_page(pid, false);
                                return Ok(node_ids);
                            }
                        }
                    }
                    bpm.unpin_page(pid, false);
                }
            }

            current_dir = next_dir;
            dir_order += 1;
        }

        Ok(node_ids)
    }

    /// 刷盘：确保 Buffer Pool 所有已提交脏页写入物理文件
    pub fn flush(&self) -> Result<(), GraphError> {
        let mut bpm = self.bpm.lock_recover();
        bpm.flush_all_pages()
    }

    /// 采集轻量元数据快照（O(1) 标量 + 字典/目录，不含图拓扑）
    pub fn snapshot_meta(&self) -> GraphMetaSnapshot {
        let mut allocator = self.allocator.lock_recover().clone();
        allocator.node_page_cache.clear();
        allocator.edge_page_cache.clear();
        GraphMetaSnapshot {
            next_node_id: self.next_node_id,
            next_edge_id: self.next_edge_id,
            node_count: self.node_count,
            edge_count: self.edge_count,
            first_free_node_id: self.first_free_node_id,
            first_free_edge_id: self.first_free_edge_id,
            dict: self.dict.clone(),
            dict_dirty: self.dict_dirty,
            dict_page_id: self.dict_page_id,
            index_catalog_page_id: self.index_catalog_page_id,
            index_catalog: self.index_catalog.clone(),
            allocator,
        }
    }

    /// 恢复元数据快照（事务失败回滚路径），同时清空物理页间接缓存
    pub fn restore_meta(&mut self, snapshot: &GraphMetaSnapshot) {
        self.next_node_id = snapshot.next_node_id;
        self.next_edge_id = snapshot.next_edge_id;
        self.node_count = snapshot.node_count;
        self.edge_count = snapshot.edge_count;
        self.first_free_node_id = snapshot.first_free_node_id;
        self.first_free_edge_id = snapshot.first_free_edge_id;
        self.dict = snapshot.dict.clone();
        self.dict_dirty = snapshot.dict_dirty;
        self.dict_page_id = snapshot.dict_page_id;
        self.index_catalog_page_id = snapshot.index_catalog_page_id;
        self.index_catalog = snapshot.index_catalog.clone();
        *self.allocator.lock_recover() = snapshot.allocator.clone();

        // 目录页在事务内可能被新建，回滚后旧目录才是权威，重新同步置换豁免集，
        // 避免残留的失效目录页或漏保护的新目录页影响后续寻址。
        self.sync_protected_pages();
    }

    /// 重新同步置换豁免页集合：Page 0 + 当前权威的节点/边页目录页
    pub fn sync_protected_pages(&self) {
        let mut bpm = self.bpm.lock_recover();
        bpm.clear_protected_pages();
        bpm.protect_page(HEADER_PAGE_ID);
        let alloc = self.allocator.lock_recover();
        if alloc.node_dir_page_id != INVALID_PAGE_ID && alloc.node_dir_page_id != 0 {
            bpm.protect_page(alloc.node_dir_page_id);
        }
        if alloc.edge_dir_page_id != INVALID_PAGE_ID && alloc.edge_dir_page_id != 0 {
            bpm.protect_page(alloc.edge_dir_page_id);
        }
    }

    /// 取出并清空本事务修改过的物理页集合
    pub fn drain_modified_pages(&self) -> Vec<PageId> {
        if let Ok(mut set) = self.tx_modified_pages.lock() {
            set.drain().collect()
        } else {
            Vec::new()
        }
    }
}
