use nervusdb_v2::Db;
use nervusdb_v2_query::{Params, Result, Value, prepare};
use tempfile::tempdir;

#[test]
fn t52_prepare_and_execute_return_one() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let q = prepare("RETURN 1").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::Int(1));
}

#[test]
fn t52_prepare_and_execute_match_out() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Setup: (n)-[:7]->(m)
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (n)-[:7]->(m)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let q = prepare("MATCH (n)-[:7]->(m) RETURN n, m LIMIT 10").unwrap();
    let snapshot = db.snapshot();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 1);

    let cols = rows[0].columns();
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0].0, "n");
    assert_eq!(cols[1].0, "m");
}
