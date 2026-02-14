use nervusdb::Db;
use nervusdb::query::Value;
use tempfile::tempdir;

#[test]
fn test_union_distinct() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t307.ndb");
    let db = Db::open(&db_path)?;

    // RETURN 1 AS x UNION RETURN 2 AS x -> 2 rows
    let query = "RETURN 1 AS x UNION RETURN 2 AS x";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 2);
    let vals: Vec<i64> = results
        .iter()
        .filter_map(|r| {
            if let Value::Int(i) = r.get("x")? {
                Some(*i)
            } else {
                None
            }
        })
        .collect();
    assert!(vals.contains(&1));
    assert!(vals.contains(&2));

    Ok(())
}

#[test]
fn test_union_dedup() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t307.ndb");
    let db = Db::open(&db_path)?;

    // RETURN 1 AS x UNION RETURN 1 AS x -> 1 row (dedup)
    let query = "RETURN 1 AS x UNION RETURN 1 AS x";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("x").unwrap(), &Value::Int(1));

    Ok(())
}

#[test]
fn test_union_all() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t307.ndb");
    let db = Db::open(&db_path)?;

    // RETURN 1 AS x UNION ALL RETURN 1 AS x -> 2 rows (no dedup)
    let query = "RETURN 1 AS x UNION ALL RETURN 1 AS x";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].get("x").unwrap(), &Value::Int(1));
    assert_eq!(results[1].get("x").unwrap(), &Value::Int(1));

    Ok(())
}

#[test]
fn test_union_with_match() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t307.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    // Create labels and nodes
    let label_a = txn.get_or_create_label("A")?;
    let label_b = txn.get_or_create_label("B")?;
    let n1 = txn.create_node(label_a.into(), 0)?;
    txn.set_node_property(n1, "id".to_string(), nervusdb::PropertyValue::Int(100))?;
    let n2 = txn.create_node(label_b.into(), 0)?;
    txn.set_node_property(n2, "id".to_string(), nervusdb::PropertyValue::Int(200))?;
    txn.commit()?;

    // MATCH (n:A) RETURN n.id AS id UNION MATCH (m:B) RETURN m.id AS id
    let query = "MATCH (n:A) RETURN n.id AS id UNION MATCH (m:B) RETURN m.id AS id";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 2);
    let vals: Vec<i64> = results
        .iter()
        .filter_map(|r| {
            if let Value::Int(i) = r.get("id")? {
                Some(*i)
            } else {
                None
            }
        })
        .collect();
    assert!(vals.contains(&100));
    assert!(vals.contains(&200));

    Ok(())
}
