use crate::csr::CsrSegment;
use crate::idmap::InternalNodeId;
use crate::snapshot::{EdgeKey, L0Run, RelTypeId};
use std::collections::HashSet;

#[allow(dead_code)]
pub(crate) fn apply_run_tombstones(
    run: &L0Run,
    blocked_nodes: &mut HashSet<InternalNodeId>,
    blocked_edges: &mut HashSet<EdgeKey>,
) {
    blocked_nodes.extend(run.iter_tombstoned_nodes());
    blocked_edges.extend(run.iter_tombstoned_edges());
}

pub(crate) fn load_outgoing_run_edges(
    run: &L0Run,
    src: InternalNodeId,
    current_edges: &mut Vec<EdgeKey>,
) {
    current_edges.extend_from_slice(run.edges_for_src(src));
}

pub(crate) fn load_incoming_run_edges(
    run: &L0Run,
    dst: InternalNodeId,
    current_edges: &mut Vec<EdgeKey>,
) {
    current_edges.extend_from_slice(run.edges_for_dst(dst));
}

pub(crate) fn load_outgoing_segment_edges(
    seg: &CsrSegment,
    src: InternalNodeId,
    rel: Option<RelTypeId>,
    current_segment_edges: &mut Vec<EdgeKey>,
) {
    current_segment_edges.extend(seg.neighbors(src, rel));
}

pub(crate) fn load_incoming_segment_edges(
    seg: &CsrSegment,
    dst: InternalNodeId,
    rel: Option<RelTypeId>,
    current_segment_edges: &mut Vec<EdgeKey>,
) {
    current_segment_edges.extend(seg.incoming_neighbors(dst, rel));
}

pub(crate) fn edge_blocked_outgoing(
    edge: EdgeKey,
    blocked_nodes: &HashSet<InternalNodeId>,
    blocked_edges: &HashSet<EdgeKey>,
) -> bool {
    blocked_nodes.contains(&edge.dst) || blocked_edges.contains(&edge)
}

pub(crate) fn edge_blocked_incoming(
    edge: EdgeKey,
    blocked_nodes: &HashSet<InternalNodeId>,
    blocked_edges: &HashSet<EdgeKey>,
) -> bool {
    blocked_nodes.contains(&edge.src) || blocked_edges.contains(&edge)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_run_tombstones, edge_blocked_incoming, edge_blocked_outgoing,
        load_incoming_run_edges, load_outgoing_run_edges,
    };
    use crate::property::PropertyValue;
    use crate::snapshot::{EdgeKey, L0Run};
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    fn mk_run() -> L0Run {
        let e1 = EdgeKey {
            src: 1,
            rel: 10,
            dst: 2,
        };
        let e2 = EdgeKey {
            src: 3,
            rel: 11,
            dst: 1,
        };
        L0Run::new(
            1,
            BTreeMap::from([(1, vec![e1])]),
            BTreeMap::from([(1, vec![e2])]),
            BTreeSet::from([99]),
            BTreeSet::from([e1]),
            BTreeMap::from([(
                1,
                BTreeMap::from([("k".to_string(), PropertyValue::Int(1))]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        )
    }

    #[test]
    fn apply_run_tombstones_accumulates_run_marks() {
        let run = mk_run();
        let mut blocked_nodes = HashSet::new();
        let mut blocked_edges = HashSet::new();

        apply_run_tombstones(&run, &mut blocked_nodes, &mut blocked_edges);

        assert!(blocked_nodes.contains(&99));
        assert!(blocked_edges.contains(&EdgeKey {
            src: 1,
            rel: 10,
            dst: 2,
        }));
    }

    #[test]
    fn run_edge_loaders_keep_directional_sources() {
        let run = mk_run();
        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();

        load_outgoing_run_edges(&run, 1, &mut outgoing);
        load_incoming_run_edges(&run, 1, &mut incoming);

        assert_eq!(outgoing.len(), 1);
        assert_eq!(incoming.len(), 1);
        assert_eq!(outgoing[0].src, 1);
        assert_eq!(incoming[0].dst, 1);
    }

    #[test]
    fn outgoing_block_rule_checks_dst_and_edge() {
        let edge = EdgeKey {
            src: 1,
            rel: 10,
            dst: 2,
        };
        let mut blocked_nodes = HashSet::new();
        let blocked_edges = HashSet::new();

        blocked_nodes.insert(2);
        assert!(edge_blocked_outgoing(edge, &blocked_nodes, &blocked_edges));

        blocked_nodes.clear();
        let mut blocked_edges = HashSet::new();
        blocked_edges.insert(edge);
        assert!(edge_blocked_outgoing(edge, &blocked_nodes, &blocked_edges));
    }

    #[test]
    fn incoming_block_rule_checks_src_and_edge() {
        let edge = EdgeKey {
            src: 7,
            rel: 10,
            dst: 2,
        };
        let mut blocked_nodes = HashSet::new();
        let blocked_edges = HashSet::new();

        blocked_nodes.insert(7);
        assert!(edge_blocked_incoming(edge, &blocked_nodes, &blocked_edges));

        blocked_nodes.clear();
        let mut blocked_edges = HashSet::new();
        blocked_edges.insert(edge);
        assert!(edge_blocked_incoming(edge, &blocked_nodes, &blocked_edges));
    }
}
