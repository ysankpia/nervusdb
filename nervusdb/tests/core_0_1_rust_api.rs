use nervusdb::{Db, EdgeKey, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn core_0_1_rust_facade_write_snapshot_and_reopen() {
    let dir = tempdir().unwrap();
    let base = dir.path().join("graph");

    let (alice, bob, knows) = {
        let db = Db::open(&base).unwrap();
        assert_eq!(db.storage_dir(), base.as_path());

        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        let knows = txn.get_or_create_rel_type("KNOWS").unwrap();
        let alice = txn.create_node(1, person).unwrap();
        let bob = txn.create_node(2, person).unwrap();

        txn.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
        txn.set_node_property(alice, "age".to_string(), PropertyValue::Int(30))
            .unwrap();
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )
        .unwrap();
        txn.create_edge(alice, knows, bob).unwrap();
        txn.set_edge_property(
            alice,
            knows,
            bob,
            "since".to_string(),
            PropertyValue::Int(2024),
        )
        .unwrap();
        txn.commit().unwrap();

        let snapshot = db.snapshot();
        assert_eq!(snapshot.resolve_label_id("Person"), Some(person));
        assert_eq!(
            snapshot.resolve_label_name(person).as_deref(),
            Some("Person")
        );
        assert_eq!(snapshot.resolve_rel_type_id("KNOWS"), Some(knows));
        assert_eq!(
            snapshot.resolve_rel_type_name(knows).as_deref(),
            Some("KNOWS")
        );
        assert_eq!(
            snapshot.node_property(alice, "name"),
            Some(PropertyValue::String("Alice".to_string()))
        );
        assert_eq!(
            snapshot.node_property(alice, "age"),
            Some(PropertyValue::Int(30))
        );
        let person_nodes: Vec<_> = snapshot.nodes_with_label(person).collect();
        assert_eq!(person_nodes, vec![alice, bob]);

        let outgoing: Vec<EdgeKey> = snapshot.neighbors(alice, Some(knows)).collect();
        assert_eq!(
            outgoing,
            vec![EdgeKey {
                src: alice,
                rel: knows,
                dst: bob
            }]
        );
        assert!(snapshot.neighbors(bob, Some(knows)).next().is_none());
        assert_eq!(
            snapshot.edge_property(outgoing[0], "since"),
            Some(PropertyValue::Int(2024))
        );

        let read_txn = db.begin_read();
        let read_outgoing: Vec<EdgeKey> = read_txn.neighbors(alice, Some(knows)).collect();
        assert_eq!(read_outgoing, outgoing);

        (alice, bob, knows)
    };

    let reopened = Db::open(&base).unwrap();
    let snapshot = reopened.snapshot();
    assert_eq!(
        snapshot.node_property(alice, "name"),
        Some(PropertyValue::String("Alice".to_string()))
    );
    assert_eq!(
        snapshot.node_property(bob, "name"),
        Some(PropertyValue::String("Bob".to_string()))
    );
    let outgoing: Vec<EdgeKey> = snapshot.neighbors(alice, Some(knows)).collect();
    assert_eq!(
        outgoing,
        vec![EdgeKey {
            src: alice,
            rel: knows,
            dst: bob
        }]
    );
    assert!(snapshot.neighbors(bob, Some(knows)).next().is_none());
    assert_eq!(
        snapshot.edge_property(outgoing[0], "since"),
        Some(PropertyValue::Int(2024))
    );
}
