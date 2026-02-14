use nervusdb::Db;
use nervusdb_query::{Params, Result, prepare};
use tempfile::tempdir;

#[test]
fn t64_match_node_scan_where_property() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create isolated nodes with properties (no edges).
    {
        let mut txn = db.begin_write();
        let q1 = prepare("CREATE (n {name: 'Linus'})").unwrap();
        q1.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        let q2 = prepare("CREATE (n {name: 'Someone'})").unwrap();
        q2.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let q = prepare("MATCH (n) WHERE n.name = 'Linus' RETURN n").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].0, "n");
}

#[test]
fn t64_match_node_scan_limit_0() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (n)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let q = prepare("MATCH (n) RETURN n LIMIT 0").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert!(rows.is_empty());
}

#[test]
fn t64_match_node_scan_delete_isolated_nodes() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let q1 = prepare("CREATE (n {name: 'Linus'})").unwrap();
        q1.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        let q2 = prepare("CREATE (n)").unwrap();
        q2.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let q = prepare("MATCH (n) WHERE n.name = 'Linus' DELETE n").unwrap();
    let mut txn = db.begin_write();
    let deleted = q
        .execute_write(&snapshot, &mut txn, &Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(deleted, 1);
}
