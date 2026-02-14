use crate::idmap::InternalNodeId;
use crate::snapshot::EdgeKey;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn iter_edges(
    edges_by_src: &BTreeMap<InternalNodeId, Vec<EdgeKey>>,
) -> impl Iterator<Item = EdgeKey> + '_ {
    edges_by_src
        .values()
        .flat_map(|edges| edges.iter().copied())
}

pub(crate) fn iter_tombstoned_nodes(
    tombstoned_nodes: &BTreeSet<InternalNodeId>,
) -> impl Iterator<Item = InternalNodeId> + '_ {
    tombstoned_nodes.iter().copied()
}

pub(crate) fn iter_tombstoned_edges(
    tombstoned_edges: &BTreeSet<EdgeKey>,
) -> impl Iterator<Item = EdgeKey> + '_ {
    tombstoned_edges.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::{iter_edges, iter_tombstoned_edges, iter_tombstoned_nodes};
    use crate::snapshot::EdgeKey;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn iter_edges_flattens_all_source_buckets() {
        let edges = BTreeMap::from([
            (
                1,
                vec![EdgeKey {
                    src: 1,
                    rel: 10,
                    dst: 2,
                }],
            ),
            (
                3,
                vec![EdgeKey {
                    src: 3,
                    rel: 20,
                    dst: 4,
                }],
            ),
        ]);

        let actual: Vec<EdgeKey> = iter_edges(&edges).collect();
        assert_eq!(actual.len(), 2);
    }

    #[test]
    fn iter_tombstoned_nodes_and_edges_preserve_set_contents() {
        let tombstoned_nodes = BTreeSet::from([7, 9]);
        let tombstoned_edges = BTreeSet::from([EdgeKey {
            src: 1,
            rel: 2,
            dst: 3,
        }]);

        let nodes: Vec<u32> = iter_tombstoned_nodes(&tombstoned_nodes).collect();
        let edges: Vec<EdgeKey> = iter_tombstoned_edges(&tombstoned_edges).collect();

        assert_eq!(nodes, vec![7, 9]);
        assert_eq!(edges.len(), 1);
    }
}
