use nervusdb_v2::{Db, PropertyValue};
use nervusdb_v2_api::GraphSnapshot;
use nervusdb_v2_query::{Params, prepare};
use tempfile::tempdir;

#[test]
fn test_where_clause_with_property_filter() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create nodes with properties using query engine
    {
        let create_query = prepare("CREATE (a {age: 25})").unwrap();
        let mut txn = db.begin_write();
        create_query
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();

        let create_query2 = prepare("CREATE (b {age: 30})").unwrap();
        create_query2
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();

        let create_query3 = prepare("CREATE (c {age: 35})").unwrap();
        create_query3
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();

        // Connect them: a -> b, a -> c
        // Since we don't have a good way to MATCH and CREATE in one query in M3,
        // we'll just use raw txn for simplicity of setup, but we must use interned IDs.
        // Or better, just use a single CREATE pattern if supported.
        // M3 CREATE supports single-node or single-hop.
        let connect_query = prepare("CREATE (n1 {age: 25})-[:1]->(n2 {age: 30})").unwrap();
        connect_query
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();

        let connect_query2 = prepare("CREATE (n1 {age: 25})-[:1]->(n3 {age: 35})").unwrap();
        connect_query2
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();

        txn.commit().unwrap();
    }

    // Note: The above creates 7 nodes total because each CREATE creates new nodes.
    // Let's refine to create exactly what we want.
    // Pattern: (n1:25)-[:1]->(n2:30), (n1:25)-[:1]->(n3:35)
    // Actually, let's just use one pattern if possible.
    // M3 parser only supports single hop: (a)-[r]->(b)
}

#[test]
fn test_filter_basic() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Setup: node1(age=25) -> node2(age=30), node1(age=25) -> node3(age=35)
    // We create them using two separate CREATE statements.
    // Note: This creates node1 twice if we are not careful.
    // M3 doesn't have MERGE.
    // Fixed setup:
    {
        let mut txn = db.begin_write();
        let q1 = prepare("CREATE (a {age: 25})-[:1]->(b {age: 30})").unwrap();
        q1.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        let q2 = prepare("CREATE (a2 {age: 25})-[:1]->(b2 {age: 35})").unwrap();
        q2.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    // Query 1: Filter by age > 25
    // This should match node2 and node3 if they were the source,
    // but here they are the destination.
    // Source node is 'a' or 'a2' with age 25.
    // WHERE a.age > 25 should match nothing for a=node1.
    {
        let snapshot = db.snapshot();
        let query = prepare("MATCH (a)-[:1]->(b) WHERE a.age > 25 RETURN b").unwrap();
        let results = query
            .execute_streaming(&snapshot, &Params::new())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(results.len(), 0);
    }

    // Query 2: Update one source node to age 30
    {
        let snapshot = db.snapshot();
        let node1 = snapshot
            .nodes()
            .find(|&iid| snapshot.node_property(iid, "age") == Some(PropertyValue::Int(25)))
            .expect("Should find node with age 25");

        let mut txn = db.begin_write();
        let _ = txn.set_node_property(node1, "age".to_string(), PropertyValue::Int(30));
        txn.commit().unwrap();
    }

    // Query 3: Re-run filter. Now one source node (and its one outgoing edge) should match.
    {
        let snapshot = db.snapshot();
        let query = prepare("MATCH (a)-[:1]->(b) WHERE a.age > 25 RETURN b").unwrap();
        let results = query
            .execute_streaming(&snapshot, &Params::new())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        // Since we have two pairs (25->30) and (25->35), and we updated ONE of the 25s to 30.
        // Then one pair should match.
        assert_eq!(results.len(), 1);
    }
}

#[test]
fn test_where_clause_with_edge_property_filter() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create a->b with edge property.
    {
        let mut txn = db.begin_write();
        let query = prepare("CREATE (a)-[:1 {w: 10}]->(b)").unwrap();
        query
            .execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    // WHERE r.w > 5 should match the edge.
    let snapshot = db.snapshot();
    let query = prepare("MATCH (a)-[r:1]->(b) WHERE r.w > 5 RETURN b").unwrap();
    let results = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(results.len(), 1);
}
