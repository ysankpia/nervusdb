use nervusdb_storage::engine::GraphEngine;
use tempfile::tempdir;

#[test]
fn m2_compaction_preserves_neighbors() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let (a, b, c) = {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        let c = tx.create_node(30, 1).unwrap();
        tx.create_edge(a, 7, b);
        tx.commit().unwrap();
        (a, b, c)
    };

    let before = engine.begin_read();
    assert_eq!(before.neighbors(a, Some(7)).count(), 1);

    {
        let mut tx = engine.begin_write();
        tx.create_edge(a, 7, c);
        tx.commit().unwrap();
    }

    assert_eq!(engine.begin_read().neighbors(a, Some(7)).count(), 2);

    engine.compact().unwrap();

    let after = engine.begin_read();
    let edges: Vec<_> = after.neighbors(a, Some(7)).collect();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().any(|e| e.dst == b));
    assert!(edges.iter().any(|e| e.dst == c));
}
