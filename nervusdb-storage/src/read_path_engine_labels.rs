use crate::idmap::LabelId;
use crate::label_interner::{LabelInterner, LabelSnapshot};
use std::sync::{Arc, Mutex, RwLock};

pub(crate) fn published_label_snapshot(
    published_labels: &RwLock<Arc<LabelSnapshot>>,
) -> Arc<LabelSnapshot> {
    published_labels.read().unwrap().clone()
}

pub(crate) fn lookup_label_id(
    label_interner: &Mutex<LabelInterner>,
    name: &str,
) -> Option<LabelId> {
    label_interner.lock().unwrap().get_id(name)
}

pub(crate) fn lookup_label_name(
    label_interner: &Mutex<LabelInterner>,
    id: LabelId,
) -> Option<String> {
    label_interner
        .lock()
        .unwrap()
        .get_name(id)
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::{lookup_label_id, lookup_label_name, published_label_snapshot};
    use crate::label_interner::LabelInterner;
    use std::sync::{Arc, Mutex, RwLock};

    #[test]
    fn lookup_helpers_read_interner_state() {
        let mut interner = LabelInterner::new();
        let user_id = interner.get_or_create("User");
        let interner = Mutex::new(interner);

        assert_eq!(lookup_label_id(&interner, "User"), Some(user_id));
        assert_eq!(
            lookup_label_name(&interner, user_id),
            Some("User".to_string())
        );
    }

    #[test]
    fn published_snapshot_helper_clones_current_snapshot() {
        let mut interner = LabelInterner::new();
        let user_id = interner.get_or_create("User");
        let snapshot = Arc::new(interner.snapshot());
        let published = RwLock::new(snapshot.clone());

        let got = published_label_snapshot(&published);
        assert_eq!(got.get_id("User"), Some(user_id));
        assert_eq!(got.get_name(user_id), Some("User"));
    }
}
