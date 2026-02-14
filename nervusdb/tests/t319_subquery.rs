use nervusdb::query::Value;
use nervusdb::{Db, PropertyValue};
use tempfile::tempdir;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn test_uncorrelated_subquery() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;

    let snapshot = db.snapshot();

    // CALL { RETURN 1 AS x } RETURN x
    let query = "CALL { RETURN 1 AS x } RETURN x";
    let prep = nervusdb::query::prepare(query)?;
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb::query::error::Result<Vec<_>>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("x").unwrap(), &Value::Int(1));

    Ok(())
}

#[test]
fn test_correlated_subquery_with_projection() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let node = txn.create_node(1, 0)?;
    txn.set_node_property(node, "val".to_string(), PropertyValue::Int(42))?;
    txn.commit()?;

    let snapshot = db.snapshot();

    // MATCH (n) CALL { WITH n RETURN n.val AS v } RETURN v
    let query = "MATCH (n) CALL { WITH n RETURN n.val AS v } RETURN v";
    let prep = nervusdb::query::prepare(query)?;
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb::query::error::Result<Vec<_>>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("v").unwrap(), &Value::Int(42));

    Ok(())
}

#[test]
fn test_correlated_aggregation_subquery() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let l_person = txn.get_or_create_label("Person")?;
    let r_knows = txn.get_or_create_rel_type("KNOWS")?;

    let alice = txn.create_node(1, l_person)?;
    let bob = txn.create_node(2, l_person)?;
    let carol = txn.create_node(3, l_person)?;

    // Alice -> Bob, Alice -> Carol
    txn.create_edge(alice, r_knows, bob);
    txn.create_edge(alice, r_knows, carol);

    // Bob -> Carol
    txn.create_edge(bob, r_knows, carol);

    txn.set_node_property(
        alice,
        "name".to_string(),
        PropertyValue::String("Alice".into()),
    )?;
    txn.set_node_property(bob, "name".to_string(), PropertyValue::String("Bob".into()))?;

    txn.commit()?;

    let snapshot = db.snapshot();

    // Calculate degree for each person
    // MATCH (p:Person) CALL { WITH p MATCH (p)-[:KNOWS]->(friend) RETURN count(friend) AS deg } RETURN p.name, deg
    let query = "MATCH (p:Person) CALL { WITH p MATCH (p)-[:KNOWS]->(friend) RETURN count(friend) AS deg } RETURN p.name, deg ORDER BY p.name";
    let prep = nervusdb::query::prepare(query)?;
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb::query::error::Result<Vec<_>>>()?;

    // Alice: 2 friends
    // Bob: 1 friend
    // Carol: 0 friends
    //
    // Cypher aggregate semantics require one row for aggregation without grouping keys,
    // even when the inner MATCH has no rows.
    assert_eq!(results.len(), 3);

    // Since we ordered by p.name
    // Alice
    assert_eq!(
        results[0].get("p.name").unwrap(),
        &Value::String("Alice".into())
    );
    assert!(matches!(
        results[0].get("deg").unwrap(),
        Value::Int(2) | Value::Float(2.0)
    ));

    // Bob
    assert_eq!(
        results[1].get("p.name").unwrap(),
        &Value::String("Bob".into())
    );
    assert!(matches!(
        results[1].get("deg").unwrap(),
        Value::Int(1) | Value::Float(1.0)
    ));

    // Carol (name is not set in fixture, so p.name is null)
    assert_eq!(results[2].get("p.name").unwrap(), &Value::Null);
    assert!(matches!(
        results[2].get("deg").unwrap(),
        Value::Int(0) | Value::Float(0.0)
    ));

    Ok(())
}
