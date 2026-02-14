use crate::pager::Pager;
use crate::stats::GraphStatistics;

pub(crate) fn read_statistics(pager: &Pager, stats_root: u64) -> crate::Result<GraphStatistics> {
    if stats_root == 0 {
        return Ok(GraphStatistics::default());
    }

    let bytes = crate::blob_store::BlobStore::read(pager, stats_root)?;
    GraphStatistics::decode(&bytes).ok_or(crate::Error::StorageCorrupted(
        "failed to decode statistics",
    ))
}

#[cfg(test)]
mod tests {
    use super::read_statistics;
    use crate::blob_store::BlobStore;
    use crate::pager::Pager;
    use crate::stats::GraphStatistics;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    #[test]
    fn read_statistics_returns_default_when_root_is_zero() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("stats_zero.ndb");
        let pager = Pager::open(&path).expect("open pager");

        let stats = read_statistics(&pager, 0).expect("read statistics");
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.total_edges, 0);
        assert!(stats.node_counts_by_label.is_empty());
        assert!(stats.edge_counts_by_type.is_empty());
    }

    #[test]
    fn read_statistics_roundtrips_encoded_payload() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("stats_ok.ndb");
        let mut pager = Pager::open(&path).expect("open pager");

        let expected = GraphStatistics {
            node_counts_by_label: BTreeMap::from([(1, 2)]),
            edge_counts_by_type: BTreeMap::from([(7, 3)]),
            total_nodes: 11,
            total_edges: 13,
        };

        let root = BlobStore::write(&mut pager, &expected.encode()).expect("write blob");
        let actual = read_statistics(&pager, root).expect("read statistics");

        assert_eq!(actual.total_nodes, expected.total_nodes);
        assert_eq!(actual.total_edges, expected.total_edges);
        assert_eq!(actual.node_counts_by_label.get(&1), Some(&2));
        assert_eq!(actual.edge_counts_by_type.get(&7), Some(&3));
    }

    #[test]
    fn read_statistics_rejects_corrupted_payload() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("stats_bad.ndb");
        let mut pager = Pager::open(&path).expect("open pager");

        let root = BlobStore::write(&mut pager, b"bad").expect("write blob");
        let err = read_statistics(&pager, root).expect_err("should fail");
        assert!(matches!(
            err,
            crate::Error::StorageCorrupted("failed to decode statistics")
        ));
    }
}
