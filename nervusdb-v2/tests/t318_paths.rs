use nervusdb_v2::query::Value;
use nervusdb_v2::{Db, PropertyValue};
use tempfile::tempdir;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn test_path_assignment_basic() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let node_label = txn.get_or_create_label("Node")?;
    let alice = txn.create_node(1u64.into(), node_label)?;
    let bob = txn.create_node(2u64.into(), node_label)?;
    let knows = txn.get_or_create_rel_type("KNOWS")?;
    txn.create_edge(alice, knows, bob);

    // Set properties for identification
    txn.set_node_property(
        alice,
        "name".to_string(),
        PropertyValue::String("Alice".into()),
    )?;
    txn.set_node_property(bob, "name".to_string(), PropertyValue::String("Bob".into()))?;

    txn.commit()?;

    let snapshot = db.snapshot();

    // MATCH p = (a {name: 'Alice'})-[:KNOWS]->(b) RETURN p, length(p), nodes(p), relationships(p)
    let query = "MATCH p = (a {name: 'Alice'})-[:KNOWS]->(b) RETURN p, length(p), nodes(p), relationships(p)";
    let prep = nervusdb_v2::query::prepare(query)?;
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;

    println!("Results: {:?}", results);
    assert_eq!(results.len(), 1);
    let row = &results[0];

    // Check length(p)
    assert_eq!(row.get("length(p)").unwrap(), &Value::Int(1));

    // Check nodes(p)
    if let Value::List(nodes) = row.get("nodes(p)").unwrap() {
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0], Value::NodeId(alice));
        assert_eq!(nodes[1], Value::NodeId(bob));
    } else {
        panic!("nodes(p) is not a list");
    }

    Ok(())
}

#[test]
fn test_path_multi_hop() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let node_label = txn.get_or_create_label("Node")?;
    let a = txn.create_node(101u64.into(), node_label)?;
    let b = txn.create_node(102u64.into(), node_label)?;
    let c = txn.create_node(103u64.into(), node_label)?;
    let rel = txn.get_or_create_rel_type("REL")?;
    txn.create_edge(a, rel, b);
    txn.create_edge(b, rel, c);

    txn.commit()?;
    let snapshot = db.snapshot();

    // MATCH p = (a)-->(b)-->(c) RETURN length(p)
    let query = "MATCH p = (a)-->(b)-->(c) RETURN length(p)";
    let prep = nervusdb_v2::query::prepare(query)?;
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("length(p)").unwrap(), &Value::Int(2));

    Ok(())
}

#[test]
fn test_path_var_len() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let node_label = txn.get_or_create_label("Node")?;
    let a = txn.create_node(201u64.into(), node_label)?;
    let b = txn.create_node(202u64.into(), node_label)?;
    let c = txn.create_node(203u64.into(), node_label)?;
    let rel = txn.get_or_create_rel_type("REL")?;
    txn.create_edge(a, rel, b);
    txn.create_edge(b, rel, c);

    txn.commit()?;
    let snapshot = db.snapshot();

    // MATCH p = (a)-[:REL*2]->(c) RETURN length(p), nodes(p)
    let query = "MATCH p = (a)-[:REL*2]->(c) RETURN length(p), nodes(p)";
    let prep = nervusdb_v2::query::prepare(query)?;
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("length(p)").unwrap(), &Value::Int(2));

    if let Value::List(nodes) = results[0].get("nodes(p)").unwrap() {
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0], Value::NodeId(a));
        assert_eq!(nodes[1], Value::NodeId(b));
        assert_eq!(nodes[2], Value::NodeId(c));
    }

    Ok(())
}

#[test]
fn test_path_incoming_undirected() -> Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let mut txn = db.begin_write();

    let node_label = txn.get_or_create_label("Node")?;
    let a = txn.create_node(301u64.into(), node_label)?;
    let b = txn.create_node(302u64.into(), node_label)?;
    let rel = txn.get_or_create_rel_type("REL")?;
    txn.create_edge(a, rel, b);

    txn.commit()?;
    let snapshot = db.snapshot();

    // Incoming: MATCH p = (b)<-[:REL]-(a) RETURN length(p)
    let query = "MATCH p = (b)<-[:REL]-(a) RETURN length(p), nodes(p)";
    let prep = nervusdb_v2::query::prepare(query)?;
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("length(p)").unwrap(), &Value::Int(1));
    if let Value::List(nodes) = results[0].get("nodes(p)").unwrap() {
        assert_eq!(nodes[0], Value::NodeId(b));
        assert_eq!(nodes[1], Value::NodeId(a));
    }

    // Undirected: MATCH p = (a)-[:REL]-(b)
    let query_undirected = "MATCH p = (a)-[:REL]-(b) RETURN length(p)";
    let prep_und = nervusdb_v2::query::prepare(query_undirected)?;
    let results_undirected: Vec<_> = prep_und
        .execute_streaming(&snapshot, &Default::default())
        .collect::<nervusdb_v2::query::error::Result<Vec<_>>>()?;
    assert!(results_undirected.len() >= 1);

    Ok(())
}
