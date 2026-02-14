use crate::idmap::LabelId;
use crate::snapshot::RelTypeId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct GraphStatistics {
    pub node_counts_by_label: BTreeMap<LabelId, u64>,
    pub edge_counts_by_type: BTreeMap<RelTypeId, u64>,
    pub total_nodes: u64,
    pub total_edges: u64,
}

impl GraphStatistics {
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Total stats
        bytes.extend_from_slice(&self.total_nodes.to_le_bytes());
        bytes.extend_from_slice(&self.total_edges.to_le_bytes());

        // Node counts
        bytes.extend_from_slice(&(self.node_counts_by_label.len() as u32).to_le_bytes());
        for (label, count) in &self.node_counts_by_label {
            bytes.extend_from_slice(&label.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
        }

        // Edge counts
        bytes.extend_from_slice(&(self.edge_counts_by_type.len() as u32).to_le_bytes());
        for (rel, count) in &self.edge_counts_by_type {
            bytes.extend_from_slice(&rel.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
        }

        bytes
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }
        let mut pos = 0;

        let total_nodes = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
        pos += 8;
        let total_edges = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
        pos += 8;

        let node_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        pos += 4;
        let mut node_counts_by_label = BTreeMap::new();
        for _ in 0..node_len {
            let label = LabelId::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let count = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            node_counts_by_label.insert(label, count);
        }

        let edge_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        pos += 4;
        let mut edge_counts_by_type = BTreeMap::new();
        for _ in 0..edge_len {
            let rel = RelTypeId::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let count = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            edge_counts_by_type.insert(rel, count);
        }

        Some(Self {
            node_counts_by_label,
            edge_counts_by_type,
            total_nodes,
            total_edges,
        })
    }
}
