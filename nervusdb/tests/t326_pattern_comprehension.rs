use nervusdb::Db;
use nervusdb::query::{Row, Value};
use tempfile::tempdir;

fn execute_write(db: &Db, query: &str) -> nervusdb::Result<()> {
    let snapshot = db.snapshot();
    let mut txn = db.begin_write();
    let prepared = nervusdb::query::prepare(query)?;
    prepared.execute_write(&snapshot, &mut txn, &Default::default())?;
    txn.commit()?;
    Ok(())
}

fn get_col<'a>(row: &'a Row, name: &str) -> &'a Value {
    row.columns()
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("missing column {name} in row: {row:?}"))
}

#[test]
fn test_pattern_comprehension_returns_paths_per_row() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t326_pattern_comp_paths.ndb");
    let db = Db::open(&db_path)?;

    execute_write(
        &db,
        "CREATE (a:A), (b:B) CREATE (a)-[:T]->(b), (b)-[:T]->(:C)",
    )?;

    let snapshot = db.snapshot();
    let query = nervusdb::query::prepare("MATCH (n) RETURN [p = (n)-->() | p] AS list")?;
    let rows: Vec<Row> = query
        .execute_streaming(&snapshot, &Default::default())
        .map(|r| r.map_err(nervusdb::Error::from))
        .collect::<nervusdb::Result<Vec<_>>>()?;

    assert_eq!(rows.len(), 3, "expected one row per node");

    let mut lengths = Vec::new();
    for row in &rows {
        let list = match get_col(row, "list") {
            Value::List(items) => items,
            other => panic!("list should be List, got {other:?}"),
        };
        if let Some(first) = list.first() {
            assert!(
                matches!(first, Value::Path(_) | Value::ReifiedPath(_)),
                "list element should be path-like, got {first:?}"
            );
        }
        lengths.push(list.len());
    }
    lengths.sort_unstable();
    assert_eq!(lengths, vec![0, 1, 1]);

    Ok(())
}

#[test]
fn test_pattern_comprehension_projects_relationship_variable() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t326_pattern_comp_relvar.ndb");
    let db = Db::open(&db_path)?;

    execute_write(
        &db,
        "CREATE (a), (b), (c) CREATE (a)-[:T {name:'val'}]->(b), (b)-[:T]->(c)",
    )?;

    let snapshot = db.snapshot();
    let query = nervusdb::query::prepare("MATCH (n) RETURN [(n)-[r:T]->() | r.name] AS list")?;
    let rows: Vec<Row> = query
        .execute_streaming(&snapshot, &Default::default())
        .map(|r| r.map_err(nervusdb::Error::from))
        .collect::<nervusdb::Result<Vec<_>>>()?;

    assert_eq!(rows.len(), 3, "expected one row per node");

    let mut fingerprints = Vec::new();
    for row in &rows {
        let list = match get_col(row, "list") {
            Value::List(items) => items,
            other => panic!("list should be List, got {other:?}"),
        };
        let fp = match list.as_slice() {
            [] => "empty".to_string(),
            [Value::String(s)] => s.clone(),
            [Value::Null] => "null".to_string(),
            other => panic!("unexpected projected list payload: {other:?}"),
        };
        fingerprints.push(fp);
    }
    fingerprints.sort();
    assert_eq!(fingerprints, vec!["empty", "null", "val"]);

    Ok(())
}
