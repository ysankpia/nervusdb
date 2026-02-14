use nervusdb::Db;
use nervusdb::query::{Params, prepare};
use tempfile::tempdir;

#[test]
fn test_incoming_match() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test_incoming.ndb")).unwrap();

    // 1. Setup: A -> B, C -> B, A -> C
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 101).unwrap();
        let b = txn.create_node(2, 102).unwrap();
        let c = txn.create_node(3, 103).unwrap();

        txn.set_node_property(a, "name".to_string(), "A".into())
            .unwrap();
        txn.set_node_property(b, "name".to_string(), "B".into())
            .unwrap();
        txn.set_node_property(c, "name".to_string(), "C".into())
            .unwrap();

        let knows = txn.get_or_create_rel_type("KNOWS").unwrap();
        txn.create_edge(a, knows, b);
        txn.create_edge(c, knows, b);
        txn.create_edge(a, knows, c);
        txn.commit().unwrap();
    }

    // 2. Query: MATCH (n {name: 'B'})<-[:KNOWS]-(m) RETURN m.name
    let snapshot = db.snapshot();
    let q = "MATCH (n {name: 'B'})<-[:KNOWS]-(m) RETURN m.name ORDER BY m.name";
    let prepared = prepare(q).unwrap();
    let params = Params::new();
    let results = prepared.execute_streaming(&snapshot, &params);

    let mut rows: Vec<_> = results.collect::<Result<Vec<_>, _>>().unwrap();
    rows.sort_by_key(|r| r.get("m.name").unwrap().as_string().unwrap().to_string());

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("m.name").unwrap().as_string().unwrap(), "A");
    assert_eq!(rows[1].get("m.name").unwrap().as_string().unwrap(), "C");
}

#[test]
fn test_undirected_match() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test_undirected.ndb")).unwrap();

    // Setup: A -> B
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 101).unwrap();
        let b = txn.create_node(2, 102).unwrap();

        txn.set_node_property(a, "name".to_string(), "A".into())
            .unwrap();
        txn.set_node_property(b, "name".to_string(), "B".into())
            .unwrap();

        let knows = txn.get_or_create_rel_type("KNOWS").unwrap();
        txn.create_edge(a, knows, b);
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let params = Params::new();

    // MATCH (n)-[:KNOWS]-(m) WHERE n.name = 'A' -> should find B (outgoing)
    let q1 = "MATCH (n {name: 'A'})-[:KNOWS]-(m) RETURN m.name";
    let p1 = prepare(q1).unwrap();
    let mut res1 = p1.execute_streaming(&snapshot, &params);
    assert_eq!(
        res1.next()
            .unwrap()
            .unwrap()
            .get("m.name")
            .unwrap()
            .as_string()
            .unwrap(),
        "B"
    );
    assert!(res1.next().is_none());

    // MATCH (n)-[:KNOWS]-(m) WHERE n.name = 'B' -> should find A (incoming)
    let q2 = "MATCH (n {name: 'B'})-[:KNOWS]-(m) RETURN m.name";
    let p2 = prepare(q2).unwrap();
    let mut res2 = p2.execute_streaming(&snapshot, &params);
    assert_eq!(
        res2.next()
            .unwrap()
            .unwrap()
            .get("m.name")
            .unwrap()
            .as_string()
            .unwrap(),
        "A"
    );
    assert!(res2.next().is_none());
}

#[test]
fn test_incoming_after_compaction() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test_compact.ndb")).unwrap();

    // 1. write to L0
    {
        let mut txn = db.begin_write();
        let a = txn.create_node(1, 1).unwrap();
        let b = txn.create_node(2, 2).unwrap();
        txn.set_node_property(a, "name".to_string(), "A".into())
            .unwrap();
        txn.set_node_property(b, "name".to_string(), "B".into())
            .unwrap();
        let knows = txn.get_or_create_rel_type("KNOWS").unwrap();
        txn.create_edge(a, knows, b);
        txn.commit().unwrap();
    }

    db.compact().unwrap();

    let snapshot = db.snapshot();
    let q = "MATCH (n {name: 'B'})<-[:KNOWS]-(m) RETURN m.name";
    let p = prepare(q).unwrap();
    let params = Params::new();
    let mut res = p.execute_streaming(&snapshot, &params);
    assert_eq!(
        res.next()
            .unwrap()
            .unwrap()
            .get("m.name")
            .unwrap()
            .as_string()
            .unwrap(),
        "A"
    );
}
