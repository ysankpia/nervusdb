use nervusdb::Db;
use nervusdb_query::{Params, Value, prepare};
use tempfile::tempdir;

fn exec_write(db: &Db, cypher: &str) -> nervusdb_query::Result<u32> {
    let prepared = prepare(cypher)?;
    let snapshot = db.snapshot();
    let mut txn = db.begin_write();
    let affected = prepared.execute_write(&snapshot, &mut txn, &Params::new())?;
    txn.commit().unwrap();
    Ok(affected)
}

#[test]
fn query_streaming_rejects_write_plans() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("parity.ndb")).unwrap();
    let snapshot = db.snapshot();
    let prepared = prepare("CREATE (:QOnly)").unwrap();
    let params = Params::new();

    let mut iter = prepared.execute_streaming(&snapshot, &params);
    let err = iter
        .next()
        .expect("expected an error row for write plan")
        .expect_err("query() path must reject write plan");

    assert!(
        err.to_string().contains("execute_write"),
        "unexpected error: {err}"
    );
}

#[test]
fn delete_connected_node_requires_detach_delete() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("parity.ndb")).unwrap();

    exec_write(&db, "CREATE (:D {id: 1})-[:R]->(:D {id: 2})").unwrap();

    let prepared = prepare("MATCH (n:D {id: 1}) DELETE n").unwrap();
    let snapshot = db.snapshot();
    let mut txn = db.begin_write();
    let err = prepared
        .execute_write(&snapshot, &mut txn, &Params::new())
        .expect_err("DELETE connected node should require DETACH");
    assert!(
        err.to_string().contains("DETACH DELETE"),
        "unexpected error: {err}"
    );
}

#[test]
fn detach_delete_connected_node_succeeds() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("parity.ndb")).unwrap();

    exec_write(&db, "CREATE (:DD {id: 1})-[:R]->(:DD {id: 2})").unwrap();
    exec_write(&db, "MATCH (n:DD {id: 1}) DETACH DELETE n").unwrap();

    let snapshot = db.snapshot();
    let rows: Vec<_> = prepare("MATCH (n:DD) RETURN n.id AS id ORDER BY id")
        .unwrap()
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("id"), Some(&Value::Int(2)));
}
