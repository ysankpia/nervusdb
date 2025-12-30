use nervusdb_v2::query::{Value, WriteableGraph};
use nervusdb_v2::{Db, GraphSnapshot, PropertyValue};
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn test_complex_types_storage() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let mut list = Vec::new();
    list.push(PropertyValue::Int(1));
    list.push(PropertyValue::String("inner".into()));

    let mut map = BTreeMap::new();
    map.insert("key1".into(), PropertyValue::Bool(true));
    map.insert("key2".into(), PropertyValue::List(list.clone()));

    let node_id;
    {
        let mut tx = db.begin_write();
        let label = tx.get_or_create_label("Person").unwrap();
        let n = tx.create_node(100, label).unwrap();
        tx.set_node_property(n, "data_list".to_string(), PropertyValue::List(list))
            .unwrap();
        tx.set_node_property(n, "data_map".to_string(), PropertyValue::Map(map))
            .unwrap();
        tx.commit().unwrap();
        node_id = n;
    }

    // Verify immediate retrieval
    {
        let snap = db.snapshot();
        let p_list = snap.node_property(node_id, "data_list").unwrap();
        if let PropertyValue::List(l) = p_list {
            assert_eq!(l.len(), 2);
            assert_eq!(l[0], PropertyValue::Int(1));
        } else {
            panic!("Expected list");
        }

        let p_map = snap.node_property(node_id, "data_map").unwrap();
        if let PropertyValue::Map(m) = p_map {
            assert_eq!(m.get("key1").unwrap(), &PropertyValue::Bool(true));
        } else {
            panic!("Expected map");
        }
    }

    // Restart and verify
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    {
        let snap = db.snapshot();
        let p_list = snap.node_property(node_id, "data_list").unwrap();
        assert!(matches!(p_list, PropertyValue::List(_)));
    }
}
