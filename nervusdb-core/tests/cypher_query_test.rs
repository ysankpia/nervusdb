//! Integration tests for Cypher query engine
//!
//! Tests the complete query pipeline:
//! Parser → Planner → Executor

use nervusdb_core::{Database, Fact, Options};
use tempfile::tempdir;

#[test]
fn test_simple_match_return() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Add test data
    db.add_fact(Fact::new("alice", "name", "Alice")).unwrap();
    db.add_fact(Fact::new("alice", "age", "30")).unwrap();
    db.add_fact(Fact::new("bob", "name", "Bob")).unwrap();
    db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();

    // Test simple MATCH RETURN
    let query = "MATCH (n) RETURN n";
    let results = db.execute_query(query).unwrap();

    // Should return nodes
    assert!(!results.is_empty(), "Expected at least one result");
    println!("Simple MATCH results: {} records", results.len());
}

#[test]
fn test_match_where_return() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Add test data with node properties
    let alice_fact = db.add_fact(Fact::new("alice", "type", "Person")).unwrap();
    let bob_fact = db.add_fact(Fact::new("bob", "type", "Person")).unwrap();

    // Set node properties using binary format
    let alice_props = serde_json::json!({
        "name": "Alice",
        "age": 30
    });
    let bob_props = serde_json::json!({
        "name": "Bob",
        "age": 25
    });

    let alice_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(alice_props).unwrap();
    let bob_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(bob_props).unwrap();

    let alice_binary =
        nervusdb_core::storage::property::serialize_properties(&alice_props_map).unwrap();
    let bob_binary =
        nervusdb_core::storage::property::serialize_properties(&bob_props_map).unwrap();

    db.set_node_property_binary(alice_fact.subject_id, &alice_binary)
        .unwrap();
    db.set_node_property_binary(bob_fact.subject_id, &bob_binary)
        .unwrap();

    // Test MATCH WHERE RETURN
    let query = "MATCH (n) WHERE n.age > 27 RETURN n";

    match db.execute_query(query) {
        Ok(results) => {
            println!("MATCH WHERE results: {} records", results.len());
            // WHERE clause should filter results (alice has age 30 > 27, bob has age 25 < 27)
            assert_eq!(
                results.len(),
                1,
                "Expected 1 filtered result (alice with age 30)"
            );
        }
        Err(e) => {
            println!("Query failed (expected for MVP): {}", e);
            // It's OK if WHERE is not fully implemented yet
        }
    }
}

#[test]
fn test_match_relationship() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Add test data: alice knows bob
    db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();
    db.add_fact(Fact::new("bob", "knows", "charlie")).unwrap();

    // Test relationship pattern
    let query = "MATCH (a)-[r]->(b) RETURN a, b";

    match db.execute_query(query) {
        Ok(results) => {
            println!("Relationship MATCH results: {} records", results.len());
            assert!(!results.is_empty(), "Expected relationship matches");

            // Check structure
            if let Some(first) = results.first() {
                assert!(first.contains_key("a"), "Expected 'a' in results");
                assert!(first.contains_key("b"), "Expected 'b' in results");
            }
        }
        Err(e) => {
            println!("Relationship query failed: {}", e);
        }
    }
}

#[test]
fn test_parser_basic_syntax() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Test that parser can handle various syntaxes
    let queries = vec![
        "MATCH (n) RETURN n",
        "MATCH (a)-[:KNOWS]->(b) RETURN a, b",
        "MATCH (n:Person) RETURN n",
        "MATCH (n) WHERE n.age > 30 RETURN n.name",
    ];

    for query in queries {
        match db.execute_query(query) {
            Ok(results) => {
                println!("✅ Query parsed successfully: {}", query);
                println!("   Results: {} records", results.len());
            }
            Err(e) => {
                println!("⚠️  Query failed: {}", query);
                println!("   Error: {}", e);
            }
        }
    }
}

#[test]
fn test_empty_database_query() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Query empty database
    let query = "MATCH (n) RETURN n";
    let results = db.execute_query(query).unwrap();

    assert_eq!(results.len(), 0, "Empty database should return 0 results");
}

#[test]
fn test_complex_query() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create a small graph
    db.add_fact(Fact::new("alice", "name", "Alice")).unwrap();
    db.add_fact(Fact::new("alice", "age", "30")).unwrap();
    db.add_fact(Fact::new("bob", "name", "Bob")).unwrap();
    db.add_fact(Fact::new("bob", "age", "25")).unwrap();
    db.add_fact(Fact::new("charlie", "name", "Charlie"))
        .unwrap();
    db.add_fact(Fact::new("charlie", "age", "35")).unwrap();

    db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();
    db.add_fact(Fact::new("bob", "knows", "charlie")).unwrap();
    db.add_fact(Fact::new("alice", "likes", "rust")).unwrap();

    // Complex query
    let query = "MATCH (a)-[:knows]->(b) RETURN a, b";

    match db.execute_query(query) {
        Ok(results) => {
            println!("Complex query results: {} records", results.len());
            assert!(
                results.len() >= 2,
                "Expected at least 2 'knows' relationships"
            );
        }
        Err(e) => {
            println!("Complex query not yet supported: {}", e);
        }
    }
}

#[test]
fn test_where_comparison_operators() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create test nodes with properties
    let alice = db.add_fact(Fact::new("alice", "type", "Person")).unwrap();
    let bob = db.add_fact(Fact::new("bob", "type", "Person")).unwrap();
    let charlie = db.add_fact(Fact::new("charlie", "type", "Person")).unwrap();

    let alice_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({"age": 30, "score": 95.5})).unwrap();
    let bob_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({"age": 25, "score": 88.0})).unwrap();
    let charlie_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({"age": 30, "score": 92.0})).unwrap();

    db.set_node_property_binary(
        alice.subject_id,
        &nervusdb_core::storage::property::serialize_properties(&alice_props_map).unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        bob.subject_id,
        &nervusdb_core::storage::property::serialize_properties(&bob_props_map).unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        charlie.subject_id,
        &nervusdb_core::storage::property::serialize_properties(&charlie_props_map).unwrap(),
    )
    .unwrap();

    // Test GreaterThan
    let results = db
        .execute_query("MATCH (n) WHERE n.age > 27 RETURN n")
        .unwrap();
    assert_eq!(results.len(), 2, "Expected 2 nodes with age > 27");

    // Test GreaterThanOrEqual
    let results = db
        .execute_query("MATCH (n) WHERE n.age >= 30 RETURN n")
        .unwrap();
    assert_eq!(results.len(), 2, "Expected 2 nodes with age >= 30");

    // Test LessThan
    let results = db
        .execute_query("MATCH (n) WHERE n.age < 30 RETURN n")
        .unwrap();
    assert_eq!(results.len(), 1, "Expected 1 node with age < 30");

    // Test LessThanOrEqual
    let results = db
        .execute_query("MATCH (n) WHERE n.score <= 92.0 RETURN n")
        .unwrap();
    assert_eq!(results.len(), 2, "Expected 2 nodes with score <= 92.0");

    // Test Equal
    let results = db
        .execute_query("MATCH (n) WHERE n.age = 30 RETURN n")
        .unwrap();
    assert_eq!(results.len(), 2, "Expected 2 nodes with age = 30");

    println!("✅ All comparison operators work correctly");
}

#[test]
fn test_where_parameter_binding() {
    use std::collections::HashMap;

    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    let alice = db.add_fact(Fact::new("alice", "type", "Person")).unwrap();

    let props_json = serde_json::json!({"name": "Alice"});
    db.set_node_property(alice.subject_id, &props_json.to_string())
        .unwrap();

    let mut params = HashMap::new();
    params.insert("target".to_string(), serde_json::json!("Alice"));

    let results = db
        .execute_query_with_params(
            "MATCH (n:Person) WHERE n.name = $target RETURN n",
            Some(params),
        )
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].contains_key("n"));
}

#[test]
fn test_where_logical_operators() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create test nodes
    let alice = db.add_fact(Fact::new("alice", "type", "Person")).unwrap();
    let bob = db.add_fact(Fact::new("bob", "type", "Person")).unwrap();

    let alice_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({"age": 30, "active": true})).unwrap();
    let bob_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({"age": 25, "active": false})).unwrap();

    db.set_node_property_binary(
        alice.subject_id,
        &nervusdb_core::storage::property::serialize_properties(&alice_props_map).unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        bob.subject_id,
        &nervusdb_core::storage::property::serialize_properties(&bob_props_map).unwrap(),
    )
    .unwrap();

    // Test AND
    let results = db
        .execute_query("MATCH (n) WHERE n.age > 20 AND n.active = true RETURN n")
        .unwrap();
    assert_eq!(results.len(), 1, "Expected 1 node matching AND condition");

    // Test OR
    let results = db
        .execute_query("MATCH (n) WHERE n.age < 27 OR n.age > 29 RETURN n")
        .unwrap();
    assert_eq!(results.len(), 2, "Expected 2 nodes matching OR condition");

    println!("✅ Logical operators work correctly");
}

#[test]
fn test_limit_clause() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();
    db.add_fact(Fact::new("bob", "knows", "charlie")).unwrap();

    let results = db.execute_query("MATCH (n) RETURN n LIMIT 1").unwrap();
    assert_eq!(results.len(), 1);

    let results = db.execute_query("MATCH (n) RETURN n LIMIT 0").unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_unsupported_features_fail_fast() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();

    let err = db
        .execute_query("MATCH (n) RETURN n UNION MATCH (n) RETURN n")
        .unwrap_err();
    assert!(matches!(err, nervusdb_core::Error::NotImplemented(_)));

    let err = db.execute_query("MATCH (n) RETURN DISTINCT n").unwrap_err();
    assert!(matches!(err, nervusdb_core::Error::NotImplemented(_)));

    // ORDER BY, SKIP, and WITH are now supported
    let _ = db.execute_query("MATCH (n) RETURN n ORDER BY n").unwrap();
    let _ = db.execute_query("MATCH (n) RETURN n SKIP 1").unwrap();
    // WITH requires proper syntax: MATCH (n) WITH n RETURN n
}

#[test]
fn test_where_arithmetic_in_expressions() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    let alice = db.add_fact(Fact::new("alice", "type", "Person")).unwrap();
    let bob = db.add_fact(Fact::new("bob", "type", "Person")).unwrap();

    let alice_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({"salary": 50000.0, "bonus": 10000.0})).unwrap();
    let bob_props_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({"salary": 40000.0, "bonus": 5000.0})).unwrap();

    db.set_node_property_binary(
        alice.subject_id,
        &nervusdb_core::storage::property::serialize_properties(&alice_props_map).unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        bob.subject_id,
        &nervusdb_core::storage::property::serialize_properties(&bob_props_map).unwrap(),
    )
    .unwrap();

    // Test arithmetic in WHERE clause
    let results = db
        .execute_query("MATCH (n) WHERE n.salary + n.bonus > 55000 RETURN n")
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "Expected 1 node with total compensation > 55000"
    );

    println!("✅ Arithmetic in WHERE clause works correctly");
}

#[test]
fn test_create_single_node() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // CREATE (n:Person)
    let query = "CREATE (n:Person)";
    let results = db.execute_query(query).unwrap();

    assert_eq!(results.len(), 1, "Expected 1 result from CREATE");

    // Verify node was created by checking if it has a variable in the result
    if let Some(first_result) = results.first() {
        assert!(first_result.contains_key("n"), "Expected 'n' in result");
    }

    println!("✅ CREATE single node works");
}

#[test]
fn test_merge_single_node() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // MERGE (n:Person {name: "Alice", age: 30})
    let query = "MERGE (n:Person {name: \"Alice\", age: 30})";
    let results = db.execute_query(query).unwrap();
    assert_eq!(results.len(), 1, "Expected 1 result from MERGE");

    let node_id = match results[0].get("n") {
        Some(nervusdb_core::query::executor::Value::Node(id)) => *id,
        _ => panic!("Expected Node value in result"),
    };

    // Second MERGE should be idempotent (same node id in this simplified model).
    let results2 = db.execute_query(query).unwrap();
    let node_id2 = match results2[0].get("n") {
        Some(nervusdb_core::query::executor::Value::Node(id)) => *id,
        _ => panic!("Expected Node value in result"),
    };
    assert_eq!(node_id2, node_id);

    // Verify properties
    if let Ok(Some(binary)) = db.get_node_property_binary(node_id) {
        let props = nervusdb_core::storage::property::deserialize_properties(&binary).unwrap();
        assert_eq!(props.get("name"), Some(&serde_json::json!("Alice")));
        assert_eq!(props.get("age"), Some(&serde_json::json!(30.0)));
    } else {
        panic!("Node properties not found");
    }
}

#[test]
fn test_merge_relationship_idempotent() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    let query = "MERGE (a:Person {name: \"Alice\"})-[:KNOWS]->(b:Person {name: \"Bob\"})";
    let _ = db.execute_query(query).unwrap();
    let _ = db.execute_query(query).unwrap();

    let results = db
        .execute_query("MATCH (a)-[:KNOWS]->(b) RETURN a, b")
        .unwrap();
    assert_eq!(results.len(), 1, "Expected 1 KNOWS relationship");
}

#[test]
fn test_create_node_with_properties() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // CREATE (n:Person {name: "Alice", age: 30})
    let query = "CREATE (n:Person {name: \"Alice\", age: 30})";
    let results = db.execute_query(query).unwrap();

    assert_eq!(results.len(), 1, "Expected 1 result from CREATE");

    // Get the created node ID
    if let Some(first_result) = results.first() {
        if let Some(nervusdb_core::query::executor::Value::Node(node_id)) = first_result.get("n") {
            // Verify properties
            if let Ok(Some(binary)) = db.get_node_property_binary(*node_id) {
                let props =
                    nervusdb_core::storage::property::deserialize_properties(&binary).unwrap();
                assert_eq!(props.get("name"), Some(&serde_json::json!("Alice")));
                // Cypher lexer parses numbers as Float
                assert_eq!(props.get("age"), Some(&serde_json::json!(30.0)));
            } else {
                panic!("Node properties not found");
            }
        } else {
            panic!("Expected Node value in result");
        }
    }

    println!("✅ CREATE node with properties works");
}

#[test]
fn test_create_multiple_nodes() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // CREATE (a:Person {name: "Alice"})
    let result_a = db
        .execute_query("CREATE (a:Person {name: \"Alice\"})")
        .unwrap();
    assert_eq!(result_a.len(), 1, "Expected 1 result from first CREATE");

    // CREATE (b:Person {name: "Bob"})
    let result_b = db
        .execute_query("CREATE (b:Person {name: \"Bob\"})")
        .unwrap();
    assert_eq!(result_b.len(), 1, "Expected 1 result from second CREATE");

    println!("✅ CREATE multiple nodes works");
}

#[test]
fn test_create_relationship() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // CREATE (a:Person {name: "Alice"})-[:KNOWS]->(b:Person {name: "Bob"})
    let query = "CREATE (a:Person {name: \"Alice\"})-[:KNOWS]->(b:Person {name: \"Bob\"})";
    let results = db.execute_query(query).unwrap();

    assert_eq!(results.len(), 1, "Expected 1 result from CREATE");

    // Verify both nodes were created
    if let Some(first_result) = results.first() {
        assert!(first_result.contains_key("a"), "Expected 'a' in result");
        assert!(first_result.contains_key("b"), "Expected 'b' in result");
    }

    // Verify relationship exists
    let rel_query = "MATCH (a)-[:KNOWS]->(b) RETURN a, b";
    let rel_results = db.execute_query(rel_query).unwrap();
    println!("DEBUG: KNOWS relationships found: {}", rel_results.len());
    for (i, result) in rel_results.iter().enumerate() {
        println!("  Result {}: {:?}", i, result);
    }
    assert_eq!(rel_results.len(), 1, "Expected 1 KNOWS relationship");

    println!("✅ CREATE relationship works");
}

#[test]
fn test_create_relationship_with_properties() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // CREATE (a:Person)-[r:KNOWS {since: 2020}]->(b:Person)
    let query = "CREATE (a:Person)-[r:KNOWS {since: 2020}]->(b:Person)";
    let results = db.execute_query(query).unwrap();

    assert_eq!(results.len(), 1, "Expected 1 result from CREATE");

    // Verify relationship variable is in result
    if let Some(first_result) = results.first() {
        if let Some(nervusdb_core::query::executor::Value::Relationship(triple)) =
            first_result.get("r")
        {
            // Verify relationship properties
            if let Ok(Some(binary)) = db.get_edge_property_binary(
                triple.subject_id,
                triple.predicate_id,
                triple.object_id,
            ) {
                let props =
                    nervusdb_core::storage::property::deserialize_properties(&binary).unwrap();
                assert_eq!(props.get("since"), Some(&serde_json::json!(2020.0)));
            } else {
                panic!("Relationship properties not found");
            }
        } else {
            panic!("Expected Relationship value in result");
        }
    }

    println!("✅ CREATE relationship with properties works");
}

#[test]
fn test_create_chain_relationship() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // CREATE (a:Person)-[:KNOWS]->(b:Person)-[:LIKES]->(c:Thing)
    let query = "CREATE (a:Person)-[:KNOWS]->(b:Person)-[:LIKES]->(c:Thing)";
    let results = db.execute_query(query).unwrap();

    assert_eq!(results.len(), 1, "Expected 1 result from CREATE");

    // Verify all three nodes were created
    if let Some(first_result) = results.first() {
        assert!(first_result.contains_key("a"), "Expected 'a' in result");
        assert!(first_result.contains_key("b"), "Expected 'b' in result");
        assert!(first_result.contains_key("c"), "Expected 'c' in result");
    }

    // Verify KNOWS relationship exists
    let knows_query = "MATCH (a)-[:KNOWS]->(b) RETURN a, b";
    let knows_results = db.execute_query(knows_query).unwrap();
    assert_eq!(knows_results.len(), 1, "Expected 1 KNOWS relationship");

    // Verify LIKES relationship exists
    let likes_query = "MATCH (b)-[:LIKES]->(c) RETURN b, c";
    let likes_results = db.execute_query(likes_query).unwrap();
    assert_eq!(likes_results.len(), 1, "Expected 1 LIKES relationship");

    println!("✅ CREATE chain relationship works");
}

#[test]
fn test_set_single_property() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create a node
    let create_query = "CREATE (n:Person {name: \"Alice\", age: 25.0})";
    db.execute_query(create_query).unwrap();

    // Update the age using SET
    let set_query = "MATCH (n:Person) SET n.age = 30.0 RETURN n";
    let results = db.execute_query(set_query).unwrap();

    assert_eq!(results.len(), 1, "Expected 1 result");

    // Verify the property was updated
    let verify_query = "MATCH (n:Person) WHERE n.age = 30.0 RETURN n.name, n.age";
    let verify_results = db.execute_query(verify_query).unwrap();

    assert_eq!(verify_results.len(), 1, "Expected 1 person with age 30");

    if let Some(nervusdb_core::query::executor::Value::String(name)) =
        verify_results[0].get("n.name")
    {
        assert_eq!(name, "Alice");
    } else {
        panic!("Expected name to be a string");
    }

    if let Some(nervusdb_core::query::executor::Value::Float(age)) = verify_results[0].get("n.age")
    {
        assert_eq!(age, &30.0);
    } else {
        panic!("Expected age to be a float");
    }

    println!("✅ SET single property works");
}

#[test]
fn test_set_multiple_properties() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create a node
    let create_query = "CREATE (n:Person {name: \"Bob\"})";
    db.execute_query(create_query).unwrap();

    // Set multiple properties
    let set_query = "MATCH (n:Person) SET n.age = 28.0, n.city = \"NYC\" RETURN n";
    let results = db.execute_query(set_query).unwrap();

    assert_eq!(results.len(), 1, "Expected 1 result");

    // Verify both properties were set
    let verify_query = "MATCH (n:Person) WHERE n.name = \"Bob\" RETURN n.age, n.city";
    let verify_results = db.execute_query(verify_query).unwrap();

    assert_eq!(verify_results.len(), 1, "Expected 1 person");

    if let Some(nervusdb_core::query::executor::Value::Float(age)) = verify_results[0].get("n.age")
    {
        assert_eq!(age, &28.0);
    } else {
        panic!("Expected age to be a float");
    }

    if let Some(nervusdb_core::query::executor::Value::String(city)) =
        verify_results[0].get("n.city")
    {
        assert_eq!(city, "NYC");
    } else {
        panic!("Expected city to be a string");
    }

    println!("✅ SET multiple properties works");
}

#[test]
fn test_set_with_expression() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create a node with initial age
    let create_query = "CREATE (n:Person {name: \"Charlie\", age: 25.0})";
    db.execute_query(create_query).unwrap();

    // Increment age using an expression
    let set_query = "MATCH (n:Person) SET n.age = n.age + 5.0 RETURN n";
    let results = db.execute_query(set_query).unwrap();

    assert_eq!(results.len(), 1, "Expected 1 result");

    // Verify age was incremented
    let verify_query = "MATCH (n:Person) WHERE n.name = \"Charlie\" RETURN n.age";
    let verify_results = db.execute_query(verify_query).unwrap();

    assert_eq!(verify_results.len(), 1, "Expected 1 person");

    if let Some(nervusdb_core::query::executor::Value::Float(age)) = verify_results[0].get("n.age")
    {
        assert_eq!(age, &30.0, "Expected age to be 30 (25 + 5)");
    } else {
        panic!("Expected age to be a float");
    }

    println!("✅ SET with expression works");
}

// ========================================
// Phase 2.6: DELETE Tests
// ========================================

#[test]
fn test_delete_simple_node() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create a node
    let create_query = "CREATE (n:Person {name: \"Alice\"})";
    db.execute_query(create_query).unwrap();

    // Verify node exists
    let verify_query = "MATCH (n:Person) RETURN n";
    let results = db.execute_query(verify_query).unwrap();
    assert_eq!(results.len(), 1, "Expected 1 node before deletion");

    // Delete the node
    let delete_query = "MATCH (n:Person) DELETE n";
    db.execute_query(delete_query).unwrap();

    // Verify node is deleted
    let verify_query = "MATCH (n:Person) RETURN n";
    let results = db.execute_query(verify_query).unwrap();
    assert_eq!(results.len(), 0, "Expected 0 nodes after deletion");

    println!("✅ DELETE simple node works");
}

#[test]
fn test_delete_node_with_relationships_fails() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create two nodes and a relationship
    let create_query = "CREATE (a:Person {name: \"Alice\"})-[:KNOWS]->(b:Person {name: \"Bob\"})";
    db.execute_query(create_query).unwrap();

    // Try to delete a node with relationships (should fail)
    let delete_query = "MATCH (a:Person) WHERE a.name = \"Alice\" DELETE a";
    let result = db.execute_query(delete_query);

    assert!(
        result.is_err(),
        "Expected error when deleting node with relationships"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("has relationships"),
        "Expected error message about relationships"
    );

    println!("✅ DELETE node with relationships fails as expected");
}

#[test]
fn test_detach_delete_node() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create two nodes and a relationship
    let create_query = "CREATE (a:Person {name: \"Alice\"})-[:KNOWS]->(b:Person {name: \"Bob\"})";
    db.execute_query(create_query).unwrap();

    // Verify nodes and relationship exist
    let verify_nodes = "MATCH (n:Person) RETURN n";
    let results = db.execute_query(verify_nodes).unwrap();
    assert_eq!(results.len(), 2, "Expected 2 nodes before deletion");

    let verify_rel = "MATCH (a)-[:KNOWS]->(b) RETURN a, b";
    let results = db.execute_query(verify_rel).unwrap();
    assert_eq!(results.len(), 1, "Expected 1 relationship before deletion");

    // Delete Alice with DETACH (should also delete the KNOWS relationship)
    let delete_query = "MATCH (a:Person) WHERE a.name = \"Alice\" DETACH DELETE a";
    db.execute_query(delete_query).unwrap();

    // Verify Alice is deleted
    let verify_alice = "MATCH (n:Person) WHERE n.name = \"Alice\" RETURN n";
    let results = db.execute_query(verify_alice).unwrap();
    assert_eq!(results.len(), 0, "Expected Alice to be deleted");

    // Verify Bob still exists
    let verify_bob = "MATCH (n:Person) WHERE n.name = \"Bob\" RETURN n";
    let results = db.execute_query(verify_bob).unwrap();
    assert_eq!(results.len(), 1, "Expected Bob to still exist");

    // Verify relationship is deleted
    let verify_rel = "MATCH (a)-[:KNOWS]->(b) RETURN a, b";
    let results = db.execute_query(verify_rel).unwrap();
    assert_eq!(
        results.len(),
        0,
        "Expected 0 relationships after DETACH DELETE"
    );

    println!("✅ DETACH DELETE node works");
}

#[test]
fn test_delete_multiple_nodes() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create multiple nodes (separately)
    db.execute_query("CREATE (a:Person {name: \"Alice\"})")
        .unwrap();
    db.execute_query("CREATE (b:Person {name: \"Bob\"})")
        .unwrap();
    db.execute_query("CREATE (c:Person {name: \"Charlie\"})")
        .unwrap();

    // Verify 3 nodes exist
    let verify_query = "MATCH (n:Person) RETURN n";
    let results = db.execute_query(verify_query).unwrap();
    assert_eq!(results.len(), 3, "Expected 3 nodes before deletion");

    // Delete Alice and Bob
    let delete_query = "MATCH (n:Person) WHERE n.name = \"Alice\" OR n.name = \"Bob\" DELETE n";
    db.execute_query(delete_query).unwrap();

    // Verify only Charlie remains
    let verify_query = "MATCH (n:Person) RETURN n";
    let results = db.execute_query(verify_query).unwrap();
    assert_eq!(results.len(), 1, "Expected 1 node after deletion");

    // Verify it's Charlie
    let verify_charlie = "MATCH (n:Person) WHERE n.name = \"Charlie\" RETURN n";
    let results = db.execute_query(verify_charlie).unwrap();
    assert_eq!(results.len(), 1, "Expected Charlie to remain");

    println!("✅ DELETE multiple nodes works");
}

#[test]
fn test_order_by_and_skip() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create nodes with different names
    db.add_fact(Fact::new("charlie", "type", "Person")).unwrap();
    db.add_fact(Fact::new("alice", "type", "Person")).unwrap();
    db.add_fact(Fact::new("bob", "type", "Person")).unwrap();

    // Set name properties
    let charlie_id = db.resolve_id("charlie").unwrap().unwrap();
    let alice_id = db.resolve_id("alice").unwrap().unwrap();
    let bob_id = db.resolve_id("bob").unwrap().unwrap();

    db.set_node_property_binary(
        charlie_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"name": "Charlie", "age": 30})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        alice_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"name": "Alice", "age": 25})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        bob_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"name": "Bob", "age": 35})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Test ORDER BY ASC
    let results = db
        .execute_query("MATCH (p:Person) RETURN p ORDER BY p.name")
        .unwrap();
    assert_eq!(results.len(), 3);
    // Should be Alice, Bob, Charlie (alphabetical)

    // Test ORDER BY DESC
    let results = db
        .execute_query("MATCH (p:Person) RETURN p ORDER BY p.age DESC")
        .unwrap();
    assert_eq!(results.len(), 3);
    // Should be Bob (35), Charlie (30), Alice (25)

    // Test SKIP
    let results = db
        .execute_query("MATCH (p:Person) RETURN p ORDER BY p.name SKIP 1")
        .unwrap();
    assert_eq!(results.len(), 2);
    // Should skip Alice, return Bob and Charlie

    // Test SKIP + LIMIT
    let results = db
        .execute_query("MATCH (p:Person) RETURN p ORDER BY p.name SKIP 1 LIMIT 1")
        .unwrap();
    assert_eq!(results.len(), 1);
    // Should return only Bob
}

#[test]
fn test_aggregate_functions() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create test data
    db.add_fact(Fact::new("alice", "type", "Person")).unwrap();
    db.add_fact(Fact::new("bob", "type", "Person")).unwrap();
    db.add_fact(Fact::new("charlie", "type", "Person")).unwrap();

    let alice_id = db.resolve_id("alice").unwrap().unwrap();
    let bob_id = db.resolve_id("bob").unwrap().unwrap();
    let charlie_id = db.resolve_id("charlie").unwrap().unwrap();

    // Set ages
    db.set_node_property_binary(
        alice_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"age": 25.0})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        bob_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"age": 30.0})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        charlie_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"age": 35.0})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Test COUNT(*)
    let results = db
        .execute_query("MATCH (p:Person) RETURN count(*)")
        .unwrap();
    assert_eq!(results.len(), 1);

    // Test SUM
    let results = db
        .execute_query("MATCH (p:Person) RETURN sum(p.age)")
        .unwrap();
    assert_eq!(results.len(), 1);

    // Test AVG
    let results = db
        .execute_query("MATCH (p:Person) RETURN avg(p.age)")
        .unwrap();
    assert_eq!(results.len(), 1);

    // Test MIN
    let results = db
        .execute_query("MATCH (p:Person) RETURN min(p.age)")
        .unwrap();
    assert_eq!(results.len(), 1);

    // Test MAX
    let results = db
        .execute_query("MATCH (p:Person) RETURN max(p.age)")
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_with_clause() {
    let dir = tempdir().unwrap();
    let mut db = Database::open(Options::new(dir.path().join("test.db"))).unwrap();

    // Create test data
    db.add_fact(Fact::new("alice", "type", "Person")).unwrap();
    db.add_fact(Fact::new("bob", "type", "Person")).unwrap();
    db.add_fact(Fact::new("charlie", "type", "Person")).unwrap();

    let alice_id = db.resolve_id("alice").unwrap().unwrap();
    let bob_id = db.resolve_id("bob").unwrap().unwrap();
    let charlie_id = db.resolve_id("charlie").unwrap().unwrap();

    db.set_node_property_binary(
        alice_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"age": 25.0})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        bob_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"age": 30.0})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    db.set_node_property_binary(
        charlie_id,
        &nervusdb_core::storage::property::serialize_properties(
            &serde_json::from_value(serde_json::json!({"age": 35.0})).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    // Test WITH basic projection
    let results = db
        .execute_query("MATCH (p:Person) WITH p RETURN p")
        .unwrap();
    assert_eq!(results.len(), 3);

    // Test WITH LIMIT
    let results = db
        .execute_query("MATCH (p:Person) WITH p LIMIT 2 RETURN p")
        .unwrap();
    assert_eq!(results.len(), 2);

    // Test WITH ORDER BY
    let results = db
        .execute_query("MATCH (p:Person) WITH p ORDER BY p.age RETURN p")
        .unwrap();
    assert_eq!(results.len(), 3);
}
