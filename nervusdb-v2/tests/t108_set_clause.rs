use nervusdb_v2::{Db, GraphSnapshot, PropertyValue};

#[test]
fn test_set_clause_index_update() -> nervusdb_v2::Result<()> {
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
        let prepared = nervusdb_v2::query::prepare(query)?;

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
fn test_set_clause_on_relationship() -> nervusdb_v2::Result<()> {
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
        let prepared = nervusdb_v2::query::prepare("MATCH (a)-[r:1]->(b) SET r.since = 2024")?;
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
