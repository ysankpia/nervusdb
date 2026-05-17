use nervusdb::{Db, GraphSnapshot, PropertyValue};
use nervusdb_query::{Params, Result as QueryResult, Value, prepare, query_collect};
use tempfile::tempdir;

#[test]
fn core_0_1_return_one() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let rows = query_collect(&db.snapshot(), "RETURN 1", &Params::new()).unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::Int(1));
}

#[test]
fn core_0_1_label_scan_property_filter_and_limit() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        let alice = txn.create_node(1, person).unwrap();
        let bob = txn.create_node(2, person).unwrap();
        txn.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )
        .unwrap();
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.name = 'Alice' RETURN n LIMIT 1",
        &Params::new(),
    )
    .unwrap();

    assert_eq!(rows.len(), 1);
    let node = rows[0].get_node("n").expect("expected n binding");
    assert_eq!(
        db.snapshot().node_property(node, "name"),
        Some(PropertyValue::String("Alice".to_string()))
    );
}

#[test]
fn core_0_1_one_hop_and_two_hop_traversal() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        let knows = txn.get_or_create_rel_type("KNOWS").unwrap();
        let alice = txn.create_node(1, person).unwrap();
        let bob = txn.create_node(2, person).unwrap();
        let carol = txn.create_node(3, person).unwrap();
        txn.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )
        .unwrap();
        txn.set_node_property(
            carol,
            "name".to_string(),
            PropertyValue::String("Carol".to_string()),
        )
        .unwrap();
        txn.create_edge(alice, knows, bob);
        txn.create_edge(bob, knows, carol);
        txn.commit().unwrap();
    }

    let one_hop = query_collect(
        &db.snapshot(),
        "MATCH (a:Person)-[:KNOWS]->(b) WHERE a.name = 'Alice' RETURN b.name LIMIT 10",
        &Params::new(),
    )
    .unwrap();
    assert_eq!(one_hop.len(), 1);
    assert_eq!(one_hop[0].columns()[0].1, Value::String("Bob".to_string()));

    let two_hop = query_collect(
        &db.snapshot(),
        "MATCH (a:Person)-[:KNOWS]->(b)-[:KNOWS]->(c) WHERE a.name = 'Alice' RETURN c.name LIMIT 10",
        &Params::new(),
    )
    .unwrap();
    assert_eq!(two_hop.len(), 1);
    assert_eq!(
        two_hop[0].columns()[0].1,
        Value::String("Carol".to_string())
    );
}

#[test]
fn core_0_1_basic_create_set_delete_and_explain() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let snapshot = db.snapshot();
        let create = prepare("CREATE (n:Person {name: 'Alice'})")?;
        let mut txn = db.begin_write();
        assert_eq!(
            create.execute_write(&snapshot, &mut txn, &Params::new())?,
            1
        );
        txn.commit().unwrap();
    }

    {
        let snapshot = db.snapshot();
        let set = prepare("MATCH (n:Person) WHERE n.name = 'Alice' SET n.name = 'Ada'")?;
        let mut txn = db.begin_write();
        assert_eq!(set.execute_write(&snapshot, &mut txn, &Params::new())?, 1);
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.name = 'Ada' RETURN n",
        &Params::new(),
    )?;
    assert_eq!(rows.len(), 1);

    let explain = query_collect(
        &db.snapshot(),
        "EXPLAIN MATCH (n:Person) WHERE n.name = 'Ada' RETURN n",
        &Params::new(),
    )?;
    assert_eq!(explain.len(), 1);
    assert!(matches!(explain[0].columns()[0].1, Value::String(_)));

    {
        let snapshot = db.snapshot();
        let delete = prepare("MATCH (n:Person) WHERE n.name = 'Ada' DELETE n")?;
        let mut txn = db.begin_write();
        assert_eq!(
            delete.execute_write(&snapshot, &mut txn, &Params::new())?,
            1
        );
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.name = 'Ada' RETURN n",
        &Params::new(),
    )?;
    assert!(rows.is_empty());

    Ok(())
}
