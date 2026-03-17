use crate::idmap::LabelId;
use crate::label_interner::LabelSnapshot;
use crate::snapshot::{PublishedRuns, PublishedSegments, Snapshot};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) fn load_properties_and_stats_roots(
    properties_root: &AtomicU64,
    stats_root: &AtomicU64,
) -> (u64, u64) {
    (
        properties_root.load(Ordering::Relaxed),
        stats_root.load(Ordering::Relaxed),
    )
}

pub(crate) fn build_snapshot_from_published(
    runs: Arc<PublishedRuns>,
    segments: Arc<PublishedSegments>,
    labels: Arc<LabelSnapshot>,
    node_labels: Arc<Vec<Vec<LabelId>>>,
    properties_root: u64,
    stats_root: u64,
) -> Snapshot {
    Snapshot::new(
        runs,
        segments,
        labels,
        node_labels,
        properties_root,
        stats_root,
    )
}

#[cfg(test)]
mod tests {
    use super::{build_snapshot_from_published, load_properties_and_stats_roots};
    use crate::label_interner::LabelInterner;
    use crate::snapshot::{PublishedRuns, PublishedSegments};
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn load_properties_and_stats_roots_reads_atomic_values() {
        let properties_root = AtomicU64::new(123);
        let stats_root = AtomicU64::new(456);
        assert_eq!(
            load_properties_and_stats_roots(&properties_root, &stats_root),
            (123, 456)
        );
    }

    #[test]
    fn build_snapshot_from_published_keeps_roots() {
        let interner = LabelInterner::new();
        let labels = Arc::new(interner.snapshot());
        let snapshot = build_snapshot_from_published(
            Arc::new(PublishedRuns::new()),
            Arc::new(PublishedSegments::new()),
            labels,
            Arc::new(Vec::new()),
            11,
            22,
        );
        assert_eq!(snapshot.properties_root, 11);
        assert_eq!(snapshot.stats_root, 22);
    }
}
