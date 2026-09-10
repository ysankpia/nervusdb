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
pub const DB_PAGE_VERSION: u32 = 1;

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
    pub const INLINE_PAYLOAD_OFFSET: usize = 352; // 剩余 3744 字节用于内联字典与索引元数据紧凑存储
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
    pub prop_page_id: u32,           // 4 bytes: 属性溢出页 PageId (INVALID_PAGE_ID 表示无溢出)
    pub inline_prop_val: i32,        // 4 bytes: 紧凑内联整数值或主属性辅助值
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
            prop_page_id: INVALID_PAGE_ID,
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
    pub in_use: u8,            // 1 byte: 1 为使用，0 为删除
    pub reserved: [u8; 3],     // 3 bytes
    pub edge_type_id: u32,     // 4 bytes: 关系类型字典 ID
    pub prop_page_id: u32,     // 4 bytes: 边属性溢出页 PageId
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
            prop_page_id: INVALID_PAGE_ID,
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
