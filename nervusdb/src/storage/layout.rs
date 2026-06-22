use crate::api::{EdgeKey, ExternalId, InternalNodeId, LabelId, PropertyValue, RelTypeId};
use crate::storage::{Error, Result};

pub(crate) const KEY_FLAG_TOMBSTONE: u8 = 0b0000_0001;

const TAG_NODE: u8 = 0x01;
const TAG_EXT2NODE: u8 = 0x02;
const TAG_LABEL_NAME: u8 = 0x10;
const TAG_LABEL_ID: u8 = 0x11;
const TAG_REL_NAME: u8 = 0x12;
const TAG_REL_ID: u8 = 0x13;
const TAG_NODE_LABEL: u8 = 0x20;
const TAG_LABEL_NODE: u8 = 0x21;
const TAG_NODE_PROP: u8 = 0x40;
const TAG_EDGE_PROP: u8 = 0x41;
const TAG_NODE_PROP_INDEX: u8 = 0x50;

#[cfg(feature = "unstable-admin")]
#[derive(Debug)]
pub(crate) struct NodePropIndexEntry {
    pub(crate) label: LabelId,
    pub(crate) property_key: String,
    pub(crate) value: PropertyValue,
    pub(crate) node: InternalNodeId,
}

#[inline]
pub(crate) fn key_u32(value: u32) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

#[inline]
pub(crate) fn decode_u32(bytes: &[u8]) -> Option<u32> {
    let raw: [u8; 4] = bytes.try_into().ok()?;
    Some(u32::from_be_bytes(raw))
}

#[inline]
fn decode_u64(bytes: &[u8]) -> Option<u64> {
    let raw: [u8; 8] = bytes.try_into().ok()?;
    Some(u64::from_be_bytes(raw))
}

#[inline]
fn tagged_u32_key(tag: u8, value: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    out.push(tag);
    out.extend_from_slice(&value.to_be_bytes());
    out
}

#[inline]
fn tagged_name_key(tag: u8, name: &str) -> Vec<u8> {
    let len = u16::try_from(name.len()).expect("name length should fit in u16");
    let mut out = Vec::with_capacity(3 + name.len());
    out.push(tag);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(name.as_bytes());
    out
}

pub(crate) fn node_scan_prefix() -> Vec<u8> {
    vec![TAG_NODE]
}

pub(crate) fn node_key(iid: InternalNodeId) -> Vec<u8> {
    tagged_u32_key(TAG_NODE, iid)
}

pub(crate) fn parse_node_key(key: &[u8]) -> Option<InternalNodeId> {
    if key.len() == 5 && key[0] == TAG_NODE {
        decode_u32(&key[1..5])
    } else {
        None
    }
}

pub(crate) fn ext2node_key(external_id: ExternalId) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.push(TAG_EXT2NODE);
    out.extend_from_slice(&external_id.to_be_bytes());
    out
}

pub(crate) fn label_name_key(name: &str) -> Vec<u8> {
    tagged_name_key(TAG_LABEL_NAME, name)
}

pub(crate) fn label_id_key(id: LabelId) -> Vec<u8> {
    tagged_u32_key(TAG_LABEL_ID, id)
}

pub(crate) fn rel_name_key(name: &str) -> Vec<u8> {
    tagged_name_key(TAG_REL_NAME, name)
}

pub(crate) fn rel_id_key(id: RelTypeId) -> Vec<u8> {
    tagged_u32_key(TAG_REL_ID, id)
}

#[cfg(feature = "unstable-admin")]
pub(crate) fn node_label_scan_prefix() -> Vec<u8> {
    vec![TAG_NODE_LABEL]
}

pub(crate) fn node_label_prefix(node: InternalNodeId) -> Vec<u8> {
    tagged_u32_key(TAG_NODE_LABEL, node)
}

pub(crate) fn node_label_key(node: InternalNodeId, label: LabelId) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.push(TAG_NODE_LABEL);
    out.extend_from_slice(&node.to_be_bytes());
    out.extend_from_slice(&label.to_be_bytes());
    out
}

pub(crate) fn parse_node_label_key(key: &[u8]) -> Option<(InternalNodeId, LabelId)> {
    if key.len() != 9 || key[0] != TAG_NODE_LABEL {
        return None;
    }
    Some((decode_u32(&key[1..5])?, decode_u32(&key[5..9])?))
}

#[cfg(feature = "unstable-admin")]
pub(crate) fn label_node_scan_prefix() -> Vec<u8> {
    vec![TAG_LABEL_NODE]
}

pub(crate) fn label_node_prefix(label: LabelId) -> Vec<u8> {
    tagged_u32_key(TAG_LABEL_NODE, label)
}

pub(crate) fn label_node_key(label: LabelId, node: InternalNodeId) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.push(TAG_LABEL_NODE);
    out.extend_from_slice(&label.to_be_bytes());
    out.extend_from_slice(&node.to_be_bytes());
    out
}

pub(crate) fn parse_label_node_key(key: &[u8]) -> Option<(LabelId, InternalNodeId)> {
    if key.len() != 9 || key[0] != TAG_LABEL_NODE {
        return None;
    }
    Some((decode_u32(&key[1..5])?, decode_u32(&key[5..9])?))
}

pub(crate) fn adj_out_scan_prefix() -> Vec<u8> {
    Vec::new()
}

pub(crate) fn adj_out_prefix(src: InternalNodeId, rel: Option<RelTypeId>) -> Vec<u8> {
    let mut out = key_u32(src);
    if let Some(rel) = rel {
        out.extend_from_slice(&rel.to_be_bytes());
    }
    out
}

pub(crate) fn adj_out_key(edge: EdgeKey) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&edge.src.to_be_bytes());
    out.extend_from_slice(&edge.rel.to_be_bytes());
    out.extend_from_slice(&edge.dst.to_be_bytes());
    out
}

pub(crate) fn edge_key_from_adj_out(key: &[u8]) -> Option<EdgeKey> {
    if key.len() != 12 {
        return None;
    }
    Some(EdgeKey {
        src: decode_u32(&key[0..4])?,
        rel: decode_u32(&key[4..8])?,
        dst: decode_u32(&key[8..12])?,
    })
}

pub(crate) fn dst_from_adj_out_key(key: &[u8]) -> Option<InternalNodeId> {
    if key.len() == 12 {
        decode_u32(&key[8..12])
    } else {
        None
    }
}

#[cfg(feature = "unstable-admin")]
pub(crate) fn adj_in_scan_prefix() -> Vec<u8> {
    Vec::new()
}

pub(crate) fn adj_in_prefix(dst: InternalNodeId, rel: Option<RelTypeId>) -> Vec<u8> {
    let mut out = key_u32(dst);
    if let Some(rel) = rel {
        out.extend_from_slice(&rel.to_be_bytes());
    }
    out
}

pub(crate) fn adj_in_key(edge: EdgeKey) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&edge.dst.to_be_bytes());
    out.extend_from_slice(&edge.rel.to_be_bytes());
    out.extend_from_slice(&edge.src.to_be_bytes());
    out
}

pub(crate) fn edge_key_from_adj_in(key: &[u8]) -> Option<EdgeKey> {
    if key.len() != 12 {
        return None;
    }
    Some(EdgeKey {
        dst: decode_u32(&key[0..4])?,
        rel: decode_u32(&key[4..8])?,
        src: decode_u32(&key[8..12])?,
    })
}

pub(crate) fn src_from_adj_in_key(key: &[u8]) -> Option<InternalNodeId> {
    if key.len() == 12 {
        decode_u32(&key[8..12])
    } else {
        None
    }
}

#[cfg(feature = "unstable-admin")]
pub(crate) fn node_prop_scan_prefix() -> Vec<u8> {
    vec![TAG_NODE_PROP]
}

pub(crate) fn node_prop_prefix(node: InternalNodeId) -> Vec<u8> {
    tagged_u32_key(TAG_NODE_PROP, node)
}

pub(crate) fn node_prop_key(node: InternalNodeId, key: &str) -> Vec<u8> {
    let len = u32::try_from(key.len()).expect("property key length should fit in u32");
    let mut out = Vec::with_capacity(9 + key.len());
    out.push(TAG_NODE_PROP);
    out.extend_from_slice(&node.to_be_bytes());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(key.as_bytes());
    out
}

pub(crate) fn parse_node_prop_key(key: &[u8]) -> Option<(InternalNodeId, String)> {
    if key.len() < 9 || key[0] != TAG_NODE_PROP {
        return None;
    }
    let node = decode_u32(&key[1..5])?;
    let len = u32::from_be_bytes(key[5..9].try_into().ok()?) as usize;
    if key.len() != 9 + len {
        return None;
    }
    let property_key = String::from_utf8(key[9..].to_vec()).ok()?;
    Some((node, property_key))
}

pub(crate) fn parse_node_prop_key_for_node(key: &[u8], node: InternalNodeId) -> Option<String> {
    let (found, property_key) = parse_node_prop_key(key)?;
    if found == node {
        Some(property_key)
    } else {
        None
    }
}

#[cfg(feature = "unstable-admin")]
pub(crate) fn edge_prop_scan_prefix() -> Vec<u8> {
    vec![TAG_EDGE_PROP]
}

pub(crate) fn edge_prop_prefix(edge: EdgeKey) -> Vec<u8> {
    let mut out = Vec::with_capacity(13);
    out.push(TAG_EDGE_PROP);
    out.extend_from_slice(&edge.src.to_be_bytes());
    out.extend_from_slice(&edge.rel.to_be_bytes());
    out.extend_from_slice(&edge.dst.to_be_bytes());
    out
}

pub(crate) fn edge_prop_key(edge: EdgeKey, key: &str) -> Vec<u8> {
    let len = u32::try_from(key.len()).expect("property key length should fit in u32");
    let mut out = edge_prop_prefix(edge);
    out.reserve(4 + key.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(key.as_bytes());
    out
}

pub(crate) fn parse_edge_prop_key(key: &[u8]) -> Option<(EdgeKey, String)> {
    if key.len() < 17 || key[0] != TAG_EDGE_PROP {
        return None;
    }
    let edge = EdgeKey {
        src: decode_u32(&key[1..5])?,
        rel: decode_u32(&key[5..9])?,
        dst: decode_u32(&key[9..13])?,
    };
    let len = u32::from_be_bytes(key[13..17].try_into().ok()?) as usize;
    if key.len() != 17 + len {
        return None;
    }
    let property_key = String::from_utf8(key[17..].to_vec()).ok()?;
    Some((edge, property_key))
}

pub(crate) fn parse_edge_prop_key_for_edge(key: &[u8], edge: EdgeKey) -> Option<String> {
    let (found, property_key) = parse_edge_prop_key(key)?;
    if found == edge {
        Some(property_key)
    } else {
        None
    }
}

#[cfg(feature = "unstable-admin")]
pub(crate) fn node_prop_index_scan_prefix() -> Vec<u8> {
    vec![TAG_NODE_PROP_INDEX]
}

pub(crate) fn node_prop_index_key(
    label: LabelId,
    key: &str,
    value: &PropertyValue,
    node: InternalNodeId,
) -> Vec<u8> {
    let encoded_value = value.encode();
    let key_len = u16::try_from(key.len()).expect("property key length should fit in u16");
    let value_len =
        u32::try_from(encoded_value.len()).expect("property value length should fit in u32");
    let mut out = Vec::with_capacity(15 + key.len() + encoded_value.len());
    out.push(TAG_NODE_PROP_INDEX);
    out.extend_from_slice(&label.to_be_bytes());
    out.extend_from_slice(&key_len.to_be_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&value_len.to_be_bytes());
    out.extend_from_slice(&encoded_value);
    out.extend_from_slice(&node.to_be_bytes());
    out
}

pub(crate) fn node_prop_index_prefix(label: LabelId, key: &str, value: &PropertyValue) -> Vec<u8> {
    let encoded_value = value.encode();
    let key_len = u16::try_from(key.len()).expect("property key length should fit in u16");
    let value_len =
        u32::try_from(encoded_value.len()).expect("property value length should fit in u32");
    let mut out = Vec::with_capacity(11 + key.len() + encoded_value.len());
    out.push(TAG_NODE_PROP_INDEX);
    out.extend_from_slice(&label.to_be_bytes());
    out.extend_from_slice(&key_len.to_be_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&value_len.to_be_bytes());
    out.extend_from_slice(&encoded_value);
    out
}

pub(crate) fn parse_node_prop_index_node(key: &[u8]) -> Option<InternalNodeId> {
    if key.len() < 15 || key[0] != TAG_NODE_PROP_INDEX {
        return None;
    }
    let key_len = u16::from_be_bytes(key[5..7].try_into().ok()?) as usize;
    let value_len_offset = 7 + key_len;
    if key.len() < value_len_offset + 8 {
        return None;
    }
    let value_len = u32::from_be_bytes(
        key[value_len_offset..value_len_offset + 4]
            .try_into()
            .ok()?,
    ) as usize;
    let node_offset = value_len_offset + 4 + value_len;
    if key.len() != node_offset + 4 {
        return None;
    }
    decode_u32(&key[node_offset..node_offset + 4])
}

#[cfg(feature = "unstable-admin")]
pub(crate) fn parse_node_prop_index_key(key: &[u8]) -> Option<NodePropIndexEntry> {
    if key.len() < 15 || key[0] != TAG_NODE_PROP_INDEX {
        return None;
    }
    let label = decode_u32(&key[1..5])?;
    let key_len = u16::from_be_bytes(key[5..7].try_into().ok()?) as usize;
    let value_len_offset = 7 + key_len;
    if key.len() < value_len_offset + 8 {
        return None;
    }
    let property_key = String::from_utf8(key[7..7 + key_len].to_vec()).ok()?;
    let value_len = u32::from_be_bytes(
        key[value_len_offset..value_len_offset + 4]
            .try_into()
            .ok()?,
    ) as usize;
    let value_offset = value_len_offset + 4;
    let node_offset = value_offset + value_len;
    if key.len() != node_offset + 4 {
        return None;
    }
    let value = PropertyValue::decode(&key[value_offset..node_offset]).ok()?;
    let node = decode_u32(&key[node_offset..node_offset + 4])?;
    Some(NodePropIndexEntry {
        label,
        property_key,
        value,
        node,
    })
}

pub(crate) fn encode_node_value(external_id: ExternalId, flags: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(&external_id.to_be_bytes());
    out.push(flags);
    out
}

pub(crate) fn decode_node_value(bytes: &[u8]) -> Option<(ExternalId, u8)> {
    parse_node_value(bytes)
}

pub(crate) fn parse_node_value(bytes: &[u8]) -> Option<(ExternalId, u8)> {
    if bytes.len() < 9 {
        return None;
    }
    let external_id = decode_u64(&bytes[..8])?;
    Some((external_id, bytes[8]))
}

pub(crate) fn parse_prop_value(bytes: &[u8]) -> Result<PropertyValue> {
    PropertyValue::decode(bytes).map_err(|e| Error::PropertyDecode(e.to_string()))
}
