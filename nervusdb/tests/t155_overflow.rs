use nervusdb::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_property_persistence_and_overflow() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // 1. Create a large property (> 4KB)
    let large_str = "A".repeat(10000);
    let node_id;
    {
        let mut tx = db.begin_write();
        let label = tx.get_or_create_label("LargeNode").unwrap();
        let n = tx.create_node(1, label).unwrap();
        tx.set_node_property(
            n,
            "payload".to_string(),
            PropertyValue::String(large_str.clone()),
        )
        .unwrap();
        tx.commit().unwrap();
        node_id = n;
    }

    // 2. Verify it's in L0Run (memory)
    {
        let snap = db.snapshot();
        let val = snap.node_property(node_id, "payload").unwrap();
        assert_eq!(val, PropertyValue::String(large_str.clone()));
    }

    // 3. Compact the graph (forces sinking to persistent store)
    db.compact().unwrap();

    // 4. Verify it's now in persistent store
    {
        let snap = db.snapshot();
        let val = snap.node_property(node_id, "payload").unwrap();
        assert_eq!(val, PropertyValue::String(large_str.clone()));
    }

    // 5. Restart and verify retrieval from persistent store
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    {
        let snap = db.snapshot();
        let val = snap.node_property(node_id, "payload").unwrap();
        assert_eq!(val, PropertyValue::String(large_str));
    }
}
