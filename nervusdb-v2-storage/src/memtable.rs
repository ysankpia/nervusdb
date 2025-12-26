use crate::idmap::InternalNodeId;
use crate::snapshot::{EdgeKey, L0Run, RelTypeId};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Default)]
pub struct MemTable {
    out: HashMap<InternalNodeId, BTreeSet<EdgeKey>>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
}

impl MemTable {
    pub fn create_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        let key = EdgeKey { src, rel, dst };
        self.tombstoned_edges.remove(&key);
        self.out.entry(src).or_default().insert(key);
    }

    pub fn tombstone_node(&mut self, node: InternalNodeId) {
        self.tombstoned_nodes.insert(node);
    }

    pub fn tombstone_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        let key = EdgeKey { src, rel, dst };
        if let Some(set) = self.out.get_mut(&src) {
            set.remove(&key);
            if set.is_empty() {
                self.out.remove(&src);
            }
        }
        self.tombstoned_edges.insert(key);
    }

    pub fn freeze_into_run(self, txid: u64) -> L0Run {
        let mut edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeKey>> = BTreeMap::new();
        for (src, edges) in self.out {
            edges_by_src.insert(src, edges.into_iter().collect());
        }
        L0Run::new(
            txid,
            edges_by_src,
            self.tombstoned_nodes,
            self.tombstoned_edges,
        )
    }
}
