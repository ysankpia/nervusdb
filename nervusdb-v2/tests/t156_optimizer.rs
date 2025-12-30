use nervusdb_v2::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn t156_optimizer_index_seek_works() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // 1. Create test data
    {
        let mut tx = db.begin_write();
        let l_person = tx.get_or_create_label("Person").unwrap();
        let alice = tx.create_node(10, l_person).unwrap();
        let bob = tx.create_node(20, l_person).unwrap();

        tx.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
        tx.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )
        .unwrap();

        tx.commit().unwrap();
    }

    // 2. Create index and compact
    db.create_index("Person", "name").unwrap();
    db.compact().unwrap();

    // 3. Test EXPLAIN - should show IndexSeek
    {
        use nervusdb_v2_query::{executor::Value, prepare};
        let snapshot = db.snapshot();
        let query = prepare("EXPLAIN MATCH (n:Person) WHERE n.name = 'Alice' RETURN n").unwrap();
        let results: Result<Vec<_>, _> = query
            .execute_streaming(&snapshot, &Default::default())
            .collect();
        let rows = results.unwrap();

        assert!(!rows.is_empty(), "EXPLAIN should return results");
        if let Some(Value::String(plan)) = rows[0].get("plan") {
            assert!(
                plan.contains("IndexSeek"),
                "Expected IndexSeek in plan, got: {}",
                plan
            );
        } else {
            panic!("Expected plan string output");
        }
    }

    // 4. Test query execution works correctly
    {
        use nervusdb_v2_query::prepare;
        let snapshot = db.snapshot();
        let query = prepare("MATCH (n:Person) WHERE n.name = 'Alice' RETURN n").unwrap();
        let results: Result<Vec<_>, _> = query
            .execute_streaming(&snapshot, &Default::default())
            .collect();
        let rows = results.unwrap();

        assert_eq!(rows.len(), 1, "Should find exactly 1 person named Alice");
    }
}

#[test]
fn t156_optimizer_statistics_collection() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // 1. Create test nodes
    let l_a;
    let l_b;
    {
        let mut tx = db.begin_write();
        l_a = tx.get_or_create_label("LabelA").unwrap();
        l_b = tx.get_or_create_label("LabelB").unwrap();

        for i in 0..10 {
            let n = tx.create_node(100 + i, l_a).unwrap();
            tx.set_node_property(n, "id".to_string(), PropertyValue::Int(i as i64))
                .unwrap();
        }
        for i in 0..5 {
            let n = tx.create_node(200 + i, l_b).unwrap();
            tx.set_node_property(n, "id".to_string(), PropertyValue::Int(i as i64))
                .unwrap();
        }

        tx.commit().unwrap();
    }

    // 2. Compact to collect stats
    db.compact().unwrap();

    // 3. Verify statistics via GraphSnapshot API
    {
        let snapshot = db.snapshot();
        let lid_a = snapshot
            .resolve_label_id("LabelA")
            .expect("LabelA should exist");
        let lid_b = snapshot
            .resolve_label_id("LabelB")
            .expect("LabelB should exist");

        let count_a = snapshot.node_count(Some(lid_a));
        let count_b = snapshot.node_count(Some(lid_b));
        let count_total = snapshot.node_count(None);

        assert_eq!(count_a, 10, "Expected 10 nodes with LabelA");
        assert_eq!(count_b, 5, "Expected 5 nodes with LabelB");
        assert_eq!(count_total, 15, "Expected 15 total nodes");
    }
}
