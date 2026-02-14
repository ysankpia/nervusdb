use nervusdb::Db;
use nervusdb::query::{Row, Value};
use std::collections::BTreeMap;
use tempfile::tempdir;

fn execute_write(db: &Db, query: &str) -> nervusdb::Result<()> {
    let snapshot = db.snapshot();
    let mut txn = db.begin_write();
    let prepared = nervusdb::query::prepare(query)?;
    prepared.execute_write(&snapshot, &mut txn, &Default::default())?;
    txn.commit()?;
    Ok(())
}

fn execute_rows(db: &Db, query: &str) -> nervusdb::Result<Vec<Row>> {
    let snapshot = db.snapshot();
    let prepared = nervusdb::query::prepare(query)?;
    prepared
        .execute_streaming(&snapshot, &Default::default())
        .map(|row| row.map_err(nervusdb::Error::from))
        .collect::<nervusdb::Result<Vec<_>>>()
}

fn get_col<'a>(row: &'a Row, name: &str) -> &'a Value {
    row.columns()
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("missing column {name} in row: {row:?}"))
}

#[test]
fn test_graph5_relationship_label_expression_matches_rel_type() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t335_rel_label_expr.ndb"))?;

    execute_write(
        &db,
        "CREATE ()-[:T1]->(), ()-[:T2]->(), ()-[:t2]->(), (:T2)-[:T3]->(), ()-[:T4]->(:T2)",
    )?;

    let rows = execute_rows(&db, "MATCH ()-[r]->() RETURN type(r) AS t, r:T2 AS result")?;
    assert_eq!(rows.len(), 5);

    let mut got = BTreeMap::new();
    for row in rows {
        let t = match get_col(&row, "t") {
            Value::String(s) => s.clone(),
            other => panic!("t should be string, got {other:?}"),
        };
        let result = match get_col(&row, "result") {
            Value::Bool(b) => *b,
            other => panic!("result should be bool, got {other:?}"),
        };
        got.insert(t, result);
    }

    assert_eq!(got.get("T1"), Some(&false));
    assert_eq!(got.get("T2"), Some(&true));
    assert_eq!(got.get("t2"), Some(&false));
    assert_eq!(got.get("T3"), Some(&false));
    assert_eq!(got.get("T4"), Some(&false));
    Ok(())
}

#[test]
fn test_graph5_conjunctive_label_expression_uses_base_operand_for_each_label()
-> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t335_node_label_expr.ndb"))?;

    execute_write(
        &db,
        "CREATE (:A:B:C), (:A:B), (:A:C), (:B:C), (:A), (:B), (:C), ()",
    )?;

    let rows = execute_rows(&db, "MATCH (a) RETURN a:A:B AS result")?;
    let true_count = rows
        .iter()
        .filter(|row| matches!(get_col(row, "result"), Value::Bool(true)))
        .count();
    assert_eq!(true_count, 2, "a:A:B should match exactly two nodes");

    let dir2 = tempdir()?;
    let db2 = Db::open(dir2.path().join("t335_node_label_expr_repeated.ndb"))?;
    execute_write(&db2, "CREATE (:A:B), (:A:C), (:B:C), (:A), (:B), (:C), ()")?;
    let repeated = execute_rows(&db2, "MATCH (a) WHERE a:C:A:A:C RETURN count(*) AS c")?;
    assert_eq!(repeated.len(), 1);
    assert_eq!(get_col(&repeated[0], "c"), &Value::Int(1));

    Ok(())
}

#[test]
fn test_graph5_null_label_expression_keeps_unquoted_column_alias() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t335_null_label_expr.ndb"))?;

    execute_write(&db, "CREATE (s:Single)")?;
    let rows = execute_rows(
        &db,
        "MATCH (n:Single) OPTIONAL MATCH (n)-[r:TYPE]-(m) RETURN m:TYPE",
    )?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns().len(), 1);
    assert_eq!(rows[0].columns()[0].0, "m:TYPE");
    assert_eq!(rows[0].columns()[0].1, Value::Null);

    Ok(())
}
