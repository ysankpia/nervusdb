use nervusdb_storage::engine::GraphEngine;
use tempfile::tempdir;

#[test]
fn m1_commit_is_recoverable() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 7, b);
        tx.commit().unwrap();
    }

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let a = engine.lookup_internal_id(10).unwrap();
    let snapshot = engine.begin_read();
    let edges: Vec<_> = snapshot.neighbors(a, Some(7)).collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].src, a);
    assert_eq!(edges[0].dst, engine.lookup_internal_id(20).unwrap());
    assert_eq!(edges[0].rel, 7);
}

#[test]
fn m1_uncommitted_tx_is_invisible_after_restart() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    {
        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 7, b);
        // no commit
    }

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    assert!(engine.lookup_internal_id(10).is_none());
    assert!(engine.lookup_internal_id(20).is_none());
}

#[test]
fn m1_snapshot_isolation_holds() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        let _c = tx.create_node(30, 1).unwrap();
        tx.create_edge(a, 7, b);
        tx.commit().unwrap();
    }

    let a = engine.lookup_internal_id(10).unwrap();
    let c = engine.lookup_internal_id(30).unwrap();

    let old_snapshot = engine.begin_read();
    assert_eq!(old_snapshot.neighbors(a, Some(7)).count(), 1);

    {
        let mut tx = engine.begin_write();
        tx.create_edge(a, 7, c);
        tx.commit().unwrap();
    }

    // old snapshot must not see the new run
    assert_eq!(old_snapshot.neighbors(a, Some(7)).count(), 1);

    let new_snapshot = engine.begin_read();
    assert_eq!(new_snapshot.neighbors(a, Some(7)).count(), 2);
}
