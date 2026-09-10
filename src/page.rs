use std::convert::TryInto;

/// 物理磁盘页大小：4096 字节 (4KB)
pub const PAGE_SIZE: usize = 4096;

/// 逻辑页编号
pub type PageId = u32;

/// 无效页 ID 哨兵常量
pub const INVALID_PAGE_ID: PageId = u32::MAX;

/// 每个页面中容纳的 NodeRecord 数量 (4096 / 32 = 128)
pub const NODE_RECORDS_PER_PAGE: usize = PAGE_SIZE / NodeRecord::RECORD_SIZE;

/// 每个页面中容纳的 EdgeRecord 数量 (4096 / 64 = 64)
pub const EDGE_RECORDS_PER_PAGE: usize = PAGE_SIZE / EdgeRecord::RECORD_SIZE;

/// 数据库物理文件头魔数 "GLDB" (GraphLite Database)
pub const DB_PAGE_MAGIC: &[u8; 4] = b"GLDB";
pub const DB_PAGE_MAGIC_LEGACY: &[u8; 4] = b"GLP4";
/// 物理存储格式版本。2 = Slotted Property Page 紧凑属性存储（1.1 起）；
/// 版本 1（每实体独占整张 4KB 属性页）不再支持，打开时返回明确错误。
pub const DB_PAGE_VERSION: u32 = 2;

/// Page 0 Header 物理页规范与偏移常量定义
pub struct HeaderPage;

impl HeaderPage {
    pub const MAGIC_OFFSET: usize = 0; // 4 bytes: b"GLDB"
    pub const VERSION_OFFSET: usize = 4; // 4 bytes: u32 (1)
    pub const PAGE_SIZE_OFFSET: usize = 8; // 4 bytes: u32 (4096)
    pub const TOTAL_PAGES_OFFSET: usize = 12; // 4 bytes: u32
    pub const NODE_FREELIST_OFFSET: usize = 16; // 8 bytes: u64
    pub const EDGE_FREELIST_OFFSET: usize = 24; // 8 bytes: u64
    pub const PAGE_FREELIST_OFFSET: usize = 32; // 4 bytes: u32
    pub const DICT_PAGE_OFFSET: usize = 36; // 4 bytes: u32
    pub const NODE_COUNT_OFFSET: usize = 40; // 8 bytes: u64
    pub const EDGE_COUNT_OFFSET: usize = 48; // 8 bytes: u64
    pub const NEXT_NODE_ID_OFFSET: usize = 56; // 8 bytes: u64
    pub const NEXT_EDGE_ID_OFFSET: usize = 64; // 8 bytes: u64
    pub const NODE_DIR_OFFSET: usize = 72; // 4 bytes: u32
    pub const EDGE_DIR_OFFSET: usize = 76; // 4 bytes: u32
    pub const OVERFLOW_FREELIST_OFFSET: usize = 80; // 4 bytes: u32
    pub const INLINE_DICT_LEN_OFFSET: usize = 84; // 4 bytes: u32
    pub const INLINE_CATALOG_LEN_OFFSET: usize = 88; // 4 bytes: u32
    pub const INDEX_CATALOG_PAGE_OFFSET: usize = 92; // 4 bytes: u32 (索引元数据溢出根页)
    pub const DIRECT_NODE_PAGES_OFFSET: usize = 96; // 32 * 4 = 128 bytes (覆盖前 4096 个节点无需分配间接目录页)
    pub const DIRECT_EDGE_PAGES_OFFSET: usize = 224; // 32 * 4 = 128 bytes (覆盖前 2048 条边无需分配间接目录页)
    pub const PROP_FREELIST_OFFSET: usize = 352; // 4 bytes: 已腾空的槽位属性页回收链
    pub const LAST_PROP_PAGE_OFFSET: usize = 356; // 4 bytes: 最近分配过的槽位属性页（写入位点提示）
    pub const INLINE_PAYLOAD_OFFSET: usize = 360; // 剩余 3736 字节用于内联字典与索引元数据紧凑存储
    pub const DIRECT_NODE_PAGES_COUNT: usize = 32;
    pub const DIRECT_EDGE_PAGES_COUNT: usize = 32;
    pub const MAX_INLINE_PAYLOAD_SIZE: usize = PAGE_SIZE - Self::INLINE_PAYLOAD_OFFSET;
}

/// 定长 32 字节 NodeRecord
/// 实现严格 O(1) 磁盘直接寻址
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRecord {
    pub in_use: u8,                  // 1 byte: 1 表示有效使用，0 表示已删除或未分配
    pub reserved: [u8; 3],           // 3 bytes: 内存对齐与扩展
    pub label_id: u32,               // 4 bytes: 字符串字典映射后的标签 ID (0 表示无标签)
    pub first_outgoing_edge_id: u64, // 8 bytes: 第一条出边 ID (0 表示无出边)
    pub first_incoming_edge_id: u64, // 8 bytes: 第一条入边 ID (0 表示无入边)
    /// 4 bytes: 属性指针（高 24 位 PageId + 低 8 位 SlotId；0=无属性，槽位 0xFF=溢出链）
    pub prop_page_id: u32,
    pub inline_prop_val: i32, // 4 bytes: 紧凑内联整数值或主属性辅助值
}

impl NodeRecord {
    pub const RECORD_SIZE: usize = 32;

    pub fn new_empty() -> Self {
        Self {
            in_use: 0,
            reserved: [0; 3],
            label_id: 0,
            first_outgoing_edge_id: 0,
            first_incoming_edge_id: 0,
            prop_page_id: PROP_PTR_NONE,
            inline_prop_val: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; Self::RECORD_SIZE] {
        let mut bytes = [0u8; Self::RECORD_SIZE];
        bytes[0] = self.in_use;
        bytes[1..4].copy_from_slice(&self.reserved);
        bytes[4..8].copy_from_slice(&self.label_id.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.first_outgoing_edge_id.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.first_incoming_edge_id.to_le_bytes());
        bytes[24..28].copy_from_slice(&self.prop_page_id.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.inline_prop_val.to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8; Self::RECORD_SIZE]) -> Self {
        Self {
            in_use: bytes[0],
            reserved: [bytes[1], bytes[2], bytes[3]],
            label_id: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            first_outgoing_edge_id: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            first_incoming_edge_id: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            prop_page_id: u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            inline_prop_val: i32::from_le_bytes(bytes[28..32].try_into().unwrap()),
        }
    }
}

/// 定长 64 字节 EdgeRecord
/// 形成磁盘原生双向双环免索引邻接链表
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeRecord {
    pub in_use: u8,        // 1 byte: 1 为使用，0 为删除
    pub reserved: [u8; 3], // 3 bytes
    pub edge_type_id: u32, // 4 bytes: 关系类型字典 ID
    /// 4 bytes: 边属性指针（高 24 位 PageId + 低 8 位 SlotId；0=无属性，槽位 0xFF=溢出链）
    pub prop_page_id: u32,
    pub reserved2: [u8; 4],    // 4 bytes: 内存对齐
    pub src_id: u64,           // 8 bytes: 源节点 ID
    pub dst_id: u64,           // 8 bytes: 目标节点 ID
    pub weight: f64,           // 8 bytes: 边权重
    pub src_prev_edge_id: u64, // 8 bytes: 源节点出边链前驱
    pub src_next_edge_id: u64, // 8 bytes: 源节点出边链后继
    pub dst_next_edge_id: u64, // 8 bytes: 目的节点入边链后继
}

impl EdgeRecord {
    pub const RECORD_SIZE: usize = 64;

    pub fn new_empty() -> Self {
        Self {
            in_use: 0,
            reserved: [0; 3],
            edge_type_id: 0,
            prop_page_id: PROP_PTR_NONE,
            reserved2: [0; 4],
            src_id: 0,
            dst_id: 0,
            weight: 1.0,
            src_prev_edge_id: 0,
            src_next_edge_id: 0,
            dst_next_edge_id: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; Self::RECORD_SIZE] {
        let mut bytes = [0u8; Self::RECORD_SIZE];
        bytes[0] = self.in_use;
        bytes[1..4].copy_from_slice(&self.reserved);
        bytes[4..8].copy_from_slice(&self.edge_type_id.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.prop_page_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.reserved2);
        bytes[16..24].copy_from_slice(&self.src_id.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.dst_id.to_le_bytes());
        bytes[32..40].copy_from_slice(&self.weight.to_le_bytes());
        bytes[40..48].copy_from_slice(&self.src_prev_edge_id.to_le_bytes());
        bytes[48..56].copy_from_slice(&self.src_next_edge_id.to_le_bytes());
        bytes[56..64].copy_from_slice(&self.dst_next_edge_id.to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8; Self::RECORD_SIZE]) -> Self {
        Self {
            in_use: bytes[0],
            reserved: [bytes[1], bytes[2], bytes[3]],
            edge_type_id: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            prop_page_id: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            reserved2: [bytes[12], bytes[13], bytes[14], bytes[15]],
            src_id: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            dst_id: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            weight: f64::from_le_bytes(bytes[32..40].try_into().unwrap()),
            src_prev_edge_id: u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
            src_next_edge_id: u64::from_le_bytes(bytes[48..56].try_into().unwrap()),
            dst_next_edge_id: u64::from_le_bytes(bytes[56..64].try_into().unwrap()),
        }
    }
}

/// 属性溢出页结构 (Property Overflow Page)
/// 当字符串或属性映射超出内联尺寸时，保存在溢出页链表中
pub struct PropertyPage;

impl PropertyPage {
    pub const HEADER_SIZE: usize = 8;
    pub const MAX_PAYLOAD: usize = PAGE_SIZE - Self::HEADER_SIZE;

    pub fn encode(next_page: PageId, payload: &[u8]) -> [u8; PAGE_SIZE] {
        let mut page = [0u8; PAGE_SIZE];
        page[0..4].copy_from_slice(&next_page.to_le_bytes());
        let len = (payload.len() as u32).min(Self::MAX_PAYLOAD as u32);
        page[4..8].copy_from_slice(&len.to_le_bytes());
        let end = 8 + len as usize;
        page[8..end].copy_from_slice(&payload[..len as usize]);
        page
    }

    pub fn decode(page: &[u8; PAGE_SIZE]) -> (PageId, Vec<u8>) {
        let next_page = u32::from_le_bytes(page[0..4].try_into().unwrap());
        let len = u32::from_le_bytes(page[4..8].try_into().unwrap()) as usize;
        let valid_len = len.min(Self::MAX_PAYLOAD);
        let payload = page[8..8 + valid_len].to_vec();
        (next_page, payload)
    }
}

/// 目录页每个页面可容纳的 PageId 条目数 (4096 - 8) / 4 = 1022
pub const DIR_ENTRIES_PER_PAGE: usize = (PAGE_SIZE - 8) / 4;
/// 物理页目录管理（支持动态平坦多级页表）
pub struct DirectoryPage;

impl DirectoryPage {
    pub fn get_next_dir(page: &[u8; PAGE_SIZE]) -> PageId {
        u32::from_le_bytes(page[0..4].try_into().unwrap())
    }

    pub fn set_next_dir(page: &mut [u8; PAGE_SIZE], next_page: PageId) {
        page[0..4].copy_from_slice(&next_page.to_le_bytes());
    }

    pub fn get_entry(page: &[u8; PAGE_SIZE], index: usize) -> PageId {
        assert!(index < DIR_ENTRIES_PER_PAGE);
        let offset = 8 + index * 4;
        u32::from_le_bytes(page[offset..offset + 4].try_into().unwrap())
    }

    pub fn set_entry(page: &mut [u8; PAGE_SIZE], index: usize, pid: PageId) {
        assert!(index < DIR_ENTRIES_PER_PAGE);
        let offset = 8 + index * 4;
        page[offset..offset + 4].copy_from_slice(&pid.to_le_bytes());
    }
}

// =========================================================================
// 属性指针编码：高 24 位 PageId + 低 8 位 SlotId
// =========================================================================

/// 无属性哨兵（与既有 `prop_page_id != INVALID_PAGE_ID && != 0` 判定完全兼容）
pub const PROP_PTR_NONE: u32 = 0;
/// 属性走 `PropertyPage` 溢出链的哨兵（原 `INVALID_PAGE_ID` 语义）
pub const PROP_PTR_OVERFLOW: u32 = u32::MAX;
/// 槽位号 0xFF 保留给「溢出链」语义，槽位页最多承载 255 条记录
pub const SLOT_OVERFLOW: u8 = 0xFF;
/// 槽位页最多可容纳的记录条数
pub const MAX_SLOTS_PER_PAGE: usize = SLOT_OVERFLOW as usize;
/// 单条属性记录的内联上限：超过 1KB 的行记录改走溢出页链
pub const INLINE_RECORD_MAX: usize = 1024;

/// 打包属性指针：PageId 占高 24 位，SlotId 占低 8 位
pub fn pack_prop_ptr(page_id: PageId, slot: u8) -> u32 {
    debug_assert!(
        page_id <= 0x00FF_FFFF,
        "PageId exceeds 24-bit address space"
    );
    ((page_id & 0x00FF_FFFF) << 8) | (slot as u32)
}

/// 解包属性指针为 (PageId, SlotId)
pub fn unpack_prop_ptr(ptr: u32) -> (PageId, u8) {
    ((ptr >> 8) & 0x00FF_FFFF, (ptr & 0xFF) as u8)
}

/// 该属性指针是否指向溢出页链
pub fn is_overflow_ptr(ptr: u32) -> bool {
    ptr != PROP_PTR_NONE && (ptr & 0xFF) == SLOT_OVERFLOW as u32
}

/// 该属性指针是否表示「无属性」
pub fn is_none_ptr(ptr: u32) -> bool {
    ptr == PROP_PTR_NONE
}

/// 单槽属性页 (Slotted Property Page)：单页内紧凑打包多条变长属性记录。
///
/// 布局：
/// ```text
/// +--------------------------------------------------------------+
/// | Header 24B | Slot Array (向下生长) | free | Payload (向上生长) |
/// +--------------------------------------------------------------+
/// ```
/// 每条记录由 4 字节槽位描述符 `[offset u16 | len u16]` 定位；`len == 0` 表示死槽。
/// 删除只标记死槽，空间不足时才原地压实，保证页内操作恒定 4KB 上界、无递归分配。
pub struct SlottedPropPage;

impl SlottedPropPage {
    /// 页魔数 "GLSP"。与 NodeRecord/EdgeRecord（首字节 in_use ∈ {0,1}）、
    /// PropertyPage（首 4 字节为 < 2^24 的页号）均不可能碰撞。
    pub const MAGIC: [u8; 4] = *b"GLSP";
    pub const VERSION: u16 = 1;

    /// magic(4) + version(2) + rec_count(2) + live_count(2) + slot_count(2) + free_start(2) + free_end(2) + reserved(8)
    pub const HEADER_SIZE: usize = 24;
    pub const SLOT_SIZE: usize = 4;
    pub const SLOT_ARRAY_OFFSET: usize = Self::HEADER_SIZE;
    /// 槽位区与负载区之间的硬边界，保证压实后有安全余量
    pub const MIN_FREE_GAP: usize = 8;

    fn read_u16(page: &[u8; PAGE_SIZE], off: usize) -> u16 {
        u16::from_le_bytes([page[off], page[off + 1]])
    }

    fn write_u16(page: &mut [u8; PAGE_SIZE], off: usize, value: u16) {
        page[off..off + 2].copy_from_slice(&value.to_le_bytes());
    }

    /// 判断该页是否已初始化为槽位属性页
    pub fn is_slotted(page: &[u8; PAGE_SIZE]) -> bool {
        page[0..4] == Self::MAGIC
    }

    /// 初始化一张空槽位页
    pub fn init(page: &mut [u8; PAGE_SIZE]) {
        page.fill(0);
        page[0..4].copy_from_slice(&Self::MAGIC);
        Self::write_u16(page, 4, Self::VERSION);
        Self::write_u16(page, 6, 0); // rec_count
        Self::write_u16(page, 8, 0); // live_count
        Self::write_u16(page, 10, 0); // slot_count
        Self::write_u16(page, 12, Self::HEADER_SIZE as u16); // free_start
        Self::write_u16(page, 14, PAGE_SIZE as u16); // free_end
    }

    /// 槽位区起始偏移（紧随 Header）
    pub fn free_start(page: &[u8; PAGE_SIZE]) -> usize {
        Self::read_u16(page, 12) as usize
    }

    /// 负载区起始偏移（向上生长后的剩余边界）
    pub fn free_end(page: &[u8; PAGE_SIZE]) -> usize {
        Self::read_u16(page, 14) as usize
    }

    /// 当前可用空闲字节数
    pub fn free_bytes(page: &[u8; PAGE_SIZE]) -> usize {
        Self::free_end(page).saturating_sub(Self::free_start(page))
    }

    /// 已分配槽位数（含死槽）
    pub fn slot_count(page: &[u8; PAGE_SIZE]) -> usize {
        Self::read_u16(page, 10) as usize
    }

    /// 存活记录数
    pub fn live_slots(page: &[u8; PAGE_SIZE]) -> usize {
        Self::read_u16(page, 8) as usize
    }

    /// 历史记录计数（只增不减，用于统计）
    pub fn rec_count(page: &[u8; PAGE_SIZE]) -> usize {
        Self::read_u16(page, 6) as usize
    }

    fn slot_descriptor(page: &[u8; PAGE_SIZE], slot: u8) -> Option<(usize, usize)> {
        let idx = slot as usize;
        if idx >= Self::slot_count(page) {
            return None;
        }
        let off = Self::SLOT_ARRAY_OFFSET + idx * Self::SLOT_SIZE;
        let data_off = Self::read_u16(page, off) as usize;
        let len = Self::read_u16(page, off + 2) as usize;
        if len == 0 {
            None
        } else {
            Some((data_off, len))
        }
    }

    fn write_slot_descriptor(page: &mut [u8; PAGE_SIZE], slot: u8, data_off: u16, len: u16) {
        let idx = slot as usize;
        let off = Self::SLOT_ARRAY_OFFSET + idx * Self::SLOT_SIZE;
        Self::write_u16(page, off, data_off);
        Self::write_u16(page, off + 2, len);
    }

    /// 读取槽位记录（死槽或越界返回 `None`）
    pub fn read(page: &[u8; PAGE_SIZE], slot: u8) -> Option<Vec<u8>> {
        let (data_off, len) = Self::slot_descriptor(page, slot)?;
        let end = data_off.checked_add(len)?;
        if end > PAGE_SIZE {
            return None;
        }
        Some(page[data_off..end].to_vec())
    }

    /// 把一条记录插入该页；空间不足返回 `None`（由调用方另行分配新页）
    pub fn insert(page: &mut [u8; PAGE_SIZE], record: &[u8]) -> Option<u8> {
        if record.is_empty() || record.len() > INLINE_RECORD_MAX {
            return None;
        }
        if !Self::is_slotted(page) {
            Self::init(page);
        }

        let needed = record.len() + Self::SLOT_SIZE + Self::MIN_FREE_GAP;
        if Self::free_bytes(page) < needed {
            // 压实后再试一次：回收死槽留下的碎片
            Self::compact(page);
            if Self::free_bytes(page) < needed {
                return None;
            }
        }

        // 复用最早的死槽，否则追加新槽
        let slot_count = Self::slot_count(page);
        let mut reuse_slot: Option<u8> = None;
        for idx in 0..slot_count {
            let off = Self::SLOT_ARRAY_OFFSET + idx * Self::SLOT_SIZE;
            if Self::read_u16(page, off + 2) == 0 {
                reuse_slot = Some(idx as u8);
                break;
            }
        }

        let slot = match reuse_slot {
            Some(s) => s,
            None => {
                if slot_count >= MAX_SLOTS_PER_PAGE {
                    return None;
                }
                let s = slot_count as u8;
                Self::write_u16(page, 10, (slot_count + 1) as u16);
                s
            }
        };

        // 负载从高地址向低地址生长
        let new_end = Self::free_end(page).checked_sub(record.len())?;
        let fs = Self::free_start(page);
        if new_end < fs + Self::SLOT_SIZE {
            return None;
        }
        page[new_end..new_end + record.len()].copy_from_slice(record);

        Self::write_u16(page, 14, new_end as u16);
        Self::write_slot_descriptor(page, slot, new_end as u16, record.len() as u16);
        Self::write_u16(page, 8, (Self::live_slots(page) + 1) as u16);
        Self::write_u16(page, 6, (Self::rec_count(page) + 1) as u16);

        // 槽位区扩张后推进 free_start
        let new_free_start = Self::SLOT_ARRAY_OFFSET + Self::slot_count(page) * Self::SLOT_SIZE;
        Self::write_u16(page, 12, new_free_start as u16);

        Some(slot)
    }

    /// 标记槽位为死槽（懒回收，空间不足时由 `compact` 压实）
    pub fn remove(page: &mut [u8; PAGE_SIZE], slot: u8) -> bool {
        let Some((_off, len)) = Self::slot_descriptor(page, slot) else {
            return false;
        };
        if len == 0 {
            return false;
        }
        let idx = slot as usize;
        let off = Self::SLOT_ARRAY_OFFSET + idx * Self::SLOT_SIZE;
        Self::write_u16(page, off + 2, 0);
        let live = Self::live_slots(page);
        if live > 0 {
            Self::write_u16(page, 8, (live - 1) as u16);
        }
        true
    }

    /// 原地压实：把存活记录紧贴页尾重新连续排布，清零中间碎片
    pub fn compact(page: &mut [u8; PAGE_SIZE]) {
        if !Self::is_slotted(page) {
            return;
        }

        let slot_count = Self::slot_count(page);
        let mut live: Vec<(u8, Vec<u8>)> = Vec::new();
        for idx in 0..slot_count {
            if let Some(data) = Self::read(page, idx as u8) {
                live.push((idx as u8, data));
            }
        }

        let mut cursor = PAGE_SIZE;
        for (slot, data) in live {
            cursor -= data.len();
            page[cursor..cursor + data.len()].copy_from_slice(&data);
            Self::write_slot_descriptor(page, slot, cursor as u16, data.len() as u16);
        }

        // 清零已释放的负载区，避免残留脏字节被误读
        let free_start = Self::SLOT_ARRAY_OFFSET + slot_count * Self::SLOT_SIZE;
        for b in page[free_start..cursor].iter_mut() {
            *b = 0;
        }

        Self::write_u16(page, 14, cursor as u16);
        Self::write_u16(page, 12, free_start as u16);
    }

    /// 该页是否已无任何存活记录（可整页回收）
    pub fn is_empty(page: &[u8; PAGE_SIZE]) -> bool {
        Self::is_slotted(page) && Self::live_slots(page) == 0
    }

    /// 空闲空间是否足以容纳一条指定长度的记录（含槽位描述符）
    pub fn can_fit(page: &[u8; PAGE_SIZE], record_len: usize) -> bool {
        if record_len == 0 || record_len > INLINE_RECORD_MAX {
            return false;
        }
        if !Self::is_slotted(page) {
            return true;
        }
        Self::free_bytes(page) >= record_len + Self::SLOT_SIZE
    }
}

// =========================================================================
// 紧凑属性记录编码
// =========================================================================

/// 属性值类型标签
const VTAG_INT: u8 = 0;
const VTAG_FLOAT: u8 = 1;
const VTAG_STRING: u8 = 2;
const VTAG_BOOL_FALSE: u8 = 3;
const VTAG_BOOL_TRUE: u8 = 4;

/// 紧凑属性编码器：替代 `bincode(HashMap)` 约 28 字节/实体的框架开销。
///
/// 布局：`varint(count)` + count × (`varint(key_len) | key_bytes | tag | varint-encoded value`)，
/// 其中 key 以紧凑 varint 长度前缀自描述（不写入 StringDict，避免字典膨胀溢出的连锁风险）。
#[derive(Default)]
pub struct PropCodec {
    buf: Vec<u8>,
}

impl PropCodec {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn push_varint(&mut self, mut value: u64) {
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            self.buf.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    pub fn push_key(&mut self, key: &str) {
        let bytes = key.as_bytes();
        self.push_varint(bytes.len() as u64);
        self.buf.extend_from_slice(bytes);
    }

    pub fn push_str_value(&mut self, value: &str) {
        self.buf.push(VTAG_STRING);
        let bytes = value.as_bytes();
        self.push_varint(bytes.len() as u64);
        self.buf.extend_from_slice(bytes);
    }

    /// 按类型标签推入属性值
    pub fn push_value(&mut self, value: &crate::graph::Value) {
        use crate::graph::Value;
        match value {
            Value::Int(v) => {
                self.buf.push(VTAG_INT);
                // ZigZag 编码，保证负数与小正整数都占 1 字节
                let zigzag = ((*v << 1) ^ (*v >> 63)) as u64;
                self.push_varint(zigzag);
            }
            Value::Float(v) => {
                self.buf.push(VTAG_FLOAT);
                self.buf.extend_from_slice(&v.to_le_bytes());
            }
            Value::String(s) => self.push_str_value(s),
            Value::Bool(b) => {
                self.buf
                    .push(if *b { VTAG_BOOL_TRUE } else { VTAG_BOOL_FALSE });
            }
        }
    }
}

/// 紧凑属性解码器
pub struct PropReader<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> PropReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.cursor)
    }

    pub fn read_varint(&mut self) -> Option<u64> {
        let mut result: u64 = 0;
        let mut shift = 0u32;
        loop {
            let byte = *self.data.get(self.cursor)?;
            self.cursor += 1;
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift += 7;
            if shift >= 64 {
                return None;
            }
        }
    }

    pub fn read_key(&mut self) -> Option<String> {
        let len = self.read_varint()? as usize;
        let end = self.cursor.checked_add(len)?;
        if end > self.data.len() {
            return None;
        }
        let key = String::from_utf8(self.data[self.cursor..end].to_vec()).ok()?;
        self.cursor = end;
        Some(key)
    }

    pub fn read_value(&mut self) -> Option<crate::graph::Value> {
        use crate::graph::Value;
        let tag = *self.data.get(self.cursor)?;
        self.cursor += 1;
        match tag {
            VTAG_INT => {
                let zigzag = self.read_varint()?;
                Some(Value::Int(((zigzag >> 1) as i64) ^ -((zigzag & 1) as i64)))
            }
            VTAG_FLOAT => {
                let end = self.cursor.checked_add(8)?;
                if end > self.data.len() {
                    return None;
                }
                let mut raw = [0u8; 8];
                raw.copy_from_slice(&self.data[self.cursor..end]);
                self.cursor = end;
                Some(Value::Float(f64::from_le_bytes(raw)))
            }
            VTAG_STRING => {
                let len = self.read_varint()? as usize;
                let end = self.cursor.checked_add(len)?;
                if end > self.data.len() {
                    return None;
                }
                let s = String::from_utf8(self.data[self.cursor..end].to_vec()).ok()?;
                self.cursor = end;
                Some(Value::String(s))
            }
            VTAG_BOOL_FALSE => Some(Value::Bool(false)),
            VTAG_BOOL_TRUE => Some(Value::Bool(true)),
            _ => None,
        }
    }
}

/// 编码属性字典为紧凑字节流
pub fn encode_props(props: &std::collections::HashMap<String, crate::graph::Value>) -> Vec<u8> {
    let mut codec = PropCodec::new();
    codec.push_varint(props.len() as u64);
    for (key, value) in props {
        codec.push_key(key);
        codec.push_value(value);
    }
    codec.into_bytes()
}

/// 解码紧凑字节流为属性字典
pub fn decode_props(data: &[u8]) -> Option<std::collections::HashMap<String, crate::graph::Value>> {
    let mut reader = PropReader::new(data);
    let count = reader.read_varint()? as usize;
    let mut props = std::collections::HashMap::with_capacity(count);
    for _ in 0..count {
        let key = reader.read_key()?;
        let value = reader.read_value()?;
        props.insert(key, value);
    }
    Some(props)
}
