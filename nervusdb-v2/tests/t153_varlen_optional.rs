use nervusdb_v2::query::{Row, Value, WriteableGraph};
use nervusdb_v2::{Db, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_optional_match() -> nervusdb_v2::Result<()> {
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
        let prepared = nervusdb_v2::query::prepare(query)?;

        let results: Vec<Row> = prepared
            .execute_streaming(&snapshot, &params)
            .map(|r| r.map_err(nervusdb_v2::Error::from))
            .collect::<nervusdb_v2::Result<Vec<_>>>()?;

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
        let prepared = nervusdb_v2::query::prepare(query)?;

        let results: Vec<Row> = prepared
            .execute_streaming(&snapshot, &params)
            .map(|r| r.map_err(nervusdb_v2::Error::from))
            .collect::<nervusdb_v2::Result<Vec<_>>>()?;

        assert_eq!(results.len(), 1, "Should return 1 row for Bob");

        let row = &results[0];
        let m_val = get_val(row, "m");
        if let Value::NodeId(id) = m_val {
            assert_eq!(id, charlie_id, "Bob knows Charlie");
        } else {
            panic!("m should be Node(Charlie), got {:?}", m_val);
        }
    }

    // Case 3: VarLen Optional Match (Alice -[*]-> ?) -> Null
    // Alice has NO outgoing edges.
    {
        let query = "MATCH (n:Person) WHERE n.name = 'Alice' OPTIONAL MATCH (n)-[:KNOWS*1..2]->(m) RETURN m";
        let prepared = nervusdb_v2::query::prepare(query)?;
        let results: Vec<Row> = prepared
            .execute_streaming(&snapshot, &params)
            .map(|r| r.map_err(nervusdb_v2::Error::from))
            .collect::<nervusdb_v2::Result<Vec<_>>>()?;

        assert_eq!(results.len(), 1, "Should return 1 row (Null) for Alice");
        let row = &results[0];
        let m_val = get_val(row, "m");
        assert!(
            matches!(m_val, Value::Null),
            "VarLen optional match should return Null if no path"
        );
    }

    // Case 4: VarLen Optional Match (Bob -[*]-> Charlie) -> Found
    {
        let query =
            "MATCH (n:Person) WHERE n.name = 'Bob' OPTIONAL MATCH (n)-[:KNOWS*1..2]->(m) RETURN m";
        let prepared = nervusdb_v2::query::prepare(query)?;
        let results: Vec<Row> = prepared
            .execute_streaming(&snapshot, &params)
            .map(|r| r.map_err(nervusdb_v2::Error::from))
            .collect::<nervusdb_v2::Result<Vec<_>>>()?;

        assert_eq!(results.len(), 1, "Should return 1 row for Bob");
        let row = &results[0];
        let m_val = get_val(row, "m");
        if let Value::NodeId(id) = m_val {
            assert_eq!(id, charlie_id, "Bob knows Charlie via VarLen");
        } else {
            panic!("m should be Node(Charlie), got {:?}", m_val);
        }
    }

    // Case 5: VarLen Chaining (Alice -> Null -> Null)
    {
        let query = "MATCH (n:Person) WHERE n.name = 'Alice' OPTIONAL MATCH (n)-[:KNOWS*1..2]->(m) OPTIONAL MATCH (m)-[:KNOWS*1..2]->(k) RETURN k";
        let prepared = nervusdb_v2::query::prepare(query)?;
        let results: Vec<Row> = prepared
            .execute_streaming(&snapshot, &params)
            .map(|r| r.map_err(nervusdb_v2::Error::from))
            .collect::<nervusdb_v2::Result<Vec<_>>>()?;

        assert_eq!(results.len(), 1);
        let row = &results[0];
        let k_val = get_val(row, "k");
        assert!(
            matches!(k_val, Value::Null),
            "k should be Null via chained VarLen optional"
        );
    }

    Ok(())
}
