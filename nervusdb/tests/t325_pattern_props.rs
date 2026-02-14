use nervusdb::{Db, PropertyValue};
use nervusdb_query::{Params, Result, prepare};
use tempfile::tempdir;

#[test]
fn test_pattern_properties_in_hops() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Setup data using manual API
    {
        let mut txn = db.begin_write();
        let label_none = txn.get_or_create_label("None").unwrap();
        // create_node(external_id, label_id)
        let a = txn.create_node(1, label_none).unwrap();
        let b = txn.create_node(2, label_none).unwrap();
        let c = txn.create_node(3, label_none).unwrap();

        txn.set_node_property(b, "name".into(), PropertyValue::String("Alice".into()))
            .unwrap();
        txn.set_node_property(c, "name".into(), PropertyValue::String("Bob".into()))
            .unwrap();

        let rel_1 = txn.get_or_create_rel_type("1").unwrap();
        txn.create_edge(a, rel_1, b);
        txn.create_edge(a, rel_1, c);

        let rel_2 = txn.get_or_create_rel_type("2").unwrap();
        txn.create_edge(a, rel_2, b);
        txn.set_edge_property(
            a,
            rel_2,
            b,
            "type".into(),
            PropertyValue::String("friend".into()),
        )
        .unwrap();
        txn.create_edge(a, rel_2, c);
        txn.set_edge_property(
            a,
            rel_2,
            c,
            "type".into(),
            PropertyValue::String("enemy".into()),
        )
        .unwrap();

        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();

    // Query 1: Start node property (should work)
    let q1 = prepare("MATCH (n:None {name: 'Alice'}) RETURN n").unwrap();
    let res1: Vec<_> = q1
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(res1.len(), 1, "Query 1 failed");

    // Query 2: Hop node property (Target of T325)
    let q2 = prepare("MATCH (a:None)-[:1]->(b:None {name: 'Alice'}) RETURN b").unwrap();
    let res2: Vec<_> = q2
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();

    println!("Query 2 results: {:?}", res2);
    // This is expected to fail (or return 2 rows) if the properties are ignored
    assert_eq!(
        res2.len(),
        1,
        "Query 2 (hop property) failed, returned {} rows",
        res2.len()
    );

    // Query 3: Relationship property (Target of T325)
    let q3 = prepare("MATCH (a:None)-[r:2 {type: 'friend'}]->(b:None) RETURN b").unwrap();
    let res3: Vec<_> = q3
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        res3.len(),
        1,
        "Query 3 (rel property) failed, returned {} rows",
        res3.len()
    );
}
