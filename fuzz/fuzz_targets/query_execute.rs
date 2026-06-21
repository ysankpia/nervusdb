#![no_main]

use libfuzzer_sys::fuzz_target;
use nervusdb::query::{EdgeKey, ExecuteOptions, GraphSnapshot, InternalNodeId, Params, RelTypeId};

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
    // query_execute 目标聚焦执行器稳定性。超长随机输入主要覆盖 parser/prepare，
    // 会显著放大单样本耗时并掩盖执行期信号。
    if input.len() > 1024 {
        return;
    }

    let Ok(prepared) = nervusdb::query::prepare(input) else {
        return;
    };

    let snapshot = EmptySnapshot;
    let params = Params::with_execute_options(ExecuteOptions {
        max_intermediate_rows: 100_000,
        max_collection_items: 100_000,
        soft_timeout_ms: 250,
        max_apply_rows_per_outer: 50_000,
    });

    let _ = prepared
        .execute_streaming(&snapshot, &params)
        .take(64)
        .collect::<Vec<_>>();
});
