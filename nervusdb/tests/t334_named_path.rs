use nervusdb::Db;
use nervusdb_query::{Params, Value, prepare};
use tempfile::tempdir;

#[test]
fn test_named_path_zero_length_is_not_null() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let create = prepare("CREATE ()").unwrap();
    let mut txn = db.begin_write();
    create
        .execute_write(&snapshot, &mut txn, &Params::new())
        .unwrap();
    txn.commit().unwrap();

    let query = prepare("MATCH p = (a) RETURN p").unwrap();
    let snapshot = db.snapshot();
    let rows: Vec<_> = query.execute_streaming(&snapshot, &Params::new()).collect();
    assert_eq!(rows.len(), 1);

    let row = rows[0].as_ref().unwrap();
    let row = row.reify(&snapshot).unwrap_or_else(|_| row.clone());
    let p_value = row
        .columns()
        .iter()
        .find(|(name, _)| name == "p")
        .map(|(_, value)| value)
        .expect("column p should exist");

    assert!(
        !matches!(p_value, Value::Null),
        "named zero-length path should not be null"
    );
}

#[test]
fn test_named_incoming_path_matches_existing_edge() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let create = prepare("CREATE (:Label1)<-[:TYPE]-(:Label2)").unwrap();
    let mut txn = db.begin_write();
    create
        .execute_write(&snapshot, &mut txn, &Params::new())
        .unwrap();
    txn.commit().unwrap();

    let query = prepare("MATCH p = (a:Label1)<--(:Label2) RETURN p").unwrap();
    let snapshot = db.snapshot();
    let rows: Vec<_> = query.execute_streaming(&snapshot, &Params::new()).collect();

    assert_eq!(rows.len(), 1, "incoming named path should match one row");
    assert!(
        rows[0].is_ok(),
        "query row should not be an execution error"
    );
}

#[test]
fn test_parse_named_path_with_undirected_fixed_varlen() {
    let query =
        "MATCH topRoute = (:Start)<-[:CONNECTED_TO]-()-[:CONNECTED_TO*3..3]-(:End) RETURN topRoute";
    let prepared = prepare(query);
    assert!(
        prepared.is_ok(),
        "query should parse, got: {:?}",
        prepared.err()
    );
}

#[test]
fn test_named_path_undirected_fixed_varlen_returns_four_routes() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let seed = prepare(
        "CREATE (db1:Start), (db2:End), (mid), (other)          CREATE (mid)-[:CONNECTED_TO]->(db1),                 (mid)-[:CONNECTED_TO]->(db2),                 (mid)-[:CONNECTED_TO]->(db2),                 (mid)-[:CONNECTED_TO]->(other),                 (mid)-[:CONNECTED_TO]->(other)",
    )
    .unwrap();
    let mut txn = db.begin_write();
    seed.execute_write(&snapshot, &mut txn, &Params::new())
        .unwrap();
    txn.commit().unwrap();

    let query = prepare(
        "MATCH topRoute = (:Start)<-[:CONNECTED_TO]-()-[:CONNECTED_TO*3..3]-(:End) RETURN topRoute",
    )
    .unwrap();

    let snapshot = db.snapshot();
    let rows: Vec<_> = query.execute_streaming(&snapshot, &Params::new()).collect();

    assert_eq!(
        rows.len(),
        4,
        "expected 4 routes from parallel edge combinations"
    );
    assert!(rows.iter().all(|r| r.is_ok()), "all rows should succeed");
}
