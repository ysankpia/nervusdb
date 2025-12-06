//! Integration test for FlexBuffers binary property storage
//!
//! Tests:
//! 1. Binary property roundtrip (FlexBuffers)
//! 2. Backward compatibility (reading old JSON properties)
//! 3. Performance comparison (FlexBuffers vs JSON)

use nervusdb_core::storage::property::{deserialize_properties, serialize_properties};
use nervusdb_core::{Database, Options};
use serde_json::json;
use std::collections::HashMap;
use tempfile::tempdir;

#[test]
fn test_binary_property_roundtrip() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Add a triple
    let triple = db
        .add_fact(nervusdb_core::Fact::new("alice", "age", "30"))
        .unwrap();

    // Set properties using binary format
    let mut props = HashMap::new();
    props.insert("value".to_string(), json!(30));
    props.insert("unit".to_string(), json!("years"));

    let binary_data = serialize_properties(&props).unwrap();

    // Use the new binary API
    db.set_node_property_binary(triple.subject_id, &binary_data)
        .unwrap();

    // Read back using binary API
    let retrieved = db
        .get_node_property_binary(triple.subject_id)
        .unwrap()
        .unwrap();
    let retrieved_props = deserialize_properties(&retrieved).unwrap();

    assert_eq!(retrieved_props.get("value").unwrap().as_i64().unwrap(), 30);
    assert_eq!(
        retrieved_props.get("unit").unwrap().as_str().unwrap(),
        "years"
    );
}

// Test removed: backward compatibility with JSON format is no longer supported
// NervusDB v2.0+ uses FlexBuffers-only storage for unified data format

#[test]
fn test_edge_properties_binary() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    let triple = db
        .add_fact(nervusdb_core::Fact::new("alice", "knows", "bob"))
        .unwrap();

    // Set edge properties
    let mut props = HashMap::new();
    props.insert("since".to_string(), json!(2020));
    props.insert("strength".to_string(), json!(0.85));

    let binary_data = serialize_properties(&props).unwrap();
    db.set_edge_property_binary(
        triple.subject_id,
        triple.predicate_id,
        triple.object_id,
        &binary_data,
    )
    .unwrap();

    // Read back
    let retrieved = db
        .get_edge_property_binary(triple.subject_id, triple.predicate_id, triple.object_id)
        .unwrap()
        .unwrap();

    let retrieved_props = deserialize_properties(&retrieved).unwrap();

    assert_eq!(
        retrieved_props.get("since").unwrap().as_i64().unwrap(),
        2020
    );
    assert_eq!(
        retrieved_props.get("strength").unwrap().as_f64().unwrap(),
        0.85
    );
}

#[test]
fn test_complex_nested_properties() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    let triple = db
        .add_fact(nervusdb_core::Fact::new(
            "project",
            "hasMetadata",
            "metadata",
        ))
        .unwrap();

    // Complex nested structure
    let mut props = HashMap::new();
    props.insert(
        "metadata".to_string(),
        json!({
            "tags": ["rust", "database", "graph"],
            "contributors": [
                {"name": "Alice", "commits": 150},
                {"name": "Bob", "commits": 89}
            ],
            "stats": {
                "stars": 1000,
                "forks": 42,
                "issues": {
                    "open": 5,
                    "closed": 95
                }
            }
        }),
    );

    let binary_data = serialize_properties(&props).unwrap();
    db.set_node_property_binary(triple.subject_id, &binary_data)
        .unwrap();

    // Read back and verify structure
    let retrieved = db
        .get_node_property_binary(triple.subject_id)
        .unwrap()
        .unwrap();
    let retrieved_props = deserialize_properties(&retrieved).unwrap();

    let metadata = retrieved_props.get("metadata").unwrap();
    assert_eq!(metadata["tags"][0].as_str().unwrap(), "rust");
    assert_eq!(
        metadata["contributors"][0]["name"].as_str().unwrap(),
        "Alice"
    );
    assert_eq!(
        metadata["contributors"][0]["commits"].as_i64().unwrap(),
        150
    );
    assert_eq!(metadata["stats"]["stars"].as_i64().unwrap(), 1000);
    assert_eq!(metadata["stats"]["issues"]["open"].as_i64().unwrap(), 5);
}

#[test]
fn test_persistence_across_reopens() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let node_id = {
        let mut db = Database::open(Options::new(&db_path)).unwrap();
        let triple = db
            .add_fact(nervusdb_core::Fact::new("alice", "age", "30"))
            .unwrap();

        let mut props = HashMap::new();
        props.insert("value".to_string(), json!(30));
        let binary_data = serialize_properties(&props).unwrap();

        db.set_node_property_binary(triple.subject_id, &binary_data)
            .unwrap();

        triple.subject_id
    };

    // Reopen database
    let db = Database::open(Options::new(&db_path)).unwrap();
    let retrieved = db.get_node_property_binary(node_id).unwrap().unwrap();
    let retrieved_props = deserialize_properties(&retrieved).unwrap();

    assert_eq!(retrieved_props.get("value").unwrap().as_i64().unwrap(), 30);
}
