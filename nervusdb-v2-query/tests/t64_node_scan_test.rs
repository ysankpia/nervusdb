use nervusdb_v2_api::GraphStore;
use nervusdb_v2_query::{Params, prepare};
use nervusdb_v2_storage::engine::GraphEngine;
use nervusdb_v2_storage::property::PropertyValue;
use tempfile::tempdir;

#[test]
fn t64_match_node_scan_where_property() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();

    // Create isolated nodes with properties (no edges).
    {
        let mut txn = engine.begin_write();
        let linus = txn.create_node(1, 0).unwrap();
        let other = txn.create_node(2, 0).unwrap();
        txn.set_node_property(
            linus,
            "name".to_string(),
            PropertyValue::String("Linus".into()),
        );
        txn.set_node_property(
            other,
            "name".to_string(),
            PropertyValue::String("Someone".into()),
        );
        txn.commit().unwrap();
    }

    let snapshot = engine.snapshot();
    let q = prepare("MATCH (n) WHERE n.name = 'Linus' RETURN n").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].0, "n");
}

#[test]
fn t64_match_node_scan_limit_0() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    {
        let mut txn = engine.begin_write();
        let _ = txn.create_node(1, 0).unwrap();
        txn.commit().unwrap();
    }

    let snapshot = engine.snapshot();
    let q = prepare("MATCH (n) RETURN n LIMIT 0").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert!(rows.is_empty());
}

#[test]
fn t64_match_node_scan_delete_isolated_nodes() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    {
        let mut txn = engine.begin_write();
        let linus = txn.create_node(1, 0).unwrap();
        txn.set_node_property(
            linus,
            "name".to_string(),
            PropertyValue::String("Linus".into()),
        );
        let _ = txn.create_node(2, 0).unwrap();
        txn.commit().unwrap();
    }

    let snapshot = engine.snapshot();
    let q = prepare("MATCH (n) WHERE n.name = 'Linus' DELETE n").unwrap();
    let mut txn = engine.begin_write();
    let deleted = q
        .execute_write(&snapshot, &mut txn, &Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(deleted, 1);
}
