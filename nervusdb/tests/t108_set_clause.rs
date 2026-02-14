use nervusdb::{Db, GraphSnapshot, PropertyValue};

#[test]
fn test_set_clause_index_update() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t108.ndb");
    let db = Db::open(&db_path)?;

    // 1. Create Index
    db.create_index("Person", "name")?;

    // 2. Insert Initial Data
    {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("Person")?;
        let iid = txn.create_node(1, label)?;
        txn.set_node_property(
            iid,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )?;
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    // Verify initial index lookup
    let ids = snapshot
        .lookup_index(
            "Person",
            "name",
            &PropertyValue::String("Alice".to_string()),
        )
        .expect("Index should exist");
    assert_eq!(ids.len(), 1);
    let alice_node_id = ids[0];

    // 3. Update Property via SET clause
    {
        // Use a snapshot for reading (outside write txn to avoid potential contention if any,
        // though MVCC should allow it).
        let write_snapshot = db.snapshot();

        let mut txn = db.begin_write();
        let query = "MATCH (n:Person) WHERE n.name = 'Alice' SET n.name = 'Bob'";
        let prepared = nervusdb::query::prepare(query)?;

        // Executing write query (1 property set)
        let count = prepared.execute_write(&write_snapshot, &mut txn, &Default::default())?;
        assert_eq!(count, 1);

        txn.commit()?;
    }

    // 4. Verify Index Update (New Snapshot)
    let snapshot = db.snapshot();

    // "Alice" should be gone from index
    let alice_lookup = snapshot.lookup_index(
        "Person",
        "name",
        &PropertyValue::String("Alice".to_string()),
    );
    if let Some(ids) = alice_lookup {
        assert!(
            ids.is_empty(),
            "Alice should be removed from index, found: {:?}",
            ids
        );
    }

    // "Bob" should be in index
    let bob_lookup = snapshot
        .lookup_index("Person", "name", &PropertyValue::String("Bob".to_string()))
        .expect("Index should still exist");
    assert_eq!(bob_lookup.len(), 1, "Bob should be in index");
    assert_eq!(bob_lookup[0], alice_node_id, "Node ID should be preserved");

    // 5. Verify Property Value
    let val = snapshot
        .node_property(alice_node_id, "name")
        .expect("Property should exist");
    match val {
        PropertyValue::String(s) => assert_eq!(s, "Bob"),
        _ => panic!("Wrong type"),
    }

    Ok(())
}

#[test]
fn test_set_clause_on_relationship() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t108_edge.ndb");
    let db = Db::open(&db_path)?;

    // Create two nodes and an edge between them.
    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;
        let rel = txn.get_or_create_label("1")?;

        let a = txn.create_node(1, person)?;
        let b = txn.create_node(2, person)?;
        txn.create_edge(a, rel, b);

        txn.commit()?;
    }

    // Update edge property via SET.
    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let prepared = nervusdb::query::prepare("MATCH (a)-[r:1]->(b) SET r.since = 2024")?;
        let count = prepared.execute_write(&snapshot, &mut txn, &Default::default())?;
        assert_eq!(count, 1);
        txn.commit()?;
    }

    // Verify edge property.
    let snap = db.snapshot();
    let a = snap
        .nodes()
        .find(|&iid| snap.resolve_external(iid) == Some(1))
        .expect("node 1 should exist");
    let rel = snap
        .resolve_rel_type_id("1")
        .expect("rel type should exist");
    let edge = snap
        .neighbors(a, Some(rel))
        .next()
        .expect("edge should exist");

    let since = snap
        .edge_property(edge, "since")
        .expect("edge property should exist");
    assert_eq!(since, PropertyValue::Int(2024));

    Ok(())
}

#[test]
fn test_set_clause_add_node_label() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t108_set_label.ndb");
    let db = Db::open(&db_path)?;

    let node_id = {
        let mut txn = db.begin_write();
        let a = txn.get_or_create_label("A")?;
        let node_id = txn.create_node(1, a)?;
        txn.commit()?;
        node_id
    };

    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let prepared = nervusdb::query::prepare("MATCH (n:A) SET n:Foo")?;
        let count = prepared.execute_write(&snapshot, &mut txn, &Default::default())?;
        assert_eq!(count, 1);
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let labels = snapshot.resolve_node_labels(node_id).unwrap_or_default();
    let foo = snapshot
        .resolve_label_id("Foo")
        .expect("Foo label should be created");
    assert!(labels.contains(&foo));

    Ok(())
}

#[test]
fn test_set_null_removes_existing_property() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t108_set_null.ndb");
    let db = Db::open(&db_path)?;

    let node_id = {
        let mut txn = db.begin_write();
        let a = txn.get_or_create_label("A")?;
        let node_id = txn.create_node(1, a)?;
        txn.set_node_property(node_id, "property1".to_string(), PropertyValue::Int(23))?;
        txn.set_node_property(node_id, "property2".to_string(), PropertyValue::Int(46))?;
        txn.commit()?;
        node_id
    };

    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let prepared = nervusdb::query::prepare("MATCH (n:A) SET n.property1 = null")?;
        let count = prepared.execute_write(&snapshot, &mut txn, &Default::default())?;
        assert_eq!(count, 1);
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    assert_eq!(snapshot.node_property(node_id, "property1"), None);
    assert_eq!(
        snapshot.node_property(node_id, "property2"),
        Some(PropertyValue::Int(46))
    );

    Ok(())
}

#[test]
fn test_set_updates_are_visible_to_following_with_and_limit() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t108_set_visibility.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        let n = txn.get_or_create_label("N")?;
        for i in 1..=5 {
            let node_id = txn.create_node(i as u64, n)?;
            txn.set_node_property(node_id, "num".to_string(), PropertyValue::Int(i as i64))?;
        }
        txn.commit()?;
    }

    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q = "MATCH (n:N) SET n.num = n.num + 1 WITH sum(n.num) AS sum RETURN sum";
        let (rows, _) =
            nervusdb::query::prepare(q)?.execute_mixed(&snapshot, &mut txn, &Default::default())?;
        txn.commit()?;

        assert_eq!(rows.len(), 1);
        assert!(matches!(
            rows[0].get("sum"),
            Some(nervusdb::query::Value::Int(20))
        ));
    }

    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q = "MATCH (n:N) SET n.num = 42 RETURN n.num AS num SKIP 2 LIMIT 2";
        let (rows, _) =
            nervusdb::query::prepare(q)?.execute_mixed(&snapshot, &mut txn, &Default::default())?;
        txn.commit()?;

        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| matches!(row.get("num"), Some(nervusdb::query::Value::Int(42))))
        );
    }

    Ok(())
}

#[test]
fn test_set_parenthesized_target_variable() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t108_set_parenthesized.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        let a = txn.get_or_create_label("A")?;
        txn.create_node(1, a)?;
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let mut txn = db.begin_write();
    let q = "MATCH (n:A) SET (n).name = 'neo4j' RETURN n.name AS name";
    let (rows, count) =
        nervusdb::query::prepare(q)?.execute_mixed(&snapshot, &mut txn, &Default::default())?;
    txn.commit()?;

    assert_eq!(count, 1);
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        rows[0].get("name"),
        Some(nervusdb::query::Value::String(s)) if s == "neo4j"
    ));

    Ok(())
}

#[test]
fn test_set_undefined_variable_in_expression_rejected_at_compile_time() {
    let err = nervusdb::query::prepare("MATCH (a) SET a.name = missing RETURN a")
        .expect_err("prepare should fail on undefined variable in SET expression")
        .to_string();
    assert!(
        err.contains("UndefinedVariable"),
        "unexpected compile error: {err}"
    );
}

#[test]
fn test_set_overwrite_with_map_replaces_properties() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t108_set_map_replace.ndb");
    let db = Db::open(&db_path)?;

    let node_id = {
        let mut txn = db.begin_write();
        let x = txn.get_or_create_label("X")?;
        let node_id = txn.create_node(1, x)?;
        txn.set_node_property(
            node_id,
            "name".to_string(),
            PropertyValue::String("A".to_string()),
        )?;
        txn.set_node_property(
            node_id,
            "name2".to_string(),
            PropertyValue::String("B".to_string()),
        )?;
        txn.commit()?;
        node_id
    };

    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q = "MATCH (n:X) SET n = {name: 'B', name2: null, baz: 'C'} RETURN n";
        let (rows, count) =
            nervusdb::query::prepare(q)?.execute_mixed(&snapshot, &mut txn, &Default::default())?;
        txn.commit()?;
        assert_eq!(rows.len(), 1);
        assert!(count > 0);
    }

    let snapshot = db.snapshot();
    assert_eq!(
        snapshot.node_property(node_id, "name"),
        Some(PropertyValue::String("B".to_string()))
    );
    assert_eq!(snapshot.node_property(node_id, "name2"), None);
    assert_eq!(
        snapshot.node_property(node_id, "baz"),
        Some(PropertyValue::String("C".to_string()))
    );

    Ok(())
}

#[test]
fn test_set_append_with_map_merges_and_removes_nulls() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t108_set_map_append.ndb");
    let db = Db::open(&db_path)?;

    let node_id = {
        let mut txn = db.begin_write();
        let x = txn.get_or_create_label("X")?;
        let node_id = txn.create_node(2, x)?;
        txn.set_node_property(
            node_id,
            "name".to_string(),
            PropertyValue::String("A".to_string()),
        )?;
        txn.set_node_property(
            node_id,
            "name2".to_string(),
            PropertyValue::String("B".to_string()),
        )?;
        txn.set_node_property(
            node_id,
            "keep".to_string(),
            PropertyValue::String("Z".to_string()),
        )?;
        txn.commit()?;
        node_id
    };

    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q =
            "MATCH (n:X) WHERE n.name = 'A' SET n += {name: 'C', name2: null, newk: 42} RETURN n";
        let (rows, count) =
            nervusdb::query::prepare(q)?.execute_mixed(&snapshot, &mut txn, &Default::default())?;
        txn.commit()?;
        assert_eq!(rows.len(), 1);
        assert!(count > 0);
    }

    let snapshot = db.snapshot();
    assert_eq!(
        snapshot.node_property(node_id, "name"),
        Some(PropertyValue::String("C".to_string()))
    );
    assert_eq!(snapshot.node_property(node_id, "name2"), None);
    assert_eq!(
        snapshot.node_property(node_id, "keep"),
        Some(PropertyValue::String("Z".to_string()))
    );
    assert_eq!(
        snapshot.node_property(node_id, "newk"),
        Some(PropertyValue::Int(42))
    );

    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q = "OPTIONAL MATCH (a:DoesNotExist) SET a += {num: 42} RETURN a";
        let (rows, count) =
            nervusdb::query::prepare(q)?.execute_mixed(&snapshot, &mut txn, &Default::default())?;
        txn.commit()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(count, 0);
        assert!(matches!(
            rows[0].get("a"),
            Some(nervusdb::query::Value::Null)
        ));
    }

    Ok(())
}
