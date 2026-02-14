use crate::idmap::LabelId;
use crate::snapshot::RelTypeId;
use crate::stats::GraphStatistics;

pub(crate) fn node_count_from_stats(
    stats: Option<&GraphStatistics>,
    label: Option<LabelId>,
) -> u64 {
    if let Some(stats) = stats {
        if let Some(lid) = label {
            stats.node_counts_by_label.get(&lid).copied().unwrap_or(0)
        } else {
            stats.total_nodes
        }
    } else {
        0
    }
}

pub(crate) fn edge_count_from_stats(
    stats: Option<&GraphStatistics>,
    rel: Option<RelTypeId>,
) -> u64 {
    if let Some(stats) = stats {
        if let Some(rid) = rel {
            stats.edge_counts_by_type.get(&rid).copied().unwrap_or(0)
        } else {
            stats.total_edges
        }
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{edge_count_from_stats, node_count_from_stats};
    use crate::stats::GraphStatistics;
    use std::collections::BTreeMap;

    #[test]
    fn node_count_uses_total_or_label_bucket() {
        let stats = GraphStatistics {
            total_nodes: 10,
            node_counts_by_label: BTreeMap::from([(1, 7), (2, 3)]),
            ..Default::default()
        };

        assert_eq!(node_count_from_stats(Some(&stats), None), 10);
        assert_eq!(node_count_from_stats(Some(&stats), Some(1)), 7);
        assert_eq!(node_count_from_stats(Some(&stats), Some(99)), 0);
        assert_eq!(node_count_from_stats(None, None), 0);
    }

    #[test]
    fn edge_count_uses_total_or_rel_bucket() {
        let stats = GraphStatistics {
            total_edges: 12,
            edge_counts_by_type: BTreeMap::from([(5, 11), (6, 1)]),
            ..Default::default()
        };

        assert_eq!(edge_count_from_stats(Some(&stats), None), 12);
        assert_eq!(edge_count_from_stats(Some(&stats), Some(5)), 11);
        assert_eq!(edge_count_from_stats(Some(&stats), Some(99)), 0);
        assert_eq!(edge_count_from_stats(None, None), 0);
    }
}
