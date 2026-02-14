use nervusdb::Db;
use nervusdb_query::GraphSnapshot;
use nervusdb_query::{Params, Result, Value, prepare};
use tempfile::tempdir;

#[test]
fn t104_explain_return_one() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let q = prepare("EXPLAIN RETURN 1").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    let cols = rows[0].columns();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].0, "plan");
    match &cols[0].1 {
        Value::String(s) => assert!(s.contains("ReturnOne")),
        other => panic!("expected plan STRING, got {other:?}"),
    }
}

#[test]
fn t104_explain_match_node_scan() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let q = prepare("EXPLAIN MATCH (n) RETURN n").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    let cols = rows[0].columns();
    match &cols[0].1 {
        Value::String(s) => assert!(s.contains("NodeScan")),
        other => panic!("expected plan STRING, got {other:?}"),
    }
}

#[test]
fn t104_explain_create_does_not_write() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let before = db.snapshot().nodes().count();

    let q = prepare("EXPLAIN CREATE (n)").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&db.snapshot(), &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 1);

    let after = db.snapshot().nodes().count();
    assert_eq!(before, after);
}
