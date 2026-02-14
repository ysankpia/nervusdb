use crate::idmap::InternalNodeId;
use crate::snapshot::L0Run;
use std::sync::Arc;

pub(crate) fn is_tombstoned_node_in_runs(runs: &Arc<Vec<Arc<L0Run>>>, iid: InternalNodeId) -> bool {
    runs.iter()
        .any(|run| run.iter_tombstoned_nodes().any(|node| node == iid))
}

pub(crate) fn live_node_ids<'a>(
    max_id: InternalNodeId,
    runs: &'a Arc<Vec<Arc<L0Run>>>,
) -> Box<dyn Iterator<Item = InternalNodeId> + 'a> {
    Box::new((0..max_id).filter(move |&iid| !is_tombstoned_node_in_runs(runs, iid)))
}

#[cfg(test)]
mod tests {
    use super::{is_tombstoned_node_in_runs, live_node_ids};
    use crate::snapshot::L0Run;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    fn run_with_tombstones(txid: u64, tombstoned_nodes: BTreeSet<u32>) -> Arc<L0Run> {
        Arc::new(L0Run::new(
            txid,
            BTreeMap::new(),
            BTreeMap::new(),
            tombstoned_nodes,
            BTreeSet::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        ))
    }

    #[test]
    fn detects_tombstoned_node_when_any_run_marks_it() {
        let runs = Arc::new(vec![
            run_with_tombstones(2, BTreeSet::from([7])),
            run_with_tombstones(1, BTreeSet::new()),
        ]);

        assert!(is_tombstoned_node_in_runs(&runs, 7));
    }

    #[test]
    fn keeps_node_live_when_no_run_marks_tombstone() {
        let runs = Arc::new(vec![
            run_with_tombstones(2, BTreeSet::from([9])),
            run_with_tombstones(1, BTreeSet::new()),
        ]);

        assert!(!is_tombstoned_node_in_runs(&runs, 7));
    }

    #[test]
    fn live_node_ids_filters_out_tombstoned_nodes() {
        let runs = Arc::new(vec![
            run_with_tombstones(3, BTreeSet::from([1])),
            run_with_tombstones(2, BTreeSet::from([3])),
        ]);

        let live: Vec<u32> = live_node_ids(5, &runs).collect();
        assert_eq!(live, vec![0, 2, 4]);
    }
}
