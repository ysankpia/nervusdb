use crate::idmap::InternalNodeId;
use crate::snapshot::L0Run;
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) fn collect_tombstoned_nodes(runs: &Arc<Vec<Arc<L0Run>>>) -> HashSet<InternalNodeId> {
    runs.iter()
        .flat_map(|run| run.iter_tombstoned_nodes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::collect_tombstoned_nodes;
    use crate::snapshot::L0Run;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    #[test]
    fn collect_tombstoned_nodes_unions_all_runs() {
        let run1 = Arc::new(L0Run::new(
            1,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeSet::from([1u32, 2u32]),
            BTreeSet::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        ));
        let run2 = Arc::new(L0Run::new(
            2,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeSet::from([2u32, 3u32]),
            BTreeSet::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        ));

        let runs = Arc::new(vec![run1, run2]);
        let got = collect_tombstoned_nodes(&runs);
        assert_eq!(got.len(), 3);
        assert!(got.contains(&1));
        assert!(got.contains(&2));
        assert!(got.contains(&3));
    }
}
