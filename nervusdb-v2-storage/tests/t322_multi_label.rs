use nervusdb_v2_storage::idmap::{I2eRecord, IdMap};
use nervusdb_v2_storage::pager::Pager;
use tempfile::tempdir;

#[test]
fn test_multi_label_create_and_lookup() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("test_multi_label.ndb");
    let mut pager = Pager::open(&ndb).unwrap();

    let mut idmap = IdMap::load(&mut pager).unwrap();

    // Create node with multiple labels
    let labels = vec![1, 2, 3]; // Person, Employee, Manager
    idmap
        .apply_create_node_multi_label(&mut pager, 100, labels.clone(), 0)
        .unwrap();

    // Verify labels
    let retrieved = idmap.get_labels(0).unwrap();
    assert_eq!(retrieved, labels);

    // Backward compat: get_label returns first label
    assert_eq!(idmap.get_label(0), Some(1));
}

#[test]
fn test_add_remove_labels() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("test_label_ops.ndb");
    let mut pager = Pager::open(&ndb).unwrap();

    let mut idmap = IdMap::load(&mut pager).unwrap();

    // Create node with single label
    idmap
        .apply_create_node_multi_label(&mut pager, 100, vec![1], 0)
        .unwrap();

    // Add label
    idmap.apply_add_label(&mut pager, 0, 2).unwrap();
    assert_eq!(idmap.get_labels(0).unwrap(), vec![1, 2]);

    // Remove label
    idmap.apply_remove_label(&mut pager, 0, 1).unwrap();
    assert_eq!(idmap.get_labels(0).unwrap(), vec![2]);
}

#[test]
fn test_persistence_multi_label() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("test_persist.ndb");

    {
        let mut pager = Pager::open(&ndb).unwrap();
        let mut idmap = IdMap::load(&mut pager).unwrap();
        idmap
            .apply_create_node_multi_label(&mut pager, 100, vec![1, 2, 3], 0)
            .unwrap();
    }

    // Reopen and verify
    // NOTE: Currently I2eRecord only persists first label.
    // Full multi-label persistence requires WAL support (Phase 1.2)
    let mut pager = Pager::open(&ndb).unwrap();
    let idmap = IdMap::load(&mut pager).unwrap();

    // After reload, only first label is restored from I2E
    assert_eq!(idmap.get_labels(0).unwrap(), vec![1]);

    // In-memory operations still work
    let mut idmap = idmap;
    idmap.apply_add_label(&mut pager, 0, 2).unwrap();
    idmap.apply_add_label(&mut pager, 0, 3).unwrap();
    assert_eq!(idmap.get_labels(0).unwrap(), vec![1, 2, 3]);
}
