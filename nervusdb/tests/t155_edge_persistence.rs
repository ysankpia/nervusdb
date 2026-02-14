use nervusdb::{Db, GraphSnapshot, PropertyValue};
use nervusdb_api::EdgeKey;
use tempfile::tempdir;

#[test]
fn test_edge_property_persistence() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let node_a;
    let node_b;
    let edge_key;

    // 1. Create nodes and edge with properties
    {
        let mut tx = db.begin_write();
        let l_person = tx.get_or_create_label("Person").unwrap();
        node_a = tx.create_node(1, l_person).unwrap();
        node_b = tx.create_node(2, l_person).unwrap();

        let rel_knows = tx.get_or_create_label("KNOWS").unwrap();
        tx.create_edge(node_a, rel_knows, node_b);

        edge_key = EdgeKey {
            src: node_a,
            rel: rel_knows,
            dst: node_b,
        };

        tx.set_edge_property(
            node_a,
            rel_knows,
            node_b,
            "since".to_string(),
            PropertyValue::Int(2023),
        )
        .unwrap();
        tx.set_edge_property(
            node_a,
            rel_knows,
            node_b,
            "strength".to_string(),
            PropertyValue::Float(0.9),
        )
        .unwrap();
        tx.commit().unwrap();
    }

    // 2. Verify in L0Run
    {
        let snap = db.snapshot();
        let p_since = snap.edge_property(edge_key, "since").unwrap();
        assert_eq!(p_since, PropertyValue::Int(2023));

        let props = snap.edge_properties(edge_key).unwrap();
        assert_eq!(props.get("since").unwrap(), &PropertyValue::Int(2023));
        assert_eq!(props.get("strength").unwrap(), &PropertyValue::Float(0.9));
    }

    // 3. Compact (Sink to B-Tree)
    db.compact().unwrap();

    // 4. Verify after compaction (from B-Tree)
    {
        let snap = db.snapshot();
        let p_since = snap
            .edge_property(edge_key, "since")
            .expect("Edge property 'since' should be found after compaction");
        assert_eq!(p_since, PropertyValue::Int(2023));

        let props = snap
            .edge_properties(edge_key)
            .expect("Edge properties should be found after compaction");
        assert_eq!(props.get("since").unwrap(), &PropertyValue::Int(2023));
        assert_eq!(props.get("strength").unwrap(), &PropertyValue::Float(0.9));
    }

    // 5. Restart and verify
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    {
        let snap = db.snapshot();
        let p_since = snap.edge_property(edge_key, "since").unwrap();
        assert_eq!(p_since, PropertyValue::Int(2023));
    }
}
