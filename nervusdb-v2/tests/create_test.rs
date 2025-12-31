use nervusdb_v2::{Db, GraphSnapshot};
use nervusdb_v2_query::prepare;
use tempfile::tempdir;

#[test]
fn test_create_single_node() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (n)").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn test_create_node_with_properties() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (n {name: 'Alice', age: 30})").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn test_create_relationship() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 3);
}

#[test]
fn test_create_relationship_with_properties() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (a {name: 'A'})-[:1 {weight: 2.5}]->(b {name: 'B'})").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 3);
}

#[test]
fn test_create_multiple_nodes() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // M3: Create nodes one at a time (no comma-separated list)
    let query = prepare("CREATE (a)").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 1);

    // Create second node
    let query = prepare("CREATE (b)").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_create_complex_pattern() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (a {x: 1})-[:1]->(b {y: 2})").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 3);
}

#[test]
fn test_delete_basic() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    let count = create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 3);

    // Now delete
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(b) DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(deleted, 1);
}

#[test]
fn test_delete_second_node() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Delete the second node
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(b) DELETE b").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(deleted, 1);
}

#[test]
fn test_detach_delete() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // DETACH DELETE
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(b) DETACH DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Should delete edge + node = 2
    assert_eq!(deleted, 2);
}

#[test]
fn test_detach_delete_standalone() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create a pattern: a -> b
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // DETACH DELETE with MATCH
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(b) DETACH DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // a has 1 edge = 2 deletions (edge + node)
    assert_eq!(deleted, 2);
}

#[test]
fn test_delete_multiple_nodes() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create two disconnected nodes
    let create_query = prepare("CREATE (a)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    let create_query = prepare("CREATE (b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Delete first node by matching with a self-loop (create one first)
    let create_query = prepare("CREATE (a)-[:1]->(a)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Delete node with self-loop
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(a) DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(deleted, 1);
}

#[test]
fn test_delete_edge_variable() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create a pattern: a -> b
    {
        let snapshot = db.snapshot();
        let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
        let mut txn = db.begin_write();
        create_query
            .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    // Delete edge by binding it to a variable.
    {
        let snapshot = db.snapshot();
        let delete_query = prepare("MATCH (a)-[r:1]->(b) DELETE r").unwrap();
        let mut txn = db.begin_write();
        let deleted = delete_query
            .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(deleted, 1);
    }

    // Verify there are no edges left.
    let snap = db.snapshot();
    let nodes: Vec<_> = snap.nodes().collect();
    let mut total_edges = 0usize;
    for &n in &nodes {
        total_edges += snap.neighbors(n, None).count();
    }
    assert_eq!(total_edges, 0);
}
