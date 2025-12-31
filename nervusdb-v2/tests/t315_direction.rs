use nervusdb_v2::Db;
use nervusdb_v2::GraphSnapshot;
use nervusdb_v2::query::Value;
use tempfile::tempdir;

#[test]
fn test_incoming_relationship() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t315.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let rel_type = txn.get_or_create_rel_type("KNOWS")?;

    // Alice -> Bob
    let alice = txn.create_node(1, label)?;
    txn.set_node_property(
        alice,
        "name".to_string(),
        nervusdb_v2::PropertyValue::String("Alice".to_string()),
    )?;

    let bob = txn.create_node(2, label)?;
    txn.set_node_property(
        bob,
        "name".to_string(),
        nervusdb_v2::PropertyValue::String("Bob".to_string()),
    )?;

    txn.create_edge(alice, bob, rel_type.into());
    txn.commit()?;

    // Query: MATCH (b:Person {name: 'Bob'})<-[:KNOWS]-(a) RETURN a.name
    // Should find Alice
    let query = "MATCH (b:Person {name: 'Bob'})<-[:KNOWS]-(a) RETURN a.name";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("a.name").unwrap(),
        &Value::String("Alice".to_string())
    );

    Ok(())
}

#[test]
fn test_undirected_relationship() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t315.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let rel_type = txn.get_or_create_rel_type("KNOWS")?;

    // Alice -> Bob
    let alice = txn.create_node(1, label)?;
    txn.set_node_property(
        alice,
        "name".to_string(),
        nervusdb_v2::PropertyValue::String("Alice".to_string()),
    )?;

    let bob = txn.create_node(2, label)?;
    txn.set_node_property(
        bob,
        "name".to_string(),
        nervusdb_v2::PropertyValue::String("Bob".to_string()),
    )?;

    txn.create_edge(alice, bob, rel_type.into());
    txn.commit()?;

    // Query 1: MATCH (a {name: 'Alice'})-[:KNOWS]-(b) RETURN b.name
    // Should find Bob (outgoing)
    let query1 = "MATCH (a:Person {name: 'Alice'})-[:KNOWS]-(b) RETURN b.name";
    let prep1 = nervusdb_v2::query::prepare(query1)?;
    let snapshot = db.snapshot();
    let res1: Vec<_> = prep1
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(res1.len(), 1);
    assert_eq!(
        res1[0].get("b.name").unwrap(),
        &Value::String("Bob".to_string())
    );

    // Query 2: MATCH (b {name: 'Bob'})-[:KNOWS]-(a) RETURN a.name
    // Should find Alice (incoming)
    let query2 = "MATCH (b:Person {name: 'Bob'})-[:KNOWS]-(a) RETURN a.name";
    let prep2 = nervusdb_v2::query::prepare(query2)?;
    let res2: Vec<_> = prep2
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(res2.len(), 1);
    assert_eq!(
        res2[0].get("a.name").unwrap(),
        &Value::String("Alice".to_string())
    );

    Ok(())
}
