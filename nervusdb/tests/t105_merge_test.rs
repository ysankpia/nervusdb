use nervusdb::Db;
use nervusdb_query::{Params, Result, prepare};
use tempfile::tempdir;

#[test]
fn t105_merge_node_is_idempotent() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let q = prepare("MERGE (n {name: 'Alice'})").unwrap();
    {
        let mut txn = db.begin_write();
        let created = q
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(created, 1);
    }
    {
        let mut txn = db.begin_write();
        let created = q
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(created, 0);
    }

    let snapshot = db.snapshot();
    let q = prepare("MATCH (n) WHERE n.name = 'Alice' RETURN n").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn t105_merge_single_hop_is_idempotent() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let q = prepare("MERGE (a {name: 'A'})-[:1]->(b {name: 'B'})").unwrap();
    {
        let mut txn = db.begin_write();
        let created = q
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(created, 3);
    }
    {
        let mut txn = db.begin_write();
        let created = q
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(created, 0);
    }
}
