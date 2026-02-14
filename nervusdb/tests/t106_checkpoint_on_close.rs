use nervusdb::Db;
use nervusdb_query::{Params, Result, prepare};
use tempfile::tempdir;

#[test]
fn t106_checkpoint_on_close_rewrites_wal_when_runs_empty() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let wal_path = db.wal_path().to_path_buf();

    // Create some data (edges) + labels via query engine.
    for _ in 0..20 {
        let q = prepare("CREATE (a:User)-[:1]->(b:User)").unwrap();
        let mut txn = db.begin_write();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    // Move edges/tombstones into segments so runs become empty.
    db.compact().unwrap();

    let before = std::fs::metadata(&wal_path).unwrap().len();
    db.close().unwrap();
    let after = std::fs::metadata(&wal_path).unwrap().len();
    assert!(after < before, "expected WAL to shrink on close");

    // Re-open and ensure label + edge data are still readable.
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();
    let q = prepare("MATCH (n:User) RETURN n").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert!(!rows.is_empty());
}

#[test]
fn t106_checkpoint_on_close_does_not_rewrite_when_properties_present() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let wal_path = db.wal_path().to_path_buf();

    // CREATE with properties produces an L0 run containing properties.
    {
        let q = prepare("CREATE (n {name: 'Linus'})").unwrap();
        let mut txn = db.begin_write();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let before = std::fs::metadata(&wal_path).unwrap().len();
    db.close().unwrap();
    let after = std::fs::metadata(&wal_path).unwrap().len();
    assert_eq!(after, before, "WAL must not be rewritten when runs exist");
}
