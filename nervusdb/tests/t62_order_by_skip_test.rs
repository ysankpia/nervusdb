use nervusdb::Db;
use nervusdb_query::{Params, Result, prepare};
use tempfile::tempdir;

#[test]
fn test_order_by_asc() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        let q2 = prepare("CREATE (a)-[:1]->(c)").unwrap();
        q2.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    // M3 only supports RETURN <var>, not property access
    let query = prepare("MATCH (n)-[:1]->(m) RETURN m ORDER BY m").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 2);
}

#[test]
fn test_order_by_desc() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        let q2 = prepare("CREATE (a)-[:1]->(c)").unwrap();
        q2.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let query = prepare("MATCH (n)-[:1]->(m) RETURN m ORDER BY m DESC").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    assert_eq!(rows.len(), 2);
}

#[test]
fn test_skip() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        let q1 = prepare("CREATE (a)-[:1]->(b)").unwrap();
        q1.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        let q2 = prepare("CREATE (a)-[:1]->(c)").unwrap();
        q2.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let query = prepare("MATCH (n)-[:1]->(m) RETURN m SKIP 1").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_limit() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        for _ in 0..10 {
            let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
            q.execute_write(&db.snapshot(), &mut txn, &Params::new())
                .unwrap();
        }
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let query = prepare("MATCH (n)-[:1]->(m) RETURN m LIMIT 5").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 5);
}

#[test]
fn test_distinct() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        // Since we don't have MERGE, creating duplicate edges is slightly tricky with CREATE.
        // Actually, CREATE just creates them.
        let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        q.execute_write(&db.snapshot(), &mut txn, &Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let query = prepare("MATCH (n)-[:1]->(m) RETURN DISTINCT m").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_distinct_real() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        use nervusdb_query::WriteableGraph;
        let n1 = txn.create_node(1, 0).unwrap();
        let n2 = txn.create_node(2, 0).unwrap();
        let rel = txn.get_or_create_rel_type_id("1").unwrap();
        txn.create_edge(n1, rel, n2);
        txn.create_edge(n1, rel, n2); // Duplicate edge
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let query = prepare("MATCH (n)-[:1]->(m) RETURN DISTINCT m").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_order_by_skip_limit_combined() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    {
        let mut txn = db.begin_write();
        for _ in 0..10 {
            let q = prepare("CREATE (a)-[:1]->(b)").unwrap();
            q.execute_write(&db.snapshot(), &mut txn, &Params::new())
                .unwrap();
        }
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    // M3 only supports RETURN <var> ORDER BY <var>
    let query = prepare("MATCH (n)-[:1]->(m) RETURN m ORDER BY m SKIP 2 LIMIT 3").unwrap();
    let rows: Vec<_> = query
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        rows.len(),
        3,
        "Expected 3 rows after SKIP 2 LIMIT 3 on 10 rows"
    );
}
