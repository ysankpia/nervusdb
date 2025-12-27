use nervusdb_v2::Db;
use nervusdb_v2_query::prepare;
use tempfile::tempdir;

fn get_snapshot(db: &Db) -> impl nervusdb_v2_api::GraphSnapshot + '_ {
    struct DbSnapshot<'a> {
        db: &'a Db,
    }

    impl<'a> nervusdb_v2_api::GraphSnapshot for DbSnapshot<'a> {
        type Neighbors<'b> = std::vec::IntoIter<nervusdb_v2_api::EdgeKey> where Self: 'b;

        fn neighbors(&self, src: nervusdb_v2_api::InternalNodeId, rel: Option<nervusdb_v2_api::RelTypeId>) -> Self::Neighbors<'_> {
            let snapshot = self.db.begin_read();
            snapshot.neighbors(src, rel).map(|e| nervusdb_v2_api::EdgeKey {
                src: e.src,
                rel: e.rel,
                dst: e.dst,
            }).collect::<Vec<_>>().into_iter()
        }

        fn nodes(&self) -> Box<dyn Iterator<Item = nervusdb_v2_api::InternalNodeId> + '_> {
            Box::new(std::iter::empty())
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
    let count = query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
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
    let count = query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
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
    let count = query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
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
    let count = query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 3);
}

#[test]
fn test_create_two_nodes() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = get_snapshot(&db);

    let query = prepare("CREATE (x) CREATE (y)").unwrap();
    let mut txn = db.begin_write();
    let count = query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 2);
}

#[test]
fn test_delete_basic() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = get_snapshot(&db);

    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    let count = create_query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 3);

    let delete_query = prepare("MATCH (a)-[:1]->(b) DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
    txn.commit().unwrap();

    assert_eq!(deleted, 1);
}

#[test]
fn test_delete_with_where() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = get_snapshot(&db);

    let create_query = prepare("CREATE (a {name: 'keep'})-[:1]->(b {name: 'delete'})").unwrap();
    let mut txn = db.begin_write();
    create_query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
    txn.commit().unwrap();

    let delete_query = prepare("MATCH (a)-[:1]->(b) WHERE b.name = 'delete' DELETE b").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
    txn.commit().unwrap();

    assert_eq!(deleted, 1);
}

#[test]
fn test_detach_delete() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = get_snapshot(&db);

    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
    txn.commit().unwrap();

    let delete_query = prepare("MATCH (a)-[:1]->(b) DETACH DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query.execute_write(&snapshot, &mut txn, &nervusdb_v2_query::Params::new()).unwrap();
    txn.commit().unwrap();

    assert_eq!(deleted, 2);
}
