use nervusdb_v2_api::{GraphSnapshot, GraphStore};
use nervusdb_v2_storage::engine::GraphEngine;
use tempfile::tempdir;

#[test]
fn t47_graphstore_trait_exposes_neighbors() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let (a, b) = {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 7, b);
        tx.commit().unwrap();
        (a, b)
    };

    let snap = engine.snapshot();
    let edges: Vec<_> = snap.neighbors(a, Some(7)).collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].dst, b);
}
