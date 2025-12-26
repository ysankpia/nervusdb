use crate::idmap::InternalNodeId;
use crate::snapshot::{EdgeKey, RelTypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeRecord {
    pub rel: RelTypeId,
    pub dst: InternalNodeId,
}

#[derive(Debug)]
pub struct CsrSegment {
    pub id: SegmentId,
    pub min_src: InternalNodeId,
    pub max_src: InternalNodeId,
    pub offsets: Vec<u64>,
    pub edges: Vec<EdgeRecord>,
}

impl CsrSegment {
    pub fn neighbors(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_> {
        if src < self.min_src || src > self.max_src {
            return Box::new(std::iter::empty());
        }

        let idx = (src - self.min_src) as usize;
        let start = self.offsets[idx] as usize;
        let end = self.offsets[idx + 1] as usize;

        Box::new(
            self.edges[start..end]
                .iter()
                .filter(move |e| rel.is_none_or(|r| e.rel == r))
                .map(move |e| EdgeKey {
                    src,
                    rel: e.rel,
                    dst: e.dst,
                }),
        )
    }
}
