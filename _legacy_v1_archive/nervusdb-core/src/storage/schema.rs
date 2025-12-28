#![cfg(not(target_arch = "wasm32"))]

use redb::TableDefinition;

use super::varint_key::VarintTripleKey;

// Legacy triple tables (v1.x, 24 bytes per key)
pub const TABLE_SPO: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("spo");
pub const TABLE_POS: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("pos");
pub const TABLE_OSP: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("osp");

// Compressed triple tables (v2.0, ~6-12 bytes per key)
pub const TABLE_SPO_V2: TableDefinition<VarintTripleKey, ()> = TableDefinition::new("spo_v2");
pub const TABLE_POS_V2: TableDefinition<VarintTripleKey, ()> = TableDefinition::new("pos_v2");
pub const TABLE_OSP_V2: TableDefinition<VarintTripleKey, ()> = TableDefinition::new("osp_v2");

pub const TABLE_ID_TO_STR: TableDefinition<u64, &str> = TableDefinition::new("id_to_str");
pub const TABLE_STR_TO_ID: TableDefinition<&str, u64> = TableDefinition::new("str_to_id");

// Legacy property tables (v1.x, JSON strings)
pub const TABLE_NODE_PROPS: TableDefinition<u64, &str> = TableDefinition::new("node_props");
pub const TABLE_EDGE_PROPS: TableDefinition<(u64, u64, u64), &str> =
    TableDefinition::new("edge_props");

// Binary property tables (v2.0, FlexBuffers)
// These store properties as raw bytes for better performance
pub const TABLE_NODE_PROPS_BINARY: TableDefinition<u64, &[u8]> =
    TableDefinition::new("node_props_v2");
pub const TABLE_EDGE_PROPS_BINARY: TableDefinition<(u64, u64, u64), &[u8]> =
    TableDefinition::new("edge_props_v2");

pub const TABLE_META: TableDefinition<&str, &str> = TableDefinition::new("meta");
