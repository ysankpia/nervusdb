use nervusdb::query::Value;
use nervusdb::{Db, PropertyValue};
use tempfile::tempdir;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn test_cartesian_product_nodes() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let person_label = txn.get_or_create_label("Person")?;
    let city_label = txn.get_or_create_label("City")?;

    let alice = txn.create_node(101u64, person_label)?;
    txn.set_node_property(
        alice,
        "name".to_string(),
        PropertyValue::String("Alice".into()),
    )?;

    let london = txn.create_node(201u64, city_label)?;
    txn.set_node_property(
        london,
        "name".to_string(),
        PropertyValue::String("London".into()),
    )?;

    txn.commit()?;

    // MATCH (a:Person), (b:City) - Cartesian Product
    let query = "MATCH (a:Person), (b:City) RETURN a.name, b.name";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb::query::error::Result<Vec<_>>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("a.name").unwrap(),
        &Value::String("Alice".into())
    );
    assert_eq!(
        results[0].get("b.name").unwrap(),
        &Value::String("London".into())
    );

    Ok(())
}

#[test]
fn test_disjoint_match_clauses() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let l = txn.get_or_create_label("L")?;
    let n1 = txn.create_node(1u64, l)?;
    txn.set_node_property(n1, "v".to_string(), PropertyValue::Int(1))?;
    let n2 = txn.create_node(2u64, l)?;
    txn.set_node_property(n2, "v".to_string(), PropertyValue::Int(2))?;

    txn.commit()?;

    // MATCH (a:L) MATCH (b:L) RETURN a.v, b.v";
    let query = "MATCH (a:L) MATCH (b:L) RETURN a.v, b.v";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb::query::error::Result<Vec<_>>>()?;

    // 2 x 2 = 4 rows
    assert_eq!(results.len(), 4);

    Ok(())
}

#[test]
fn test_shared_variable_expansion() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Node")?;
    let rel_type = txn.get_or_create_rel_type("KNOWS")?;

    let alice_node = txn.create_node(1u64, label)?;
    txn.set_node_property(
        alice_node,
        "name".to_string(),
        PropertyValue::String("Alice".into()),
    )?;
    let bob_node = txn.create_node(2u64, label)?;
    txn.set_node_property(
        bob_node,
        "name".to_string(),
        PropertyValue::String("Bob".into()),
    )?;

    txn.create_edge(alice_node, rel_type, bob_node);

    txn.commit()?;

    // MATCH (a:Node {name: 'Alice'}), (a)-[:KNOWS]->(b) - Shared variable in one MATCH
    let query = "MATCH (a:Node {name: 'Alice'}), (a)-[:KNOWS]->(b) RETURN b.name";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb::query::error::Result<Vec<_>>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("b.name").unwrap(),
        &Value::String("Bob".into())
    );

    Ok(())
}

#[test]
fn test_cross_join_with_where() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let l = txn.get_or_create_label("N")?;
    let n1 = txn.create_node(1u64, l)?;
    txn.set_node_property(n1, "val".to_string(), PropertyValue::Int(10))?;
    let n2 = txn.create_node(2u64, l)?;
    txn.set_node_property(n2, "val".to_string(), PropertyValue::Int(20))?;

    txn.commit()?;

    // MATCH (a:N), (b:N) WHERE a.val < b.val RETURN a.val, b.val
    let query = "MATCH (a:N), (b:N) WHERE a.val < b.val RETURN a.val, b.val";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb::query::error::Result<Vec<_>>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("a.val").unwrap(), &Value::Int(10));
    assert_eq!(results[0].get("b.val").unwrap(), &Value::Int(20));

    Ok(())
}
