use crate::idmap::LabelId;
use crate::label_interner::LabelSnapshot;
use crate::read_path_labels::{resolve_label_id, resolve_label_name};
use std::sync::Arc;

pub(crate) fn resolve_symbol_id(labels: &Arc<LabelSnapshot>, name: &str) -> Option<LabelId> {
    resolve_label_id(labels, name)
}

pub(crate) fn resolve_symbol_name(labels: &Arc<LabelSnapshot>, id: LabelId) -> Option<String> {
    resolve_label_name(labels, id)
}

#[cfg(test)]
mod tests {
    use super::{resolve_symbol_id, resolve_symbol_name};
    use crate::label_interner::LabelSnapshot;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn labels_snapshot() -> Arc<LabelSnapshot> {
        Arc::new(LabelSnapshot::new(
            HashMap::from([("Person".to_string(), 1), ("KNOWS".to_string(), 2)]),
            vec!["".to_string(), "Person".to_string(), "KNOWS".to_string()],
        ))
    }

    #[test]
    fn resolve_symbol_id_reads_existing_names() {
        let labels = labels_snapshot();
        assert_eq!(resolve_symbol_id(&labels, "Person"), Some(1));
        assert_eq!(resolve_symbol_id(&labels, "Missing"), None);
    }

    #[test]
    fn resolve_symbol_name_reads_existing_ids() {
        let labels = labels_snapshot();
        assert_eq!(resolve_symbol_name(&labels, 2), Some("KNOWS".to_string()));
        assert_eq!(resolve_symbol_name(&labels, 9), None);
    }
}
