use nervusdb::query::{Row, Value, WriteableGraph};
use nervusdb::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_optional_match() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t151.ndb");
    let db = Db::open(&db_path)?;

    // Setup Data
    let (_alice_id, _bob_id, charlie_id) = {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;
        let knows = txn.get_or_create_rel_type_id("KNOWS")?;

        let alice = txn.create_node(1, person)?;
        txn.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )?;

        let bob = txn.create_node(2, person)?;
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )?;

        let charlie = txn.create_node(3, person)?;
        txn.set_node_property(
            charlie,
            "name".to_string(),
            PropertyValue::String("Charlie".to_string()),
        )?;

        txn.create_edge(bob, knows, charlie);

        txn.commit()?;
        (alice, bob, charlie)
    };

    let snapshot = db.snapshot();
    let params = Default::default();

    // Helper
    let get_val = |row: &Row, alias: &str| -> Value {
        row.columns()
            .iter()
            .find(|(k, _)| k == alias)
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Null)
    };

    // Case 1: Alice (No friends)
    {
        let query =
            "MATCH (n:Person) WHERE n.name = 'Alice' OPTIONAL MATCH (n)-[:KNOWS]->(m) RETURN m";
        let prepared = nervusdb::query::prepare(query)?;

        let results: Vec<Row> = prepared
            .execute_streaming(&snapshot, &params)
            .map(|r| r.map_err(nervusdb::Error::from))
            .collect::<nervusdb::Result<Vec<_>>>()?;

        assert_eq!(
            results.len(),
            1,
            "Should return 1 row for Alice (Optional Match)"
        );

        let row = &results[0];
        let m_val = get_val(row, "m");
        assert!(
            matches!(m_val, Value::Null),
            "m should be Null for Alice, got {:?}",
            m_val
        );
    }

    // Case 2: Bob (Has friend Charlie)
    {
        let query =
            "MATCH (n:Person) WHERE n.name = 'Bob' OPTIONAL MATCH (n)-[:KNOWS]->(m) RETURN m";
        let prepared = nervusdb::query::prepare(query)?;

        let results: Vec<Row> = prepared
            .execute_streaming(&snapshot, &params)
            .map(|r| r.map_err(nervusdb::Error::from))
            .collect::<nervusdb::Result<Vec<_>>>()?;

        assert_eq!(results.len(), 1, "Should return 1 row for Bob");

        let row = &results[0];
        let m_val = get_val(row, "m");
        if let Value::NodeId(id) = m_val {
            assert_eq!(id, charlie_id, "Bob knows Charlie");
        } else {
            panic!("m should be Node(Charlie), got {:?}", m_val);
        }
    }

    // Case 3: Chaining Optional Matches
    {
        let query = "MATCH (n:Person) WHERE n.name = 'Alice' OPTIONAL MATCH (n)-[:KNOWS]->(m) OPTIONAL MATCH (m)-[:KNOWS]->(k) RETURN k";
        let prepared = nervusdb::query::prepare(query)?;
        let results: Vec<Row> = prepared
            .execute_streaming(&snapshot, &params)
            .map(|r| r.map_err(nervusdb::Error::from))
            .collect::<nervusdb::Result<Vec<_>>>()?;

        assert_eq!(results.len(), 1);
        let row = &results[0];
        let k_val = get_val(row, "k");
        assert!(matches!(k_val, Value::Null), "k should be Null");
    }

    Ok(())
}

#[test]
fn test_optional_match_on_empty_graph_returns_single_null_row() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;
    let snapshot = db.snapshot();

    let query = nervusdb::query::prepare("OPTIONAL MATCH (n) RETURN n")?;
    let rows: Vec<Row> = query
        .execute_streaming(&snapshot, &Default::default())
        .map(|r| r.map_err(nervusdb::Error::from))
        .collect::<nervusdb::Result<Vec<_>>>()?;

    assert_eq!(rows.len(), 1);
    let value = rows[0]
        .columns()
        .iter()
        .find(|(k, _)| k == "n")
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Null);
    assert!(matches!(value, Value::Null));

    Ok(())
}

#[test]
fn test_optional_match_with_bound_relationship_avoids_cartesian_expansion() -> nervusdb::Result<()>
{
    let dir = tempdir()?;
    let db_path = dir.path().join("t151_bound_rel.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        let label_a = txn.get_or_create_label("A")?;
        let label_b = txn.get_or_create_label("B")?;
        let rel_t = txn.get_or_create_rel_type_id("T")?;

        let a = txn.create_node(1, label_a)?;
        let b = txn.create_node(2, label_b)?;
        let _extra = txn.create_node(3, label_b)?;
        txn.create_edge(a, rel_t, b);
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let query = nervusdb::query::prepare(
        "MATCH ()-[r]->() WITH r LIMIT 1 OPTIONAL MATCH (a2)-[r]->(b2) RETURN a2, r, b2",
    )?;
    let rows: Vec<Row> = query
        .execute_streaming(&snapshot, &Default::default())
        .map(|r| r.map_err(nervusdb::Error::from))
        .collect::<nervusdb::Result<Vec<_>>>()?;

    assert_eq!(
        rows.len(),
        1,
        "optional match should not expand by unrelated start nodes"
    );

    let row = &rows[0];
    let a2 = row
        .columns()
        .iter()
        .find(|(k, _)| k == "a2")
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Null);
    let r = row
        .columns()
        .iter()
        .find(|(k, _)| k == "r")
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Null);
    let b2 = row
        .columns()
        .iter()
        .find(|(k, _)| k == "b2")
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Null);

    assert!(
        matches!(a2, Value::NodeId(_) | Value::Node(_)),
        "expected a2 to be node-like, got {a2:?}"
    );
    match r {
        Value::Relationship(rel) => {
            assert_eq!(rel.rel_type, "T");
        }
        Value::EdgeKey(_) => {}
        _ => panic!("expected r to be relationship-like, got {r:?}"),
    }
    assert!(
        matches!(b2, Value::NodeId(_) | Value::Node(_)),
        "expected b2 to be node-like, got {b2:?}"
    );

    Ok(())
}

#[test]
fn test_optional_match_bound_rel_reverse_direction_with_where_scope() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;

    {
        let mut txn = db.begin_write();
        let label_a = txn.get_or_create_label("A")?;
        let label_b = txn.get_or_create_label("B")?;
        let rel_t = txn.get_or_create_rel_type_id("T")?;
        let a = txn.create_node(1, label_a)?;
        let b = txn.create_node(2, label_b)?;
        txn.create_edge(a, rel_t, b);
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let query = nervusdb::query::prepare(
        "MATCH (a1)-[r]->() \
         WITH r, a1 LIMIT 1 \
         OPTIONAL MATCH (a2)<-[r]-(b2) \
         WHERE a1 = a2 \
         RETURN a1, r, b2, a2",
    )?;
    let rows: Vec<Row> = query
        .execute_streaming(&snapshot, &Default::default())
        .map(|r| r.map_err(nervusdb::Error::from))
        .collect::<nervusdb::Result<Vec<_>>>()?;

    assert_eq!(
        rows.len(),
        1,
        "query should keep one row after OPTIONAL MATCH"
    );
    let row = &rows[0];
    let a1 = row
        .columns()
        .iter()
        .find(|(k, _)| k == "a1")
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Null);
    let a2 = row
        .columns()
        .iter()
        .find(|(k, _)| k == "a2")
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Null);
    let b2 = row
        .columns()
        .iter()
        .find(|(k, _)| k == "b2")
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Null);

    assert!(
        matches!(a1, Value::NodeId(_) | Value::Node(_)),
        "expected a1 to be node-like, got {a1:?}"
    );
    assert!(
        matches!(a2, Value::Null),
        "expected a2 to be null, got {a2:?}"
    );
    assert!(
        matches!(b2, Value::Null),
        "expected b2 to be null, got {b2:?}"
    );

    Ok(())
}

#[test]
fn test_optional_match_respects_labels_on_optional_end_node() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path())?;

    {
        let parsed = nervusdb::query::parse(
            "CREATE (a1:X {id: 1}), (a2:X {id: 2}), (b1:Y), (b2:Y:Z), \
             (a1)-[:REL]->(b1), (a1)-[:REL]->(b2), (a2)-[:REL]->(b1)",
        )?;
        let mut saw_yz = false;
        for clause in parsed.clauses {
            if let nervusdb::query::ast::Clause::Create(create) = clause {
                for pattern in create.patterns {
                    for element in pattern.elements {
                        if let nervusdb::query::ast::PathElement::Node(node) = element
                            && node.labels == vec!["Y".to_string(), "Z".to_string()]
                        {
                            saw_yz = true;
                        }
                    }
                }
            }
        }
        assert!(saw_yz, "parser must keep multi-label node pattern (:Y:Z)");

        let mut txn = db.begin_write();
        let setup = nervusdb::query::prepare(
            "CREATE (a1:X {id: 1}), (a2:X {id: 2}), (b1:Y), (b2:Y:Z), \
             (a1)-[:REL]->(b1), (a1)-[:REL]->(b2), (a2)-[:REL]->(b1)",
        )?;
        let _ = setup.execute_write(&db.snapshot(), &mut txn, &Default::default())?;
        txn.commit()?;
    }

    let snapshot = db.snapshot();

    let y = snapshot.resolve_label_id("Y").expect("label Y must exist");
    let z = snapshot.resolve_label_id("Z").expect("label Z must exist");
    let mut debug_labels = Vec::new();
    let mut has_yz = false;
    for node in snapshot.nodes() {
        let labels = snapshot.resolve_node_labels(node).unwrap_or_default();
        debug_labels.push((node, labels.clone()));
        if labels.contains(&y) && labels.contains(&z) {
            has_yz = true;
        }
    }
    assert!(
        has_yz,
        "setup must produce at least one (:Y:Z) node, got {:?}",
        debug_labels
    );

    let query = nervusdb::query::prepare(
        "MATCH (a:X) OPTIONAL MATCH (a)-[:REL]->(b:Y:Z) RETURN a.id AS id, b ORDER BY id",
    )?;
    let rows: Vec<Row> = query
        .execute_streaming(&snapshot, &Default::default())
        .map(|r| r.map_err(nervusdb::Error::from))
        .collect::<nervusdb::Result<Vec<_>>>()?;

    assert_eq!(
        rows.len(),
        2,
        "optional end-node label filtering should hold"
    );

    let id1 = rows[0].get("id").cloned().unwrap_or(Value::Null);
    let id2 = rows[1].get("id").cloned().unwrap_or(Value::Null);
    assert_eq!(id1, Value::Int(1));
    assert_eq!(id2, Value::Int(2));

    let b_for_id1 = rows[0].get("b").cloned().unwrap_or(Value::Null);
    let b_for_id2 = rows[1].get("b").cloned().unwrap_or(Value::Null);

    assert!(
        matches!(b_for_id1, Value::NodeId(_) | Value::Node(_)),
        "id=1 should match only (:Y:Z), got {b_for_id1:?}"
    );
    assert!(matches!(b_for_id2, Value::Null), "id=2 should be null");

    Ok(())
}
