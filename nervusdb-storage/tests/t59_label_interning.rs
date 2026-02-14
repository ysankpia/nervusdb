use nervusdb_storage::Result;
use nervusdb_storage::engine::GraphEngine;
use std::path::PathBuf;
use tempfile::tempdir;

fn open_engine(dir: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");
    (ndb, wal)
}

#[test]
fn t59_label_interning_basic() -> Result<()> {
    let dir = tempdir().unwrap();
    let (ndb, wal) = open_engine(&dir);

    let engine = GraphEngine::open(&ndb, &wal)?;

    // Test label creation
    let user_id = engine.get_or_create_label("User")?;
    assert_eq!(user_id, 0);

    let post_id = engine.get_or_create_label("Post")?;
    assert_eq!(post_id, 1);

    // Test label lookup
    assert_eq!(engine.get_label_id("User"), Some(user_id));
    assert_eq!(engine.get_label_id("Post"), Some(post_id));
    assert_eq!(engine.get_label_id("Comment"), None);

    // Test label name lookup
    assert_eq!(engine.get_label_name(0), Some("User".to_string()));
    assert_eq!(engine.get_label_name(1), Some("Post".to_string()));
    assert_eq!(engine.get_label_name(2), None);

    // Test idempotent creation
    let user_id2 = engine.get_or_create_label("User")?;
    assert_eq!(user_id, user_id2);

    Ok(())
}

/// Note: Label persistence requires CreateLabel WAL records or manifest storage.
/// For MVP, labels are created in-memory but may not persist across restarts
/// without additional infrastructure (T59.5).
#[test]
fn t59_label_snapshot() {
    let dir = tempdir().unwrap();
    let (ndb, wal) = open_engine(&dir);

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    engine.get_or_create_label("User").unwrap();

    // Get label snapshot
    let snapshot = engine.label_snapshot();
    assert_eq!(snapshot.get_id("User"), Some(0));
    assert_eq!(snapshot.get_name(0), Some("User"));
}

#[test]
fn t59_label_interner_unit_tests() {
    use nervusdb_storage::label_interner::LabelInterner;

    let mut interner = LabelInterner::new();

    // Test get_or_create
    let id1 = interner.get_or_create("User");
    let id2 = interner.get_or_create("User");
    assert_eq!(id1, id2);

    let id3 = interner.get_or_create("Post");
    assert_ne!(id1, id3);

    // Test snapshot
    let snapshot = interner.snapshot();
    assert_eq!(snapshot.get_id("User"), Some(0));
    assert_eq!(snapshot.get_name(1), Some("Post"));

    // Test modification doesn't affect snapshot
    interner.get_or_create("Comment");
    assert_eq!(snapshot.get_id("Comment"), None);

    // Test contains
    assert!(interner.contains("User"));
    assert!(!interner.contains("NonExistent"));

    // Test len
    assert_eq!(interner.len(), 3);

    // Test next_id
    assert_eq!(interner.next_id(), 3);
}
