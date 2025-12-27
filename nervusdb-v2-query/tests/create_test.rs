use nervusdb_v2::Db;
use nervusdb_v2_api::{EdgeKey, GraphSnapshot, InternalNodeId, RelTypeId};
use nervusdb_v2_query::prepare;
use tempfile::tempdir;

fn get_snapshot(db: &Db) -> impl GraphSnapshot + '_ {
    struct DbSnapshot<'a> {
        db: &'a Db,
    }

    impl<'a> GraphSnapshot for DbSnapshot<'a> {
        type Neighbors<'b>
            = std::vec::IntoIter<EdgeKey>
        where
            Self: 'b;

        fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
            let snapshot = self.db.begin_read();
            snapshot
                .neighbors(src, rel)
                .map(|e| EdgeKey {
                    src: e.src,
                    rel: e.rel,
                    dst: e.dst,
                })
                .collect::<Vec<_>>()
                .into_iter()
        }

        fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
            // For DELETE tests, we need to iterate over existing nodes
            // This is a simplified implementation - in production, ReadTxn should implement GraphSnapshot
            let snapshot = self.db.begin_read();
            // Since ReadTxn doesn't have nodes(), we use neighbors to find nodes
            // A node that has no outgoing edges might be missed, but for tests it should work
            let mut nodes: Vec<InternalNodeId> = Vec::new();
            // Collect up to 100 node IDs by probing
            for i in 0..100u32 {
                let neighbors: Vec<_> = snapshot.neighbors(i, None).collect();
                if !neighbors.is_empty() {
                    nodes.push(i);
                }
            }
            Box::new(nodes.into_iter())
        }

        fn is_tombstoned_node(&self, _iid: InternalNodeId) -> bool {
            false
        }
    }

    DbSnapshot { db }
}

#[test]
fn test_create_single_node() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = get_snapshot(&db);

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
    let snapshot = get_snapshot(&db);

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
    let snapshot = get_snapshot(&db);

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
    let snapshot = get_snapshot(&db);

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
    let snapshot = get_snapshot(&db);

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
    let snapshot = get_snapshot(&db);

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
    let snapshot = get_snapshot(&db);

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    let count = create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 3);

    // Now delete
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
    let snapshot = get_snapshot(&db);

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Delete the second node
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
    let snapshot = get_snapshot(&db);

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // DETACH DELETE
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
    let snapshot = get_snapshot(&db);

    // Create a pattern: a -> b
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // DETACH DELETE with MATCH
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
    let snapshot = get_snapshot(&db);

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
    let delete_query = prepare("MATCH (a)-[:1]->(a) DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(deleted, 1);
}
