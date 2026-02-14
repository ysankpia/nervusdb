use nervusdb::Db;
use nervusdb::query::Value;
use nervusdb_api::GraphSnapshot;
use tempfile::tempdir;

#[test]

fn test_exists_with_edge() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t309.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let rel_type = txn.get_or_create_rel_type("KNOWS")?;
    let n1 = txn.create_node(1, label)?;
    txn.set_node_property(
        n1,
        "name".to_string(),
        nervusdb::PropertyValue::String("Alice".to_string()),
    )?;
    txn.commit()?;

    let mut txn = db.begin_write();
    let n2 = txn.create_node(2, label)?;
    txn.set_node_property(
        n2,
        "name".to_string(),
        nervusdb::PropertyValue::String("Bob".to_string()),
    )?;
    txn.create_edge(n1, rel_type, n2);
    txn.commit()?;

    // MATCH (n:Person) WHERE EXISTS ((n)-[:KNOWS]->()) RETURN n.name
    let query = "MATCH (n:Person) WHERE EXISTS { (n)-[:KNOWS]->() } RETURN n.name AS name";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    // Only Alice has outgoing KNOWS edge
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("name").unwrap(),
        &Value::String("Alice".to_string())
    );

    Ok(())
}

#[test]

fn test_exists_no_edge() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t309.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let n1 = txn.create_node(1, label)?;
    txn.set_node_property(
        n1,
        "name".to_string(),
        nervusdb::PropertyValue::String("Lonely".to_string()),
    )?;
    txn.commit()?;

    // No edges, EXISTS should filter out everyone
    let query = "MATCH (n:Person) WHERE EXISTS { (n)-[:KNOWS]->() } RETURN n.name AS name";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 0);

    Ok(())
}

#[test]

fn test_not_exists() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t309.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let rel_type = txn.get_or_create_rel_type("KNOWS")?;
    let n1 = txn.create_node(1, label)?;
    txn.set_node_property(
        n1,
        "name".to_string(),
        nervusdb::PropertyValue::String("Alice".to_string()),
    )?;
    txn.commit()?;

    let mut txn = db.begin_write();
    let n2 = txn.create_node(2, label)?;
    txn.set_node_property(
        n2,
        "name".to_string(),
        nervusdb::PropertyValue::String("Bob".to_string()),
    )?;
    txn.create_edge(n1, rel_type, n2);
    txn.commit()?;

    // NOT EXISTS - Bob doesn't have outgoing KNOWS
    let query = "MATCH (n:Person) WHERE NOT EXISTS { (n)-[:KNOWS]->() } RETURN n.name AS name";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    // Only Bob has no outgoing KNOWS edge
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("name").unwrap(),
        &Value::String("Bob".to_string())
    );

    Ok(())
}

#[test]
fn test_exists_pattern_with_where_clause() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t309.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let a = txn.get_or_create_label("A")?;
    let b = txn.get_or_create_label("B")?;
    let rel_type = txn.get_or_create_rel_type("R")?;

    let n1 = txn.create_node(1, a)?;
    txn.set_node_property(n1, "prop".to_string(), nervusdb::PropertyValue::Int(1))?;
    let n2 = txn.create_node(2, b)?;
    txn.set_node_property(n2, "prop".to_string(), nervusdb::PropertyValue::Int(1))?;
    let n3 = txn.create_node(3, b)?;
    txn.set_node_property(n3, "prop".to_string(), nervusdb::PropertyValue::Int(2))?;
    assert_ne!(n2, n3);

    txn.create_edge(n1, rel_type, n2);
    txn.create_edge(n1, rel_type, n3);
    txn.commit()?;

    let debug_snapshot = db.snapshot();
    assert_eq!(
        debug_snapshot.node_property(n2, "prop"),
        Some(nervusdb::PropertyValue::Int(1))
    );
    assert_eq!(
        debug_snapshot.node_property(n3, "prop"),
        Some(nervusdb::PropertyValue::Int(2))
    );
    let outgoing: Vec<_> = debug_snapshot.neighbors(n1, Some(rel_type)).collect();
    assert_eq!(outgoing.len(), 2);
    assert!(outgoing.iter().any(|edge| edge.dst == n2));
    assert!(outgoing.iter().any(|edge| edge.dst == n3));

    let plain_match_query = r#"
        MATCH (n)
        MATCH (n)-->(m)
        RETURN n.prop AS n_prop, m.prop AS m_prop
    "#;
    let plain_match_prep = nervusdb::query::prepare(plain_match_query)?;
    let plain_snapshot = db.snapshot();
    let plain_results: Vec<_> = plain_match_prep
        .execute_streaming(&plain_snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(plain_results.len(), 2);
    let plain_pairs: Vec<(Value, Value)> = plain_results
        .iter()
        .map(|row| {
            (
                row.get("n_prop").cloned().unwrap_or(Value::Null),
                row.get("m_prop").cloned().unwrap_or(Value::Null),
            )
        })
        .collect();
    assert!(
        plain_pairs.contains(&(Value::Int(1), Value::Int(1))),
        "plain_pairs={plain_pairs:?}"
    );

    let direct_query = r#"
        MATCH (n)
        MATCH (n)-->(m)
        WHERE n.prop = m.prop
        RETURN n.prop AS prop
    "#;
    let direct_prep = nervusdb::query::prepare(direct_query)?;
    let direct_snapshot = db.snapshot();
    let direct_results: Vec<_> = direct_prep
        .execute_streaming(&direct_snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(direct_results.len(), 1);
    assert_eq!(direct_results[0].get("prop").unwrap(), &Value::Int(1));

    let query = r#"
        MATCH (n)
        WHERE EXISTS {
          (n)-->(m) WHERE n.prop = m.prop
        }
        RETURN n.prop AS prop
    "#;
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("prop").unwrap(), &Value::Int(1));

    Ok(())
}

#[test]
fn test_exists_full_subquery_match_return() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t309.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let a = txn.get_or_create_label("A")?;
    let b = txn.get_or_create_label("B")?;
    let rel_type = txn.get_or_create_rel_type("R")?;

    let n1 = txn.create_node(1, a)?;
    txn.set_node_property(n1, "prop".to_string(), nervusdb::PropertyValue::Int(1))?;
    let n2 = txn.create_node(2, b)?;
    txn.set_node_property(n2, "prop".to_string(), nervusdb::PropertyValue::Int(1))?;
    txn.create_edge(n1, rel_type, n2);
    txn.commit()?;

    let query = r#"
        MATCH (n)
        WHERE EXISTS {
          MATCH (n)-->()
          RETURN true
        }
        RETURN n.prop AS prop
    "#;
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("prop").unwrap(), &Value::Int(1));

    Ok(())
}
