use nervusdb_api::{GraphSnapshot, GraphStore};
use nervusdb_storage::engine::GraphEngine;
use tempfile::tempdir;

#[test]
fn t51_snapshot_exposes_nodes_and_idmap() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let (a, b) = {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 7).unwrap();
        tx.create_edge(a, 99, b);
        tx.commit().unwrap();
        (a, b)
    };

    let snap = engine.snapshot();
    let mut nodes: Vec<_> = snap.nodes().collect();
    nodes.sort();
    assert_eq!(nodes, vec![a, b]);

    assert_eq!(snap.resolve_external(a), Some(10));
    assert_eq!(snap.resolve_external(b), Some(20));
    assert_eq!(snap.node_label(a), Some(1));
    assert_eq!(snap.node_label(b), Some(7));
}

#[test]
fn t51_snapshot_nodes_skip_tombstoned() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let (a, b) = {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.commit().unwrap();
        (a, b)
    };
    {
        let mut tx = engine.begin_write();
        tx.tombstone_node(a);
        tx.commit().unwrap();
    }

    let snap = engine.snapshot();
    let nodes: Vec<_> = snap.nodes().collect();
    assert_eq!(nodes, vec![b]);
    assert!(snap.is_tombstoned_node(a));
}
