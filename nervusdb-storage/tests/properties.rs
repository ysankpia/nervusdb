use nervusdb_api::{GraphSnapshot, GraphStore};
use nervusdb_storage::Result;
use nervusdb_storage::engine::GraphEngine;
use nervusdb_storage::property::PropertyValue;
use tempfile::tempdir;

#[test]
fn test_node_property_set_and_get() -> Result<()> {
    let dir = tempdir()?;
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal)?;

    // Write transaction: create node and set property
    {
        let mut txn = engine.begin_write();
        let node_id = txn.create_node(1, 0)?;
        txn.set_node_property(
            node_id,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        );
        txn.set_node_property(node_id, "age".to_string(), PropertyValue::Int(30));
        txn.commit()?;
    }

    // Read transaction: verify properties
    {
        let _snapshot = engine.begin_read();
        let api_snapshot = engine.snapshot();

        // Test single property
        let name = api_snapshot.node_property(0, "name");
        assert_eq!(
            name,
            Some(nervusdb_api::PropertyValue::String("Alice".to_string()))
        );

        let age = api_snapshot.node_property(0, "age");
        assert_eq!(age, Some(nervusdb_api::PropertyValue::Int(30)));

        // Test all properties
        let props = api_snapshot.node_properties(0);
        assert!(props.is_some());
        let props = props.unwrap();
        assert_eq!(
            props.get("name"),
            Some(&nervusdb_api::PropertyValue::String("Alice".to_string()))
        );
        assert_eq!(
            props.get("age"),
            Some(&nervusdb_api::PropertyValue::Int(30))
        );
    }

    Ok(())
}

#[test]
fn test_edge_property_set_and_get() -> Result<()> {
    let dir = tempdir()?;
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal)?;

    // Write transaction: create nodes, edge, and set property
    {
        let mut txn = engine.begin_write();
        let node1 = txn.create_node(1, 0)?;
        let node2 = txn.create_node(2, 0)?;
        txn.create_edge(node1, 1, node2);
        txn.set_edge_property(
            node1,
            1,
            node2,
            "weight".to_string(),
            PropertyValue::Float(0.5),
        );
        txn.commit()?;
    }

    // Read transaction: verify property
    {
        let api_snapshot = engine.snapshot();
        let edge = nervusdb_api::EdgeKey {
            src: 0,
            rel: 1,
            dst: 1,
        };

        let weight = api_snapshot.edge_property(edge, "weight");
        assert_eq!(weight, Some(nervusdb_api::PropertyValue::Float(0.5)));
    }

    Ok(())
}

#[test]
fn test_property_overwrite() -> Result<()> {
    let dir = tempdir()?;
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal)?;

    // First transaction: set property
    {
        let mut txn = engine.begin_write();
        let node_id = txn.create_node(1, 0)?;
        txn.set_node_property(node_id, "age".to_string(), PropertyValue::Int(25));
        txn.commit()?;
    }

    // Second transaction: overwrite property
    {
        let mut txn = engine.begin_write();
        txn.set_node_property(0, "age".to_string(), PropertyValue::Int(30));
        txn.commit()?;
    }

    // Verify: newest value should be returned
    {
        let api_snapshot = engine.snapshot();
        let age = api_snapshot.node_property(0, "age");
        assert_eq!(age, Some(nervusdb_api::PropertyValue::Int(30)));
    }

    Ok(())
}

#[test]
fn test_property_remove() -> Result<()> {
    let dir = tempdir()?;
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal)?;

    // Write transaction: set and remove property
    {
        let mut txn = engine.begin_write();
        let node_id = txn.create_node(1, 0)?;
        txn.set_node_property(
            node_id,
            "temp".to_string(),
            PropertyValue::String("value".to_string()),
        );
        txn.remove_node_property(node_id, "temp");
        txn.commit()?;
    }

    // Verify: property should not exist
    {
        let api_snapshot = engine.snapshot();
        let value = api_snapshot.node_property(0, "temp");
        assert_eq!(value, None);
    }

    Ok(())
}

#[test]
fn test_wal_replay_properties() -> Result<()> {
    let dir = tempdir()?;
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    // Create database and write properties
    {
        let engine = GraphEngine::open(&ndb, &wal)?;
        let mut txn = engine.begin_write();
        let node_id = txn.create_node(1, 0)?;
        txn.set_node_property(
            node_id,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        );
        txn.commit()?;
    }

    // Reopen and verify properties are recovered
    {
        let engine = GraphEngine::open(&ndb, &wal)?;
        let api_snapshot = engine.snapshot();
        let name = api_snapshot.node_property(0, "name");
        assert_eq!(
            name,
            Some(nervusdb_api::PropertyValue::String("Bob".to_string()))
        );
    }

    Ok(())
}

#[test]
fn test_multiple_property_types() -> Result<()> {
    let dir = tempdir()?;
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal)?;

    {
        let mut txn = engine.begin_write();
        let node_id = txn.create_node(1, 0)?;
        txn.set_node_property(node_id, "null".to_string(), PropertyValue::Null);
        txn.set_node_property(node_id, "bool".to_string(), PropertyValue::Bool(true));
        txn.set_node_property(node_id, "int".to_string(), PropertyValue::Int(42));
        txn.set_node_property(node_id, "float".to_string(), PropertyValue::Float(2.5));
        txn.set_node_property(
            node_id,
            "string".to_string(),
            PropertyValue::String("hello".to_string()),
        );
        txn.commit()?;
    }

    {
        let api_snapshot = engine.snapshot();
        assert_eq!(
            api_snapshot.node_property(0, "null"),
            Some(nervusdb_api::PropertyValue::Null)
        );
        assert_eq!(
            api_snapshot.node_property(0, "bool"),
            Some(nervusdb_api::PropertyValue::Bool(true))
        );
        assert_eq!(
            api_snapshot.node_property(0, "int"),
            Some(nervusdb_api::PropertyValue::Int(42))
        );
        assert_eq!(
            api_snapshot.node_property(0, "float"),
            Some(nervusdb_api::PropertyValue::Float(2.5))
        );
        assert_eq!(
            api_snapshot.node_property(0, "string"),
            Some(nervusdb_api::PropertyValue::String("hello".to_string()))
        );
    }

    Ok(())
}
