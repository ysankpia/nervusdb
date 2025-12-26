use crate::engine::GraphEngine;
use crate::snapshot;
use nervusdb_v2_api::{EdgeKey, GraphSnapshot, GraphStore, InternalNodeId, RelTypeId};

#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    inner: snapshot::Snapshot,
}

impl GraphStore for GraphEngine {
    type Snapshot = StorageSnapshot;

    fn snapshot(&self) -> Self::Snapshot {
        StorageSnapshot {
            inner: self.begin_read(),
        }
    }
}

impl GraphSnapshot for StorageSnapshot {
    type Neighbors<'a>
        = std::iter::Map<snapshot::NeighborsIter, fn(snapshot::EdgeKey) -> EdgeKey>
    where
        Self: 'a;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        fn conv(e: snapshot::EdgeKey) -> EdgeKey {
            EdgeKey {
                src: e.src,
                rel: e.rel,
                dst: e.dst,
            }
        }
        self.inner
            .neighbors(src, rel)
            .map(conv as fn(snapshot::EdgeKey) -> EdgeKey)
    }
}
