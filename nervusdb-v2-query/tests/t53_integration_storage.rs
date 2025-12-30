use nervusdb_v2::{Db, GraphSnapshot};
use nervusdb_v2_query::{Params, Result, Value, prepare};
use tempfile::tempdir;

#[test]
fn t53_end_to_end_v2_storage_plus_query() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let (a, b) = {
        let mut txn = db.begin_write();
        // Setup using CREATE to ensure interning
        let q = prepare("CREATE (a {ext: 10})-[:7]->(b {ext: 20})").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();

        let snapshot = db.snapshot();
        let a = snapshot
            .nodes()
            .find(|&n| {
                snapshot.node_property(n, "ext") == Some(nervusdb_v2::PropertyValue::Int(10))
            })
            .unwrap();
        let b = snapshot
            .nodes()
            .find(|&n| {
                snapshot.node_property(n, "ext") == Some(nervusdb_v2::PropertyValue::Int(20))
            })
            .unwrap();
        (a, b)
    };

    let snap = db.snapshot();
    let q = prepare("MATCH (n)-[:7]->(m) RETURN n, m LIMIT 10").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snap, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    let cols = rows[0].columns();
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0].0, "n");
    assert_eq!(cols[1].0, "m");
    assert_eq!(cols[0].1, Value::NodeId(a));
    assert_eq!(cols[1].1, Value::NodeId(b));
}
