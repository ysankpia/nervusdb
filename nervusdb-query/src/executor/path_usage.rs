use super::{Row, Value};
use nervusdb_api::{EdgeKey, GraphSnapshot};

pub(super) fn edge_multiplicity<S: GraphSnapshot>(snapshot: &S, edge: EdgeKey) -> usize {
    let count = snapshot
        .neighbors(edge.src, Some(edge.rel))
        .filter(|candidate| candidate.dst == edge.dst)
        .count();
    count.max(1)
}

pub(super) fn path_alias_contains_edge<S: GraphSnapshot>(
    snapshot: &S,
    row: &Row,
    path_alias: Option<&str>,
    edge: EdgeKey,
) -> bool {
    if let Some(alias) = path_alias
        && let Some(Value::Path(path)) = row.get(alias)
    {
        let used = path
            .edges
            .iter()
            .filter(|existing| **existing == edge)
            .count();
        if used == 0 {
            return false;
        }
        return used >= edge_multiplicity(snapshot, edge);
    }
    false
}
