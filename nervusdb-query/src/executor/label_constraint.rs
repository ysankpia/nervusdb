use super::LabelId;
use nervusdb_api::{GraphSnapshot, InternalNodeId};

#[derive(Debug, Clone)]
pub(super) enum LabelConstraint {
    Any,
    Required(Vec<LabelId>),
    Impossible,
}

pub(super) fn resolve_label_constraint<S: GraphSnapshot>(
    snapshot: &S,
    labels: &[String],
) -> LabelConstraint {
    if labels.is_empty() {
        return LabelConstraint::Any;
    }

    let mut ids = Vec::with_capacity(labels.len());
    for label in labels {
        let Some(id) = snapshot.resolve_label_id(label) else {
            return LabelConstraint::Impossible;
        };
        ids.push(id);
    }
    LabelConstraint::Required(ids)
}

pub(super) fn node_matches_label_constraint<S: GraphSnapshot>(
    snapshot: &S,
    node: InternalNodeId,
    constraint: &LabelConstraint,
) -> bool {
    match constraint {
        LabelConstraint::Any => true,
        LabelConstraint::Impossible => false,
        LabelConstraint::Required(required) => snapshot
            .resolve_node_labels(node)
            .is_some_and(|labels| required.iter().all(|id| labels.contains(id))),
    }
}
