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
fn core_0_1_match_all_nodes() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        txn.create_node(1, person).unwrap();
        txn.create_node(2, person).unwrap();
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n) RETURN n LIMIT 10",
        &Params::new(),
    )
    .unwrap();

    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.get_node("n").is_some()));
}

#[test]
fn core_0_1_limit_zero_and_limit_cap() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        for external_id in 1..=3 {
            txn.create_node(external_id, person).unwrap();
        }
        txn.commit().unwrap();
    }

    let zero_rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) RETURN n LIMIT 0",
        &Params::new(),
    )
    .unwrap();
    assert!(zero_rows.is_empty());

    let capped_rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) RETURN n LIMIT 2",
        &Params::new(),
    )
    .unwrap();
    assert_eq!(capped_rows.len(), 2);
}

#[test]
fn core_0_1_simple_string_and_integer_filters() {
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
        txn.set_node_property(alice, "age".to_string(), PropertyValue::Int(30))
            .unwrap();
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )
        .unwrap();
        txn.set_node_property(bob, "age".to_string(), PropertyValue::Int(40))
            .unwrap();
        txn.commit().unwrap();
    }

    let by_name = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.name = 'Alice' RETURN n",
        &Params::new(),
    )
    .unwrap();
    assert_eq!(by_name.len(), 1);

    let by_age = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.age = 30 RETURN n",
        &Params::new(),
    )
    .unwrap();
    assert_eq!(by_age.len(), 1);
    let node = by_age[0].get_node("n").expect("expected n binding");
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
        txn.create_edge(alice, knows, bob).unwrap();
        txn.create_edge(bob, knows, carol).unwrap();
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

#[test]
fn core_0_1_create_edge_query_then_match() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let snapshot = db.snapshot();
        let create =
            prepare("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")?;
        let mut txn = db.begin_write();
        assert_eq!(
            create.execute_write(&snapshot, &mut txn, &Params::new())?,
            3
        );
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (a:Person)-[:KNOWS]->(b) WHERE a.name = 'Alice' RETURN b.name LIMIT 10",
        &Params::new(),
    )?;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::String("Bob".to_string()));

    Ok(())
}

#[test]
fn core_0_1_multi_statement_txn() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let snapshot = db.snapshot();
    let mut txn = db.begin_write();

    let create_a = prepare("CREATE (a:Person {name: 'Alice'})")?;
    let create_b = prepare("CREATE (b:Person {name: 'Bob'})")?;

    assert_eq!(
        create_a.execute_write(&snapshot, &mut txn, &Params::new())?,
        1
    );
    assert_eq!(
        create_b.execute_write(&snapshot, &mut txn, &Params::new())?,
        1
    );

    txn.commit().unwrap();

    let mut names: Vec<_> = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) RETURN n.name LIMIT 10",
        &Params::new(),
    )?
    .into_iter()
    .map(|row| row.columns()[0].1.clone())
    .collect();
    names.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(
        names,
        vec![
            Value::String("Alice".to_string()),
            Value::String("Bob".to_string())
        ]
    );

    Ok(())
}

#[test]
fn core_0_1_write_reopen_query_survives() {
    let dir = tempdir().unwrap();

    {
        let db = Db::open(dir.path()).unwrap();
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        let knows = txn.get_or_create_rel_type("KNOWS").unwrap();
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
        txn.create_edge(alice, knows, bob).unwrap();
        txn.commit().unwrap();
    }

    let db = Db::open(dir.path()).unwrap();
    let rows = query_collect(
        &db.snapshot(),
        "MATCH (a:Person)-[:KNOWS]->(b) WHERE a.name = 'Alice' RETURN b.name LIMIT 10",
        &Params::new(),
    )
    .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::String("Bob".to_string()));
}
