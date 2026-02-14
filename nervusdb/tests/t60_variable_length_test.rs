use nervusdb::Db;
use nervusdb_query::facade::query_collect;
use nervusdb_query::{Params, Result, prepare};
use tempfile::tempdir;

#[test]
fn test_single_hop_pattern() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create: (a)-[:1]->(b)-[:1]->(c)
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        let q2 = prepare("CREATE (b)-[:1]->(c)").unwrap();
        q2.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    // Single hop: a->b
    let query = prepare("MATCH (a)-[:1]->(b) RETURN a, b").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    // We expect 2 rows: a->b and b->c
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_variable_length_star() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create a chain: (0)-[:1]->(1)-[:1]->(2)-[:1]->(3)
    {
        let mut txn = db.begin_write();
        for _ in 0..3 {
            let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
            q.execute_write(&db.snapshot(), &mut txn, &Params::new())
                .unwrap();
        }
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    // Query: a-[*]->d (any path from any node)
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*]->(d) RETURN a, d",
        &Params::new(),
    )
    .unwrap();

    // With 3 separate edges (not a chain), each edge is a valid path
    assert!(!rows.is_empty(), "Should find at least one path");
}

#[test]
fn test_variable_length_range() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create chain: single edge
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    // Query: a-[*1..2]->c (1 or 2 hops)
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*1..2]->(c) RETURN a, c",
        &Params::new(),
    )
    .unwrap();

    // Should find at least the 1-hop path
    assert!(!rows.is_empty(), "Should find at least 1 path");
}

#[test]
fn test_variable_length_min_only() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create 2-hop chain: a -> b -> c (connected nodes)
    {
        let mut txn = db.begin_write();
        use nervusdb_query::WriteableGraph;
        let rel_id = txn.get_or_create_rel_type_id("1").unwrap();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        txn.create_edge(a, rel_id, b); // a -> b
        txn.create_edge(b, rel_id, c); // b -> c
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    // a-[*2..]->c: minimum 2 hops
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*2..]->(c) RETURN a, c",
        &Params::new(),
    )
    .unwrap();

    // Should find the a->b->c path (2 hops)
    assert!(
        !rows.is_empty(),
        "Should find at least 1 path with min 2 hops"
    );
}

#[test]
fn test_variable_length_max_only() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create chain
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    // a-[*..2]->b: maximum 2 hops
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*..2]->(b) RETURN a, b",
        &Params::new(),
    )
    .unwrap();

    // Should find at least the 1-hop path
    assert!(
        !rows.is_empty(),
        "Should find at least 1 path with max 2 hops"
    );
}

#[test]
fn test_variable_length_exact() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create 2-hop chain: a -> b -> c (connected nodes)
    {
        let mut txn = db.begin_write();
        use nervusdb_query::WriteableGraph;
        let rel_id = txn.get_or_create_rel_type_id("1").unwrap();
        let a = txn.create_node(1, 0).unwrap();
        let b = txn.create_node(2, 0).unwrap();
        let c = txn.create_node(3, 0).unwrap();
        txn.create_edge(a, rel_id, b); // a -> b
        txn.create_edge(b, rel_id, c); // b -> c
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    // a-[*2]->c: exactly 2 hops
    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*2]->(c) RETURN a, c",
        &Params::new(),
    )
    .unwrap();

    assert!(
        !rows.is_empty(),
        "Should find at least 1 path with exactly 2 hops"
    );
}

#[test]
fn test_variable_length_with_limit() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        for _ in 0..4 {
            let q = prepare("CREATE (n)-[:1]->(m)").unwrap();
            q.execute_write(&db.snapshot(), &mut txn, &Params::new())
                .unwrap();
        }
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    let rows = query_collect(
        &snapshot,
        "MATCH (a)-[:1*]->(e) RETURN a, e LIMIT 1",
        &Params::new(),
    )
    .unwrap();

    assert_eq!(rows.len(), 1);
}

#[test]
fn test_variable_length_no_path() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        // Create isolated nodes with no edges of rel type 999
        let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    // Query with non-existent relationship type
    let rows = query_collect(
        &snapshot,
        "MATCH (b)-[:999*]->(a) RETURN b, a",
        &Params::new(),
    )
    .unwrap();

    assert_eq!(rows.len(), 0, "No path with rel type 999 should exist");
}
