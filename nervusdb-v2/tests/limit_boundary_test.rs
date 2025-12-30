use nervusdb_v2::Db;
use nervusdb_v2_api::GraphSnapshot;
use nervusdb_v2_query::{Params, Result, Value, prepare};
use tempfile::tempdir;

#[test]
fn test_limit_zero_returns_empty() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create some data
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (n)-[:1]->(m)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let q = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 0").unwrap();
    let snapshot = db.snapshot();

    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert!(rows.is_empty(), "LIMIT 0 should return empty result");
}

#[test]
fn test_limit_larger_than_results() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create 5 pairs = 5 edges
    {
        let mut txn = db.begin_write();
        for _ in 0..5 {
            let q = prepare("CREATE (n)-[:1]->(m)").unwrap();
            q.execute_write(&db.snapshot(), &mut txn, &Params::new())
                .unwrap();
        }
        txn.commit().unwrap();
    }

    let q = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 1000").unwrap();
    let snapshot = db.snapshot();

    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    // Should return all available results (5 edges)
    assert_eq!(rows.len(), 5);
}

#[test]
fn test_limit_with_no_matching_edges() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create some data with rel type "1"
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (n)-[:1]->(m)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let q = prepare("MATCH (n)-[:999]->(m) RETURN n, m LIMIT 10").unwrap();
    let snapshot = db.snapshot();

    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert!(rows.is_empty(), "No matching edges should return empty");
}

#[test]
fn test_return_one_limit_5() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // RETURN 1 with LIMIT
    let q = prepare("RETURN 1 LIMIT 5").unwrap();

    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::Int(1));
}

#[test]
fn test_return_one_limit_100() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // RETURN 1 with large LIMIT - should still return 1 row
    let q = prepare("RETURN 1 LIMIT 100").unwrap();

    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::Int(1));
}

#[test]
fn test_match_limit_1() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create 10 edges
    {
        let mut txn = db.begin_write();
        for _ in 0..10 {
            let q = prepare("CREATE (n)-[:1]->(m)").unwrap();
            q.execute_write(&db.snapshot(), &mut txn, &Params::new())
                .unwrap();
        }
        txn.commit().unwrap();
    }

    let q = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 1").unwrap();
    let snapshot = db.snapshot();

    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
}

#[test]
fn test_match_limit_5() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create 10 edges
    {
        let mut txn = db.begin_write();
        for _ in 0..10 {
            let q = prepare("CREATE (n)-[:1]->(m)").unwrap();
            q.execute_write(&db.snapshot(), &mut txn, &Params::new())
                .unwrap();
        }
        txn.commit().unwrap();
    }

    let q = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 5").unwrap();
    let snapshot = db.snapshot();

    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 5);
}

#[test]
fn test_match_external_id_projection() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create a node with external ID
    // M3 CREATE doesn't support setting external ID directly in syntax?
    // Actually CREATE (n) uses default label and 0 external ID in execution.
    // Let's use raw txn for external ID setup.
    {
        let mut txn = db.begin_write();
        // Use query engine's trait methods via the implementation on WriteTxn
        use nervusdb_v2_query::WriteableGraph;

        let label_id = txn.get_or_create_label_id("User").unwrap();
        let rel_id = txn.get_or_create_rel_type_id("1").unwrap();

        let n1 = txn.create_node(100, label_id).unwrap();
        let n2 = txn.create_node(200, label_id).unwrap();
        txn.create_edge(n1, rel_id, n2); // Returns ()
        txn.commit().unwrap();
    }

    let q = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 5").unwrap();
    let snapshot = db.snapshot();

    let rows: Vec<_> = q
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 1);

    // Verify the row contains both node IDs
    let cols = rows[0].columns();
    assert_eq!(cols[0].0, "n");
    assert_eq!(cols[1].0, "m");

    // Check if we can resolve external IDs via snapshot
    if let Value::NodeId(iid) = cols[0].1 {
        assert_eq!(snapshot.resolve_external(iid), Some(100));
    } else {
        panic!("Expected NodeId");
    }
}
