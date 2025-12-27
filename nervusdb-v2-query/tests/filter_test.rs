use nervusdb_v2_api::GraphStore;
use nervusdb_v2_query::{Params, prepare};
use nervusdb_v2_storage::engine::GraphEngine;
use nervusdb_v2_storage::property::PropertyValue;
use tempfile::tempdir;

#[test]
fn test_where_clause_with_property_filter() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("test.ndb");
    let wal = dir.path().join("test.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();

    // Create nodes with properties
    {
        let mut txn = engine.begin_write();
        let node1 = txn.create_node(1, 0).unwrap();
        let node2 = txn.create_node(2, 0).unwrap();
        let node3 = txn.create_node(3, 0).unwrap();

        txn.set_node_property(node1, "age".to_string(), PropertyValue::Int(25));
        txn.set_node_property(node2, "age".to_string(), PropertyValue::Int(30));
        txn.set_node_property(node3, "age".to_string(), PropertyValue::Int(35));

        txn.create_edge(node1, 1, node2);
        txn.create_edge(node1, 1, node3);
        txn.commit().unwrap();
    }

    // Query with WHERE clause: filter by source node property
    {
        let query = prepare("MATCH (a)-[:1]->(b) WHERE a.age > 25 RETURN b").unwrap();
        let snapshot = engine.snapshot();
        let params = Params::new();
        let results: Vec<_> = query
            .execute_streaming(&snapshot, &params)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        // node1 has age=25, so WHERE a.age > 25 should filter out all edges from node1
        // Result should be empty
        assert_eq!(results.len(), 0);
    }

    // Query with WHERE clause: filter by source node property (should match)
    {
        // First, update node1's age to 30
        {
            let mut txn = engine.begin_write();
            txn.set_node_property(0, "age".to_string(), PropertyValue::Int(30));
            txn.commit().unwrap();
        }

        let query = prepare("MATCH (a)-[:1]->(b) WHERE a.age > 25 RETURN b").unwrap();
        let snapshot = engine.snapshot();
        let params = Params::new();
        let results: Vec<_> = query
            .execute_streaming(&snapshot, &params)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        // Now node1 has age=30 > 25, so both edges should be returned
        assert_eq!(results.len(), 2);
    }
}
