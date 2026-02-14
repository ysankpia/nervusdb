use crate::idmap::{InternalNodeId, LabelId};
use crate::label_interner::LabelSnapshot;

pub(crate) fn node_primary_label(
    node_labels: &[Vec<LabelId>],
    iid: InternalNodeId,
) -> Option<LabelId> {
    node_labels.get(iid as usize)?.first().copied()
}

pub(crate) fn node_all_labels(
    node_labels: &[Vec<LabelId>],
    iid: InternalNodeId,
) -> Option<Vec<LabelId>> {
    node_labels.get(iid as usize).cloned()
}

pub(crate) fn resolve_label_id(labels: &LabelSnapshot, name: &str) -> Option<LabelId> {
    labels.get_id(name)
}

pub(crate) fn resolve_label_name(labels: &LabelSnapshot, id: LabelId) -> Option<String> {
    labels.get_name(id).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::{node_all_labels, node_primary_label, resolve_label_id, resolve_label_name};
    use crate::label_interner::LabelInterner;

    #[test]
    fn node_label_helpers_read_primary_and_all_labels() {
        let node_labels = vec![vec![1, 2], vec![7], vec![]];

        assert_eq!(node_primary_label(&node_labels, 0), Some(1));
        assert_eq!(node_primary_label(&node_labels, 1), Some(7));
        assert_eq!(node_primary_label(&node_labels, 2), None);
        assert_eq!(node_primary_label(&node_labels, 99), None);

        assert_eq!(node_all_labels(&node_labels, 0), Some(vec![1, 2]));
        assert_eq!(node_all_labels(&node_labels, 1), Some(vec![7]));
        assert_eq!(node_all_labels(&node_labels, 2), Some(vec![]));
        assert_eq!(node_all_labels(&node_labels, 99), None);
    }

    #[test]
    fn resolve_helpers_roundtrip_label_ids_and_names() {
        let mut interner = LabelInterner::new();
        let user_id = interner.get_or_create("User");
        let post_id = interner.get_or_create("Post");
        let snapshot = interner.snapshot();

        assert_eq!(resolve_label_id(&snapshot, "User"), Some(user_id));
        assert_eq!(resolve_label_id(&snapshot, "Post"), Some(post_id));
        assert_eq!(resolve_label_id(&snapshot, "Comment"), None);

        assert_eq!(
            resolve_label_name(&snapshot, user_id),
            Some("User".to_string())
        );
        assert_eq!(
            resolve_label_name(&snapshot, post_id),
            Some("Post".to_string())
        );
        assert_eq!(resolve_label_name(&snapshot, 99), None);
    }
}
