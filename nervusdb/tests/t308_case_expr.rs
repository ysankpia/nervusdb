use nervusdb::Db;
use nervusdb::query::Value;
use tempfile::tempdir;

#[test]
fn test_case_when_then() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t308.ndb");
    let db = Db::open(&db_path)?;

    // RETURN CASE WHEN true THEN 1 ELSE 0 END AS result
    let query = "RETURN CASE WHEN true THEN 1 ELSE 0 END AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(1));

    Ok(())
}

#[test]
fn test_case_else() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t308.ndb");
    let db = Db::open(&db_path)?;

    // RETURN CASE WHEN false THEN 1 ELSE 99 END AS result
    let query = "RETURN CASE WHEN false THEN 1 ELSE 99 END AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(99));

    Ok(())
}

#[test]
fn test_case_multiple_when() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t308.ndb");
    let db = Db::open(&db_path)?;

    // RETURN CASE WHEN 1=2 THEN 'a' WHEN 2=2 THEN 'b' ELSE 'c' END AS result
    let query = "RETURN CASE WHEN 1=2 THEN 'a' WHEN 2=2 THEN 'b' ELSE 'c' END AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("result").unwrap(),
        &Value::String("b".to_string())
    );

    Ok(())
}

#[test]
fn test_case_with_property() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t308.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let n = txn.create_node(label.into(), 0)?;
    txn.set_node_property(n, "age".to_string(), nervusdb::PropertyValue::Int(25))?;
    txn.commit()?;

    // MATCH (n:Person) RETURN CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END AS status
    let query =
        "MATCH (n:Person) RETURN CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END AS status";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("status").unwrap(),
        &Value::String("adult".to_string())
    );

    Ok(())
}

#[test]
fn test_simple_case_expression() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t308_simple.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN CASE 2 WHEN 1 THEN 'a' WHEN 2 THEN 'b' ELSE 'c' END AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("result").unwrap(),
        &Value::String("b".to_string())
    );

    Ok(())
}
