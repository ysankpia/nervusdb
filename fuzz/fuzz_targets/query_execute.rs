#![no_main]

use libfuzzer_sys::fuzz_target;
use nervusdb_v2_query::{EdgeKey, GraphSnapshot, InternalNodeId, Params, RelTypeId};

struct EmptySnapshot;

impl GraphSnapshot for EmptySnapshot {
    type Neighbors<'a>
        = std::iter::Empty<EdgeKey>
    where
        Self: 'a;

    fn neighbors(&self, _src: InternalNodeId, _rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        std::iter::empty()
    }

    fn incoming_neighbors(
        &self,
        _dst: InternalNodeId,
        _rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_> {
        std::iter::empty()
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        Box::new(std::iter::empty())
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(prepared) = nervusdb_v2_query::prepare(input) else {
        return;
    };

    let snapshot = EmptySnapshot;
    let params = Params::new();

    let _ = prepared
        .execute_streaming(&snapshot, &params)
        .take(64)
        .collect::<Vec<_>>();
});
