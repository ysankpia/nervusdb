use crate::buffer::BufferPoolManager;
use crate::graph::{Direction, Edge, GraphError, Node, Value};
use crate::page::{
    DirectoryPage, EdgeRecord, HeaderPage, NodeRecord, PageId, PropertyPage, SlottedPropPage,
    DIR_ENTRIES_PER_PAGE, EDGE_RECORDS_PER_PAGE, INVALID_PAGE_ID, NODE_RECORDS_PER_PAGE,
    SLOT_OVERFLOW,
};
use crate::sync_ext::MutexRecoverExt;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

/// Header 物理页编号
pub const HEADER_PAGE_ID: PageId = 0;

/// 节点复合持久化载荷（标签集合与动态属性字典完整物理存储）
#[derive(Debug, Clone)]
pub struct NodeData {
    pub labels: HashSet<String>,
    pub properties: HashMap<String, Value>,
}

/// 批量织网的单条边插入请求
///
/// ## `edge_id` 由引擎分配，不是调用方的输入
///
/// [`Transaction::add_edges`](crate::Transaction::add_edges) 会**忽略**这个字段：
/// 它一次性预留 N 个 ID 再逐条填充，返回的 `Vec<u64>` 才是真正落库的 ID。所以
/// 用 `EdgeInsert { edge_id: 999_000, .. }` 传入并不会创建 ID 为 999000 的边——
/// 实测传 999000 得到的是 1，而 `get_edge(999_000)` 返回 `None`。
///
/// 这个字段之所以存在，是因为同一个结构体也用作**提交路径内部**的载体，那里
/// 它承载的正是已分配好的 ID（见 `Transaction::commit_internal`）。两条用途
/// 共用一个类型，于是公开这一侧出现了一个无法生效的字段。
///
/// **要构造公开输入，用 [`EdgeInsert::new`]**，它不给你误设的机会。直接写字面量
/// 仍然可以编译，但 `edge_id` 写什么都不会生效。
#[derive(Debug, Clone)]
pub struct EdgeInsert {
    /// 落库 ID。**由引擎分配**；作为 `add_edges` 的输入时被忽略。
    pub edge_id: u64,
    pub src_id: u64,
    pub dst_id: u64,
    pub edge_type: String,
    pub properties: HashMap<String, Value>,
    pub weight: f64,
}

impl EdgeInsert {
    /// 构造一条待插入的边，**不含** `edge_id`：那个值由引擎分配。
    ///
    /// 供 [`Transaction::add_edges`](crate::Transaction::add_edges) 的调用方使用，
    /// 使「ID 由引擎分配」这件事在类型层面就成立，而不是靠调用方读到文档才发现。
    pub fn new(
        src_id: u64,
        dst_id: u64,
        edge_type: impl Into<String>,
        properties: HashMap<String, Value>,
        weight: f64,
    ) -> Self {
        Self {
            edge_id: 0, // 占位；add_edges 会忽略并在提交时填入真实值
            src_id,
            dst_id,
            edge_type: edge_type.into(),
            properties,
            weight,
        }
    }
}

/// 批量织网的写放大阈值：段长达到该规模才走两阶段批量路径。
///
/// 小批量（如单条边的自动提交）逐条头插的开销可忽略，且能避免为几条边
/// 建立哈希表；只有大批量才值得换取「节点页每页只触碰一次」的收益。
pub const EDGE_BATCH_WEAVE_MIN: usize = 64;

/// 单次批量织网的边数上限。
///
/// 织网会在内存中为整段边构建若干张哈希表（`src_seq` / `dst_seq` / `meta` /
/// `dst_pos`）与两个记录向量，规模都是 O(段长)。一次提交上千万条边时，
/// 这个常驻内存会远超「内存占用由缓冲池决定」的架构承诺
/// （AGENTS.md §1 有界内存）。因此把长段切成子段逐段织网，
/// 使峰值内存由该常量而非用户输入决定。
///
/// 切分子段**不改变图结构**：链指针的推导只依赖「同批次内该节点的边序列」，
/// 而子段按同一顺序依次施加，后一子段的「批次前旧链头」正是前一子段写入的
/// 链头，拼起来与一次织完整段完全等价。边的 id 在 `add_edge` 时就已单调分配，
/// 切分不会重排 id。等价性由 `tests/edge_locality_tests.rs` 逐项对照。
pub const MAX_BATCH_EDGES_IN_MEMORY: usize = 100_000;

/// 字符串字典管理器：将 Label 和 EdgeType 映射为 u32 ID
#[derive(Debug, Clone, Default)]
pub struct StringDict {
    str_to_id: HashMap<String, u32>,
    id_to_str: HashMap<u32, String>,
    next_id: u32,
}

impl StringDict {
    /// 编码为磁盘载荷。布局见 `FORMAT.md`：
    ///
    /// ```text
    /// next_id:u32
    /// count:u32
    /// count × ( id:u32, str:len:u32 + UTF-8 bytes )
    /// ```
    ///
    /// 只写 `id_to_str` 一侧，`str_to_id` 在解码时重建：两者是同一映射的正反两面，
    /// 写两份会白占空间，也让两个副本有机会不一致。
    pub fn encode(&self) -> Vec<u8> {
        let mut w = crate::codec::Writer::with_capacity(self.id_to_str.len() * 24 + 8);
        w.u32(self.next_id);
        w.u32(self.id_to_str.len() as u32);
        // 排序保证同一字典的编码字节稳定，便于比对与调试
        let mut entries: Vec<(&u32, &String)> = self.id_to_str.iter().collect();
        entries.sort_unstable_by_key(|(id, _)| **id);
        for (id, s) in entries {
            w.u32(*id);
            w.string(s);
        }
        w.finish()
    }

    /// 从磁盘载荷解码。畸形输入返回 `Err`，调用方保留现有字典（不半途污染）。
    pub fn decode(buf: &[u8]) -> Result<StringDict, GraphError> {
        let mut r = crate::codec::Reader::new(buf);
        let next_id = r.u32()?;
        let count = r.u32()? as usize;
        // 每条至少 4(id)+4(长度) = 8 字节，据此拒绝荒谬的 count，
        // 避免用损坏的计数预分配巨量内存
        if count.saturating_mul(8) > r.remaining() {
            return Err(GraphError::SerializationError(format!(
                "dictionary claims {} entries but only {} byte(s) remain",
                count,
                r.remaining()
            )));
        }

        let mut str_to_id = HashMap::with_capacity(count);
        let mut id_to_str = HashMap::with_capacity(count);
        for _ in 0..count {
            let id = r.u32()?;
            let s = r.string()?;
            str_to_id.insert(s.clone(), id);
            id_to_str.insert(id, s);
        }
        if !r.is_exhausted() {
            return Err(GraphError::SerializationError(format!(
                "dictionary has {} trailing byte(s)",
                r.remaining()
            )));
        }

        Ok(StringDict {
            str_to_id,
            id_to_str,
            next_id,
        })
    }
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

/// 分配器的可回收空间统计（由 [`DiskGraph::allocator_snapshot`] 产出）。
///
/// 独立于 `AllocatorMeta`：元数据是内部状态（含页号与缓存），而这是一份面向
/// 调用方的**只读报告**，刻意不含任何页号。
#[derive(Debug, Clone, Default)]
pub struct AllocatorStats {
    /// 已分配页总数（文件高水位）
    pub allocated_pages: PageId,
    /// 整页可复用的槽位属性页数
    pub free_property_pages: usize,
    /// 整页可复用的溢出页数
    pub free_overflow_pages: usize,
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
    /// 页校验和目录（L1 链头）；`INVALID_PAGE_ID` 表示尚未建立
    pub crc_dir_page_id: PageId,
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
                crc_dir_page_id: INVALID_PAGE_ID,
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
        let magic_ok = magic == crate::page::DB_PAGE_MAGIC
            || crate::page::DB_PAGE_MAGIC_LEGACY
                .iter()
                .any(|m| m.as_slice() == magic);
        if magic_ok {
            // 版本守卫**不在这里**：`NervusDb::open` 已在任何写入（含 WAL 回放）
            // 之前用 `check_format_version` 挡下不匹配的文件（见 lib.rs）。
            //
            // 若在此处再检查一次，就太晚了——回放已经用当前版本的语义解释并写回
            // 了旧格式的文件，此时报错只会留下一个被污染的库。
            // 这里保留一处断言，防止有人绕过 `open` 直接构造 `DiskGraph`。
            //
            // 本块内（含下面的断言）每一处 `try_into().unwrap()` 都从**定长**
            // `frame.data` (`[u8; PAGE_SIZE]`) 上取固定字段，切片的起止都由
            // `HeaderPage::*_OFFSET` 常量决定、与目标整数宽度对齐，因此
            // `try_into()` 不可能失败（AGENTS.md §13）。写 `unwrap` 而非 `?` 是
            // 刻意的：它让「偏移常量与字段宽度不一致」在编译期/测试期立刻暴露。
            debug_assert!(
                {
                    let v = u32::from_le_bytes(
                        frame.data[HeaderPage::VERSION_OFFSET..HeaderPage::VERSION_OFFSET + 4]
                            .try_into()
                            .unwrap(),
                    );
                    v == crate::page::DB_PAGE_VERSION
                },
                "DiskGraph constructed on a file whose format version was not validated"
            );

            // 全部字段由 `HeaderFields::decode` 解析——布局定义在 `page.rs`，
            // 与 `HeaderPage` 的偏移常量同处一个文件（见 #20）。
            let fields = crate::page::HeaderFields::decode(&frame.data);

            self.next_node_id = fields.next_node_id;
            self.next_edge_id = fields.next_edge_id;
            self.node_count = fields.node_count as usize;
            self.edge_count = fields.edge_count as usize;
            self.first_free_node_id = fields.node_freelist;
            self.first_free_edge_id = fields.edge_freelist;
            self.dict_page_id = fields.dict_page_id;
            self.index_catalog_page_id = fields.index_catalog_page_id;

            let allocated_pages = fields.total_pages;
            let free_page = fields.page_freelist;
            let free_prop_page = fields.prop_freelist;
            let last_prop_page = fields.last_prop_page_id;
            let crc_dir_page = fields.crc_dir_page_id;
            let node_dir = fields.node_dir_page_id;
            let edge_dir = fields.edge_dir_page_id;
            let free_overflow = fields.overflow_freelist;
            let inline_dict_len = fields.inline_dict_len as usize;
            let inline_cat_len = fields.inline_catalog_len as usize;
            let direct_node_pages = fields.direct_node_pages;
            let direct_edge_pages = fields.direct_edge_pages;

            {
                let mut alloc = self.allocator.lock_recover();
                alloc.allocated_pages = allocated_pages.max(1);
                alloc.node_dir_page_id = node_dir;
                alloc.edge_dir_page_id = edge_dir;
                alloc.first_free_overflow_page = free_overflow;
                alloc.first_free_page_id = free_page;
                alloc.first_free_prop_page = free_prop_page;
                alloc.last_prop_page_id = last_prop_page;
                alloc.crc_dir_page_id = crc_dir_page;
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

            // 元数据解码失败必须**报错**，不能 `if let Ok(..)` 吞掉。
            //
            // 修复前这四处都是静默忽略：字典一旦解码不了，`self.dict` 就保持为空，
            // 于是重开后标签与边类型全部消失，而调用方只看到「图模式是空的」——
            // 数据损坏被伪装成「这个库本来就没标签」。这类静默降级正是
            // AGENTS.md §12 禁止的读错误隐藏。
            if self.dict_page_id != INVALID_PAGE_ID && self.dict_page_id != 0 {
                let payload = Self::read_overflow_payload_internal(&mut bpm, self.dict_page_id)?;
                self.dict = StringDict::decode(&payload).map_err(|e| {
                    GraphError::StorageError(format!(
                        "dictionary is unreadable (page {}): {}",
                        self.dict_page_id, e
                    ))
                })?;
            } else if inline_dict_len > 0 && inline_dict_len <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE
            {
                let dict_slice = &header_bytes[HeaderPage::INLINE_PAYLOAD_OFFSET
                    ..HeaderPage::INLINE_PAYLOAD_OFFSET + inline_dict_len];
                self.dict = StringDict::decode(dict_slice).map_err(|e| {
                    GraphError::StorageError(format!("inline dictionary is unreadable: {}", e))
                })?;
            }

            if self.index_catalog_page_id != INVALID_PAGE_ID && self.index_catalog_page_id != 0 {
                let payload =
                    Self::read_overflow_payload_internal(&mut bpm, self.index_catalog_page_id)?;
                self.index_catalog = crate::index::IndexCatalog::decode(&payload).map_err(|e| {
                    GraphError::StorageError(format!(
                        "index catalog is unreadable (page {}): {}",
                        self.index_catalog_page_id, e
                    ))
                })?;
            } else if inline_cat_len > 0
                && inline_dict_len + inline_cat_len <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE
            {
                let cat_start = HeaderPage::INLINE_PAYLOAD_OFFSET + inline_dict_len;
                let cat_slice = &header_bytes[cat_start..cat_start + inline_cat_len];
                self.index_catalog =
                    crate::index::IndexCatalog::decode(cat_slice).map_err(|e| {
                        GraphError::StorageError(format!(
                            "inline index catalog is unreadable: {}",
                            e
                        ))
                    })?;
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
            // 固定偏移 `0..4`，源为定长 `[u8; PAGE_SIZE]`，不可能失败（§13）。
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
            // 固定偏移 `0..4`，源为定长 `[u8; PAGE_SIZE]`，不可能失败（§13）。
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
                    // 固定偏移 `0..4`，源为定长 `[u8; PAGE_SIZE]`，不可能失败（§13）。
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
            return crate::page::pack_prop_ptr(root, SLOT_OVERFLOW);
        }

        let pid = Self::locate_prop_page_for(bpm, allocator, tx_modified, hint, payload)?;
        match Self::try_insert_into_prop_page(bpm, tx_modified, pid, payload)? {
            Some(slot) => crate::page::pack_prop_ptr(pid, slot),
            None => {
                // 定位阶段的判断与实际插入之间存在竞争（同页并发写），退化为新页
                let pid = Self::raw_allocate_prop_page(bpm, allocator, tx_modified)?;
                Self::push_prop_hint(hint, pid);
                match Self::try_insert_into_prop_page(bpm, tx_modified, pid, payload)? {
                    Some(slot) => crate::page::pack_prop_ptr(pid, slot),
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
            // 用 `lock_recover` 而非 `.lock().unwrap()`：AGENTS.md §13 禁止后者。
            // 这里只缓存一个页号提示，中毒后继续使用是安全的——丢了它只是少一次
            // 缓存命中，不影响正确性；而 panic 会终止整个进程。
            allocator
                .lock_recover()
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
            // 同上：这是可重建的缓存提示，中毒恢复优于进程终止
            allocator
                .lock_recover()
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

    /// 把一段元数据（字典或索引目录）写入溢出页链，按 `MAX_PAYLOAD` 分块。
    ///
    /// ## 为什么要分块成链
    ///
    /// 元数据（`StringDict`、`IndexCatalog`）会随图模式增长：每个标签、每种边类型、
    /// 每个被索引的属性都占一条。单页只能装 4088 字节——约 140 个标签——而
    /// `PropertyPage::encode` 对超长载荷是**静默截断**的。
    ///
    /// 修复前正是「单页 + 截断」：超过约 80 个标签后，字典在写盘时被切掉一半，
    /// 重开时解码失败、错误又被静默吞掉，于是整个图模式消失。改为分块成链后，
    /// 容量不再有这一层上限。
    ///
    /// `is_dict = true` 写 `dict_page_id`，否则写 `index_catalog_page_id`。
    fn write_metadata_chain(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        existing_head: PageId,
        payload: &[u8],
    ) -> Result<PageId, GraphError> {
        let chunk_size = PropertyPage::MAX_PAYLOAD;
        let num_chunks = payload.len().div_ceil(chunk_size).max(1);

        // 复用既有头页，不足则继续从溢出空闲链取
        let mut pages: Vec<PageId> = Vec::with_capacity(num_chunks);
        if existing_head != INVALID_PAGE_ID && existing_head != 0 {
            pages.push(existing_head);
        }
        while pages.len() < num_chunks {
            pages.push(Self::raw_allocate_overflow_page(
                bpm,
                allocator,
                tx_modified,
            )?);
        }

        for (i, &pid) in pages.iter().enumerate() {
            let next_pid = pages.get(i + 1).copied().unwrap_or(INVALID_PAGE_ID);
            let start = i * chunk_size;
            let end = (start + chunk_size).min(payload.len());
            let encoded = PropertyPage::encode(next_pid, &payload[start..end]);
            let fid = bpm.fetch_page(pid)?;
            {
                let frame = bpm.get_frame_mut(fid);
                frame.data.copy_from_slice(&encoded);
            }
            bpm.unpin_page(pid, true);
            bpm.mark_page_uncommitted(pid);
            if let Ok(mut set) = tx_modified.lock() {
                set.insert(pid);
            }
        }

        Ok(pages[0])
    }

    /// 同步元数据到 Page 0 (包含直接页槽位与紧凑内联字典与索引目录)
    pub fn sync_header(&mut self) -> Result<(), GraphError> {
        let mut bpm = self.bpm.lock_recover();

        let dict_bytes = self.dict.encode();
        let cat_bytes = self.index_catalog.encode();

        let mut inline_dict_len: u32 = 0;
        let mut inline_cat_len: u32 = 0;

        if dict_bytes.len() + cat_bytes.len() <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE {
            inline_dict_len = dict_bytes.len() as u32;
            inline_cat_len = cat_bytes.len() as u32;
        } else {
            if dict_bytes.len() <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE / 2 {
                inline_dict_len = dict_bytes.len() as u32;
            } else {
                // 字典放不下内联区，改走**溢出页链**。
                //
                // 修复前这里只写单页：`PropertyPage::encode` 会把载荷静默截断到
                // `MAX_PAYLOAD`（4088 字节），而读取侧遇到截断数据会解码失败、
                // 又被 `if let Ok(..)` 吞掉——结果是字典整体退化为空。
                // 实测阈值：80 个标签（约 2.3KB）就会全部丢失，重开后 `.schema`
                // 与标签列表都是空的。
                //
                // 溢出链必须按 `MAX_PAYLOAD` 分块并串起来，与属性载荷走同一条路径。
                self.dict_page_id = Self::write_metadata_chain(
                    &mut bpm,
                    &self.allocator,
                    &self.tx_modified_pages,
                    self.dict_page_id,
                    &dict_bytes,
                )?;
            }

            if cat_bytes.len() + (inline_dict_len as usize) <= HeaderPage::MAX_INLINE_PAYLOAD_SIZE {
                inline_cat_len = cat_bytes.len() as u32;
            } else {
                // 与字典同理：索引目录也可能超过单页，必须分块成链
                self.index_catalog_page_id = Self::write_metadata_chain(
                    &mut bpm,
                    &self.allocator,
                    &self.tx_modified_pages,
                    self.index_catalog_page_id,
                    &cat_bytes,
                )?;
            }
        }

        self.dict_dirty = false;

        let alloc = self.allocator.lock_recover().clone();

        let frame_id = bpm.fetch_page(HEADER_PAGE_ID)?;
        let frame = bpm.get_frame_mut(frame_id);

        // 字节布局由 `HeaderFields::encode` 负责——它与 `HeaderPage` 的偏移常量
        // 同在 `page.rs`。这里只提供数值。见 #20。
        crate::page::HeaderFields {
            total_pages: alloc.allocated_pages,
            node_freelist: self.first_free_node_id,
            edge_freelist: self.first_free_edge_id,
            page_freelist: alloc.first_free_page_id,
            dict_page_id: self.dict_page_id,
            node_count: self.node_count as u64,
            edge_count: self.edge_count as u64,
            next_node_id: self.next_node_id,
            next_edge_id: self.next_edge_id,
            node_dir_page_id: alloc.node_dir_page_id,
            edge_dir_page_id: alloc.edge_dir_page_id,
            overflow_freelist: alloc.first_free_overflow_page,
            inline_dict_len,
            inline_catalog_len: inline_cat_len,
            index_catalog_page_id: self.index_catalog_page_id,
            direct_node_pages: alloc.direct_node_pages,
            direct_edge_pages: alloc.direct_edge_pages,
            prop_freelist: alloc.first_free_prop_page,
            last_prop_page_id: alloc.last_prop_page_id,
            crc_dir_page_id: alloc.crc_dir_page_id,
        }
        .encode(
            &mut frame.data,
            &dict_bytes[..inline_dict_len as usize],
            &cat_bytes[..inline_cat_len as usize],
        );

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
        let mut bpm = self.bpm.lock_recover();
        Self::read_node_record_locked(&mut bpm, &self.allocator, &self.tx_modified_pages, node_id)
    }

    /// 在**调用方已持有** `bpm` 锁的前提下读一条节点记录。
    ///
    /// 与 [`DiskGraph::read_edge_record_locked`] 同一目的：让一次点读的多个寻址步骤
    /// 共用一次加锁。语义与 `read_node_record_raw` 完全一致。
    fn read_node_record_locked(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        node_id: u64,
    ) -> Result<Option<NodeRecord>, GraphError> {
        if node_id == 0 {
            return Ok(None);
        }
        let logical_page = (node_id - 1) as usize / NODE_RECORDS_PER_PAGE;
        let offset = ((node_id - 1) as usize % NODE_RECORDS_PER_PAGE) * NodeRecord::RECORD_SIZE;

        let physical_page = match Self::get_or_allocate_node_page(
            bpm,
            allocator,
            tx_modified,
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
    /// 在**调用方已持有** `bpm` 锁的前提下读一条边记录。
    ///
    /// 抽出来的唯一目的是让链式遍历能在一次加锁内完成（见
    /// [`DiskGraph::collect_edge_chain_batched`]）。语义与
    /// [`DiskGraph::read_edge_record_raw`] 完全一致。
    fn read_edge_record_locked(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        edge_id: u64,
    ) -> Result<Option<EdgeRecord>, GraphError> {
        if edge_id == 0 {
            return Ok(None);
        }
        let logical_page = (edge_id - 1) as usize / EDGE_RECORDS_PER_PAGE;
        let offset = ((edge_id - 1) as usize % EDGE_RECORDS_PER_PAGE) * EdgeRecord::RECORD_SIZE;

        let physical_page = match Self::get_or_allocate_edge_page(
            bpm,
            allocator,
            tx_modified,
            logical_page,
            false,
        )? {
            Some(pid) => pid,
            None => return Ok(None),
        };

        let frame_id = bpm.fetch_page(physical_page)?;
        let mut bytes = [0u8; EdgeRecord::RECORD_SIZE];
        {
            let frame = bpm.get_frame(frame_id);
            bytes.copy_from_slice(&frame.data[offset..offset + EdgeRecord::RECORD_SIZE]);
        }
        bpm.unpin_page(physical_page, false);

        Ok(Some(EdgeRecord::from_bytes(&bytes)))
    }

    pub fn read_edge_record_raw(&self, edge_id: u64) -> Result<Option<EdgeRecord>, GraphError> {
        let mut bpm = self.bpm.lock_recover();
        Self::read_edge_record_locked(&mut bpm, &self.allocator, &self.tx_modified_pages, edge_id)
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
        Self::read_node_data_locked(&mut bpm, ptr)
    }

    /// 在**调用方已持有** `bpm` 锁的前提下读节点载荷。
    fn read_node_data_locked(
        bpm: &mut BufferPoolManager,
        ptr: u32,
    ) -> Result<NodeData, GraphError> {
        let payload = Self::read_prop_record(bpm, ptr)?;
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
        // `encode_props` 会因为 null/list 报错，必须向上传播：静默写入一个不含该键
        // 的载荷，会让「设置属性」看起来成功而数据其实没变。
        let payload = crate::page::encode_props(props)?;

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
        // 键排序保证同一份数据编码字节稳定
        let mut keys: Vec<&String> = data.properties.keys().collect();
        keys.sort_unstable();
        for key in keys {
            codec.push_key(key);
            // null/list 不可落盘，`push_value` 会返回错误，这里向上传播
            codec.push_value(&data.properties[key])?;
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
        // 同 `decode_props`：count 来自磁盘，先按剩余字节设界再分配，
        // 否则一个损坏的 varint 就能触发数 GB 的分配请求。
        if label_count > reader.remaining() {
            return Err(GraphError::SerializationError(
                "corrupted node label count (exceeds payload)".into(),
            ));
        }
        let mut labels = HashSet::with_capacity(label_count.min(reader.remaining()));
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
        if prop_count > reader.remaining() {
            return Err(GraphError::SerializationError(
                "corrupted node property count (exceeds payload)".into(),
            ));
        }
        let mut properties = HashMap::with_capacity(prop_count.min(reader.remaining()));
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

    /// 一次性分配 `count` 个节点 ID，顺序与单独调用 `count` 次
    /// [`Self::allocate_next_node_id`] 完全一致。
    ///
    /// ## 为什么需要批量版本
    ///
    /// 单条分配要求调用方持写锁，而批量的价值正在于**把 N 次加解锁压成 1 次**。
    /// 逐条分配时每次都要重新取 `GraphInner` 的写锁，这条路径上的锁竞争比实际
    /// 写入更贵——实测 Python SDK 逐条写入约 42k ops/s，而原生批量路径可达
    /// 数十万。
    ///
    /// 语义不变：仍优先消费 Freelist，用尽后才推进 `next_node_id`。返回顺序即
    /// 分配顺序，调用方可以据此与输入一一对应。
    pub fn allocate_next_node_ids(&mut self, count: usize) -> Result<Vec<u64>, GraphError> {
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(self.allocate_next_node_id()?);
        }
        Ok(ids)
    }

    /// 一次性分配 `count` 个边 ID。语义与 [`Self::allocate_next_node_ids`] 相同。
    pub fn allocate_next_edge_ids(&mut self, count: usize) -> Result<Vec<u64>, GraphError> {
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(self.allocate_next_edge_id()?);
        }
        Ok(ids)
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

    /// 获取完整 Node 结构体（按需通过 Buffer Pool 调入）。
    ///
    /// ## 一次加锁，而不是四次
    ///
    /// 这条路径由 `read_node_record` + `read_node_data` + 两条链遍历组成。若各自
    /// 持锁，一次 `get_node` 就是 **4 次**全局 `bpm` 加锁（链遍历已由
    /// `collect_edge_chain_batched` 从 O(度) 降到 O(1)）。4 次在单线程下无所谓，
    /// 在 16 线程下就是 4 倍的争用窗口。
    ///
    /// 本机实测（20 万节点/60 万边，读互不相交区间，命中率 98.7%，即瓶颈是锁而非
    /// 磁盘）：1 线程 779k ops/s，8 线程降到 195k——**负扩展**。把 4 次合并为 1 次
    /// 直接缩小每个读取在临界区里的停留次数。
    ///
    /// ## 这不等于让读者并行
    ///
    /// `bpm` 仍是**一把**全局锁：合并加锁只缩短临界区，两个读者依然互斥。真正的
    /// 并行需要按帧加锁（缓冲池并发模型重设计，见 AGENTS.md §10 与 `ROADMAP.md`）。
    /// 这里做到的是「把 4 次争用变成 1 次」，不是「没有争用」。
    pub fn get_node(&self, node_id: u64) -> Result<Option<Node>, GraphError> {
        // 单次持锁完成全部读取。四个步骤原本各自 `lock_recover()`，现在共用一次。
        let mut bpm = self.bpm.lock_recover();

        let record = match Self::read_node_record_locked(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            node_id,
        )? {
            Some(r) if r.in_use == 1 => r,
            _ => return Ok(None),
        };

        let node_data = Self::read_node_data_locked(&mut bpm, record.prop_page_id)?;
        let outgoing = Self::collect_edge_chain_locked(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            record.first_outgoing_edge_id,
            false,
        )?;
        let incoming = Self::collect_edge_chain_locked(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            record.first_incoming_edge_id,
            true,
        )?;

        let mut node = Node::new(node_id, node_data.labels, node_data.properties);
        node.outgoing = outgoing;
        node.incoming = incoming;
        Ok(Some(node))
    }

    /// 沿磁盘出边链遍历收集出边 ID (Index-Free Adjacency)
    /// 一次加锁走完整条边链（出边或入边），返回边 ID 列表。
    ///
    /// ## 为什么需要它
    ///
    /// [`DiskGraph::read_edge_record_raw`] 每读一条边就取一次全局 `bpm` 锁。
    /// 于是一次 `get_node` 的加锁次数是 `3 + 出度 + 入度`——实测 com-DBLP 上
    /// 度为 343 的枢纽约 **345 次**。这个数字本身就是问题：它把只读并发压成了
    /// **负扩展**（16 线程吞吐降到单线程的 0.6%–1.4%，见
    /// `docs/benchmarks.md#concurrency-scaling`）。
    ///
    /// 本函数把整条链放进**一次**加锁内完成，加锁次数从 O(度) 降到 O(1)。
    ///
    /// ## 为什么这样是安全的
    ///
    /// 只读路径（`is_write = false`）不修改任何持久状态：
    /// `get_or_allocate_edge_page` 在只读时唯一副作用是写 `edge_page_cache`，
    /// 而它是**可重建的页号提示缓存**（注释原文：「中毒恢复优于进程终止」），
    /// 重复插入同一个键值是幂等的。链长度本身有 `seen` 环检测守卫，与原实现一致。
    ///
    /// ## 它不做什么
    ///
    /// **这不等于让读者并行。** `bpm` 仍是单把全局锁，多线程仍会相互排队，
    /// 只是每次调用占用的锁时间变短。真正并行需要按帧加锁（见 AGENTS.md §10）。
    ///
    /// ## 关于下面两个守卫的可达性（实测的诚实说明）
    ///
    /// `record.in_use == 1` 过滤与 `seen` 环检测是**纵深防御，当前不可达**：
    /// `remove_edge` / `remove_node` 在标记 `in_use = 0` 的同时会把记录从链上摘除，
    /// 因此正常维护下链上不会出现已删记录，也不会出现环（链是单向的）。
    ///
    /// 这一点是实测出来的，不是推断：把这两个守卫分别改成恒真后重跑测试，
    /// 两者都**没有**失败。保留它们是因为链内容可能被并发写者或崩溃恢复后的
    /// 部分状态影响，而那时遍历会退化为死循环（比报错严重得多）。但不要声称
    /// 测试覆盖了它们——这里如实记录其不可达性。
    fn collect_edge_chain_batched(
        &self,
        first_edge_id: u64,
        incoming: bool,
    ) -> Result<Vec<u64>, GraphError> {
        // 锁的持有顺序与既有实现一致（先 bpm，后 allocator），不引入新的锁序。
        let mut bpm = self.bpm.lock_recover();
        Self::collect_edge_chain_locked(
            &mut bpm,
            &self.allocator,
            &self.tx_modified_pages,
            first_edge_id,
            incoming,
        )
    }

    /// 在**调用方已持有** `bpm` 锁的前提下遍历整条边链。
    ///
    /// 抽出来的原因与 `read_edge_record_locked` 相同：让 `get_node` 的四个读取步骤
    /// 共用一次加锁（见 [`DiskGraph::get_node`]）。
    fn collect_edge_chain_locked(
        bpm: &mut BufferPoolManager,
        allocator: &Arc<Mutex<AllocatorMeta>>,
        tx_modified: &Arc<Mutex<HashSet<PageId>>>,
        first_edge_id: u64,
        incoming: bool,
    ) -> Result<Vec<u64>, GraphError> {
        let mut ids = Vec::new();
        let mut curr = first_edge_id;
        let mut seen = HashSet::new();

        while curr != 0 && seen.insert(curr) {
            match Self::read_edge_record_locked(bpm, allocator, tx_modified, curr)? {
                // 与 `read_edge_record` 一致：只接受 in_use 的记录
                Some(record) if record.in_use == 1 => {
                    ids.push(curr);
                    curr = if incoming {
                        record.dst_next_edge_id
                    } else {
                        record.src_next_edge_id
                    };
                }
                _ => break,
            }
        }
        Ok(ids)
    }

    /// 沿磁盘出边链遍历收集出边 ID (Index-Free Adjacency)
    pub fn collect_outgoing_edge_ids(&self, first_edge_id: u64) -> Result<Vec<u64>, GraphError> {
        self.collect_edge_chain_batched(first_edge_id, false)
    }

    /// 沿磁盘入边链遍历收集入边 ID
    pub fn collect_incoming_edge_ids(&self, first_edge_id: u64) -> Result<Vec<u64>, GraphError> {
        self.collect_edge_chain_batched(first_edge_id, true)
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
        // 无属性的边必须写 `PROP_PTR_NONE`（0），**不能写 `INVALID_PAGE_ID`**。
        //
        // 两者数值相同（都是 `u32::MAX`），而 `u32::MAX` 正是
        // `PROP_PTR_OVERFLOW` 哨兵——含义是「属性位于根页 0x00FFFFFF 的溢出链」。
        // 于是删除这样一条边时，`free_edge_properties` 会把一个**并不存在的页
        // 16777215** 当作溢出链回收：它进入溢出空闲链并被标记为脏，随之写进 WAL、
        // 在 checkpoint 时落到主文件的 68,719,472,640 偏移处。
        //
        // 实测后果：删掉一条无属性边，数据库文件从 12KB 变成 **64 GiB**（正好顶到
        // 格式上限），而 `backup()` / `vacuum()` 会把这个体积一并复制出去。
        //
        // 批量织网路径（`insert_edges_batch`）一直用的是正确的 `PROP_PTR_NONE`，
        // 只有这里写错，因此这是单条插入路径独有的缺陷。
        let prop_page_id = if !properties.is_empty() {
            self.write_edge_properties(&properties)?
        } else {
            crate::page::PROP_PTR_NONE
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
            // 环检测守卫：链指针损坏或并发改写可能形成环，无守卫会死循环。
            // AGENTS.md §10 要求所有指针遍历都带 `seen` 集合，此处曾遗漏。
            let mut seen = HashSet::new();
            while curr != 0 && seen.insert(curr) {
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
            // 读取失败必须向上传播，不能 `if let Ok(..)` 跳过：
            // 页 CRC 不匹配时静默跳过会让存活节点数无声减少，调用方看到的是
            // 「图变小了」而不是「某页坏了」——这正是 AGENTS.md §12 禁止的
            // 静默读错误。
            let fid = bpm.fetch_page(pid)?;
            {
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
            }
            bpm.unpin_page(pid, false);
        }

        // 2. 扫描间接目录页
        let mut current_dir = self.allocator.lock_recover().node_dir_page_id;
        let mut dir_order = 0;
        // 环检测守卫：目录链损坏时（`next_dir` 指回上游或自身）无守卫会死循环。
        // AGENTS.md §10 要求所有指针遍历都带 `seen`，此处曾遗漏——同文件的
        // `walk_free_chain` 就同时带 `seen` 与步数上限，属不一致的疏漏。
        let mut seen_dirs = HashSet::new();

        while current_dir != INVALID_PAGE_ID && current_dir != 0 && seen_dirs.insert(current_dir) {
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
                // 同上：读取失败向上传播，不做静默跳过
                let data_fid = bpm.fetch_page(pid)?;
                {
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
                }
                bpm.unpin_page(pid, false);
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

    /// 慢照分配器的可回收空间：沿空闲链走一遍，数出**整页可复用**的属性页与
    /// 溢出页数量。
    ///
    /// 只统计页数，不返回页号列表——调用方（`vacuum` 报告）只需要规模，而返回
    /// 列表会让一个诊断 API 携带 O(空闲页数) 的分配。
    ///
    /// 两条链都以「页首 4 字节 = 下一页」串联（与 `get_free_page` 的读取方式一致），
    /// 因此走链过程中任何一次读取失败都视为链尾，不返回错误——这是一个只读诊断，
    /// 不应因为遇到半损坏的链就让整个调用失败。
    pub fn allocator_snapshot(&self) -> AllocatorStats {
        let (prop_head, over_head, allocated) = {
            let a = self.allocator.lock_recover();
            (
                a.first_free_prop_page,
                a.first_free_overflow_page,
                a.allocated_pages,
            )
        };

        AllocatorStats {
            allocated_pages: allocated,
            free_property_pages: Self::count_free_chain(self, prop_head),
            free_overflow_pages: Self::count_free_chain(self, over_head),
        }
    }

    /// 沿一条空闲页链数节点；上限 `MAX_FREE_CHAIN_WALK` 防环。
    fn count_free_chain(&self, head: PageId) -> usize {
        const MAX_FREE_CHAIN_WALK: usize = 1_000_000;
        let mut count = 0usize;
        let mut cur = head;
        let mut seen = std::collections::HashSet::new();

        while cur != 0 && cur != INVALID_PAGE_ID && count < MAX_FREE_CHAIN_WALK && seen.insert(cur)
        {
            let mut bpm = self.bpm.lock_recover();
            let next = match bpm.fetch_page(cur) {
                Ok(fid) => {
                    let frame = bpm.get_frame(fid);
                    let n = u32::from_le_bytes(frame.data[0..4].try_into().unwrap_or([0; 4]));
                    bpm.unpin_page(cur, false);
                    n
                }
                Err(_) => break,
            };
            count += 1;
            cur = next;
        }
        count
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

    /// 页校验和目录的根页号（Header 持久化字段的只读访问）
    pub fn crc_dir_root(&self) -> PageId {
        self.allocator.lock_recover().crc_dir_page_id
    }

    /// 直接从主文件读取并校验指定页的校验和（运维探针）。
    ///
    /// 与 `fetch_page` 不同：它**绕过缓冲池缓存**，因此验证的总是磁盘上的实际内容，
    /// 而不是可能已被修好的内存副本。用于「哪一页坏了」这类定点排查。
    pub fn verify_page_on_disk(&self, page_id: PageId) -> Result<(), GraphError> {
        let mut bpm = self.bpm.lock_recover();
        let mut data = [0u8; crate::page::PAGE_SIZE];
        bpm.disk_manager().read_page(page_id, &mut data)?;

        // 借出 CRC 存储做一次校验，再归还（它在缓冲池里，因为需要共用缓存）
        let mut crc = bpm.take_crc();
        let result = if let Some(store) = crc.as_mut() {
            store.verify(page_id, &data)
        } else {
            Ok(())
        };
        if let Some(store) = crc {
            bpm.attach_crc(store);
        }
        result
    }

    /// 更新页校验和目录根页号（首次建立目录后需写回 Header）
    pub fn set_crc_dir_root(&mut self, root: PageId) {
        self.allocator.lock_recover().crc_dir_page_id = root;
    }

    /// 对主文件中的**每一张**页做一次 CRC 校验，返回坏页清单 `(页号, 细节)`。
    ///
    /// 与逐个调用 [`DiskGraph::verify_page_on_disk`] 的区别：本函数只取一次锁、
    /// 只借出一次 CRC 存储，因此在百万页级的大库上仍是单次扫描而不是百万次加解锁。
    ///
    /// 未记录 CRC 的页（新分配、从未落盘、或目录页本身）直接跳过，
    /// 与 `CrcStore` 的「`0` = 未记录」语义一致。
    pub fn verify_all_pages_on_disk(&self) -> Result<Vec<(PageId, String)>, GraphError> {
        let mut bpm = self.bpm.lock_recover();
        let page_count = (bpm.disk_manager().file_size() / crate::page::PAGE_SIZE as u64) as PageId;

        let mut crc = bpm.take_crc();
        let mut bad = Vec::new();
        let mut data = [0u8; crate::page::PAGE_SIZE];

        let result = (|| -> Result<(), GraphError> {
            for pid in 1..page_count {
                bpm.disk_manager().read_page(pid, &mut data)?;
                let Some(store) = crc.as_mut() else {
                    break;
                };
                if let Err(GraphError::PageChecksumMismatch {
                    expected, actual, ..
                }) = store.verify(pid, &data)
                {
                    bad.push((
                        pid,
                        format!("expected {:#010x}, actual {:#010x}", expected, actual),
                    ));
                }
            }
            Ok(())
        })();

        if let Some(store) = crc {
            bpm.attach_crc(store);
        }
        result?;
        Ok(bad)
    }
}
