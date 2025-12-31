use nervusdb_v2::query::{Params, WriteableGraph};
use nervusdb_v2::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_remove_node_property() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t304_node.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;
        let alice = txn.create_node(1, person)?;
        txn.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )?;
        txn.set_node_property(alice, "temp_flag".to_string(), PropertyValue::Bool(true))?;
        txn.commit()?;
    }

    // REMOVE via Cypher.
    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q = "MATCH (n:Person) WHERE n.name = 'Alice' REMOVE n.temp_flag";
        let prep = nervusdb_v2::query::prepare(q)?;
        let n = prep.execute_write(&snapshot, &mut txn, &Params::default())?;
        assert_eq!(n, 1);
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let alice = snapshot
        .nodes()
        .find(|&iid| snapshot.resolve_external(iid) == Some(1))
        .expect("node 1 should exist");
    assert_eq!(snapshot.node_property(alice, "temp_flag"), None);

    Ok(())
}

#[test]
fn test_remove_edge_property() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t304_edge.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;
        let rel = txn.get_or_create_rel_type_id("1")?;

        let a = txn.create_node(1, person)?;
        let b = txn.create_node(2, person)?;
        txn.create_edge(a, rel, b);
        txn.set_edge_property(a, rel, b, "since".to_string(), PropertyValue::Int(2024))?;

        txn.commit()?;
    }

    // REMOVE edge property.
    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q = "MATCH (a)-[r:1]->(b) REMOVE r.since";
        let prep = nervusdb_v2::query::prepare(q)?;
        let n = prep.execute_write(&snapshot, &mut txn, &Params::default())?;
        assert_eq!(n, 1);
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let a = snapshot
        .nodes()
        .find(|&iid| snapshot.resolve_external(iid) == Some(1))
        .expect("node 1 should exist");
    let rel = snapshot
        .resolve_rel_type_id("1")
        .expect("rel type should exist");
    let edge = snapshot
        .neighbors(a, Some(rel))
        .next()
        .expect("edge should exist");
    assert_eq!(snapshot.edge_property(edge, "since"), None);

    Ok(())
}
