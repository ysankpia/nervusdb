use nervusdb::{Db, GraphSnapshot, PropertyValue};

#[test]
fn test_index_integration_e2e() -> nervusdb::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("t107.ndb");
    let db = Db::open(&db_path)?;

    // 1. Create Index for Person.name
    db.create_index("Person", "name")?;

    // 2. Insert Data
    {
        let mut txn = db.begin_write();
        // Create 100 users
        // Use manual calls because CREATE parser might be limited, but CREATE parser supports properties now.
        // Let's use the executor wrapper (via query) or manual txn API.
        // Manual API is safer for test setup.

        // Fix: Use get_or_create_label
        let label = txn.get_or_create_label("Person")?;

        for i in 0..100 {
            let ext_id = (1000 + i) as u64; // External Id
            let iid = txn.create_node(ext_id, label)?;

            let name = format!("User{}", i);
            txn.set_node_property(iid, "name".to_string(), PropertyValue::String(name))?;
            txn.set_node_property(iid, "age".to_string(), PropertyValue::Int(i as i64))?;
        }
        txn.commit()?;
    }

    // 3. Verify Index Usage with EXPLAIN
    let snapshot = db.snapshot();
    let query_str = "MATCH (n:Person) WHERE n.name = 'User42' RETURN n";
    let prepared = nervusdb::query::prepare(&format!("EXPLAIN {}", query_str))
        .expect("Failed to prepare explain");

    // We can inspect the explain output if exposed, or just run execute_streaming()
    // PreparedQuery has `explain: Option<String>`.
    // But `prepare()` wrapper in `query` crate returns `PreparedQuery`.
    // We need to inspect `prepare` result.
    // `nervusdb::query` re-exports `prepare`.

    // Check explanation
    if let Some(plan) = prepared.explain_string() {
        // Need to check if explain_string exists or accessible
        println!("Plan:\n{}", plan);
        assert!(plan.contains("IndexSeek"), "Plan should use IndexSeek");
        assert!(
            plan.contains("label=Person"),
            "Plan should explicitly mention label"
        );
    } else {
        // Just inspect the plan structure via Debug? PreparedQuery field `plan` is private?
        // Wait, I implemented `prepare` to return `PreparedQuery` struct.
        // `explain` field is private.
        // `PreparedQuery` has `execute_streaming`.
        // If I run EXPLAIN query, it returns 1 row with plan string.
        let explain_rows = nervusdb::query::query_collect(
            &snapshot,
            &format!("EXPLAIN {}", query_str),
            &Default::default(),
        )?;
        let plan_text = match &explain_rows[0].columns()[0].1 {
            nervusdb_query::Value::String(s) => s,
            _ => panic!("Expected string plan"),
        };
        println!("Explain Output:\n{}", plan_text);
        assert!(plan_text.contains("IndexSeek"), "Plan should use IndexSeek");
    }

    // 4. Verify Execution Correctness
    let rows = nervusdb::query::query_collect(&snapshot, query_str, &Default::default())?;

    assert_eq!(rows.len(), 1);
    // Removed invalid row call
    // Row::cols is private? No, `columns()` is public.
    // Row structure: `cols: Vec<(String, Value)>`.
    // But getting property requires fetching from node?
    // RETURN n returns NodeId.
    // Query returned `n`.
    let node_id = rows[0].get_node("n").expect("Should have node n");
    let name_prop = snapshot
        .node_property(node_id, "name")
        .expect("Should have name");

    match name_prop {
        PropertyValue::String(s) => assert_eq!(s, "User42"),
        _ => panic!("Wrong type"),
    }

    Ok(())
}

#[test]
fn test_index_seek_invalid_value_expression_raises_runtime_type_error() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("t107_runtime_guard.ndb");
    let db = Db::open(&db_path).unwrap();
    db.create_index("Person", "name").unwrap();

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        let n = txn.create_node(1, person).unwrap();
        txn.set_node_property(
            n,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    let query_text = "MATCH (n:Person) WHERE n.name = toBoolean(1) RETURN n";
    let query = nervusdb::query::prepare(query_text).unwrap();
    let err = query
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()
        .expect_err("invalid index-seek value expression should raise runtime TypeError")
        .to_string();

    assert!(
        err.contains("InvalidArgumentValue"),
        "expected InvalidArgumentValue, got: {err}"
    );
}
