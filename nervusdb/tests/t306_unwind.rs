use nervusdb::Db;
use nervusdb::query::Value;
use tempfile::tempdir;

#[test]
fn test_unwind_list() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t306.ndb");
    let db = Db::open(&db_path)?;

    // UNWIND [1, 2, 3] AS x RETURN x
    let query = "UNWIND [1, 2, 3] AS x RETURN x";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].get("x").unwrap(), &Value::Int(1));
    assert_eq!(results[1].get("x").unwrap(), &Value::Int(2));
    assert_eq!(results[2].get("x").unwrap(), &Value::Int(3));

    Ok(())
}

#[test]
fn test_unwind_empty_list() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t306.ndb");
    let db = Db::open(&db_path)?;

    // UNWIND [] AS x RETURN x
    let query = "UNWIND [] AS x RETURN x";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 0);

    Ok(())
}

#[test]
fn test_unwind_null() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t306.ndb");
    let db = Db::open(&db_path)?;

    // UNWIND null AS x RETURN x
    let query = "UNWIND null AS x RETURN x";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 0);

    Ok(())
}

#[test]
fn test_unwind_scalar() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t306.ndb");
    let db = Db::open(&db_path)?;

    // UNWIND 42 AS x RETURN x
    // Scalar should be treated as single-element list
    let query = "UNWIND 42 AS x RETURN x";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("x").unwrap(), &Value::Int(42));

    Ok(())
}

#[test]
fn test_unwind_chaining() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t306.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();
    // Create node with list property
    let node = txn.create_node(1, 0)?;
    txn.set_node_property(
        node,
        "nums".to_string(),
        nervusdb::PropertyValue::List(vec![
            nervusdb::PropertyValue::Int(10),
            nervusdb::PropertyValue::Int(20),
        ]),
    )?;
    txn.commit()?;

    // MATCH (n) UNWIND n.nums AS num RETURN num
    let query = "MATCH (n) UNWIND n.nums AS num RETURN num";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    // Should get 2 rows: 10, 20
    assert_eq!(results.len(), 2);
    // Ordering is not guaranteed without ORDER BY, but likely stable here
    let vals: Vec<i64> = results
        .iter()
        .map(|r| {
            if let Value::Int(i) = r.get("num").unwrap() {
                *i
            } else {
                0
            }
        })
        .collect();

    assert!(vals.contains(&10));
    assert!(vals.contains(&20));

    Ok(())
}
