//! Core 0.1 realistic usage examples — documented and runnable.
//!
//! Each example demonstrates a practical graph scenario using the
//! Rust API or Mini-Cypher. Run with:
//!
//! ```bash
//! cargo test --test core_0_1_examples
//! ```

use nervusdb::query::{Params, Result as QueryResult, Value, prepare, query_collect};
use nervusdb::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

// ─── Example 1: Open a database and create a simple graph ────────────────

#[test]
fn example_1_open_create_and_query() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create two Person nodes and a KNOWS edge
    {
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
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )
        .unwrap();
        txn.create_edge(alice, knows, bob).unwrap();

        txn.commit().unwrap();
    }

    // Query with Mini-Cypher
    let rows = query_collect(
        &db.snapshot(),
        "MATCH (a:Person)-[:KNOWS]->(b) WHERE a.name = 'Alice' RETURN b.name",
        &Params::new(),
    )?;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::String("Bob".to_string()));
    Ok(())
}

// ─── Example 2: Multi-statement write transaction ────────────────────────

#[test]
fn example_2_multi_statement_transaction() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Two CREATE statements in one atomic transaction
    let snapshot = db.snapshot();
    let mut txn = db.begin_write();

    prepare("CREATE (a:Person {name: 'Alice'})")?.execute_write(
        &snapshot,
        &mut txn,
        &Params::new(),
    )?;
    prepare("CREATE (b:Person {name: 'Bob'})")?.execute_write(
        &snapshot,
        &mut txn,
        &Params::new(),
    )?;

    txn.commit().unwrap();

    let mut names: Vec<_> = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) RETURN n.name",
        &Params::new(),
    )?
    .into_iter()
    .map(|row| row.columns()[0].1.clone())
    .collect();
    names.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(
        names,
        vec![
            Value::String("Alice".to_string()),
            Value::String("Bob".to_string())
        ]
    );
    Ok(())
}

// ─── Example 3: Filtering with WHERE ─────────────────────────────────────

#[test]
fn example_3_filter_with_where() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        let a = txn.create_node(1, person).unwrap();
        txn.set_node_property(a, "age".to_string(), PropertyValue::Int(25))
            .unwrap();
        let b = txn.create_node(2, person).unwrap();
        txn.set_node_property(b, "age".to_string(), PropertyValue::Int(35))
            .unwrap();
        let c = txn.create_node(3, person).unwrap();
        txn.set_node_property(c, "age".to_string(), PropertyValue::Int(45))
            .unwrap();
        txn.commit().unwrap();
    }

    let mut ages: Vec<_> = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.age > 30 RETURN n.age",
        &Params::new(),
    )?
    .into_iter()
    .map(|row| row.columns()[0].1.clone())
    .collect();

    ages.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(ages, vec![Value::Int(35), Value::Int(45)]);
    Ok(())
}

// ─── Example 4: LIMIT ───────────────────────────────────────────────────

#[test]
fn example_4_limit_result_count() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        for i in 1..=5u64 {
            let n = txn.create_node(i, person).unwrap();
            txn.set_node_property(n, "score".to_string(), PropertyValue::Int(i as i64 * 10))
                .unwrap();
        }
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) RETURN n.score LIMIT 2",
        &Params::new(),
    )?;

    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| matches!(row.columns()[0].1, Value::Int(10 | 20 | 30 | 40 | 50)))
    );
    Ok(())
}

// ─── Example 5: Count with Rust snapshot API ────────────────────────────

#[test]
fn example_5_count_with_snapshot_api() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let product;
    {
        let mut txn = db.begin_write();
        product = txn.get_or_create_label("Product").unwrap();
        let a = txn.create_node(1, product).unwrap();
        txn.set_node_property(a, "price".to_string(), PropertyValue::Int(100))
            .unwrap();
        let b = txn.create_node(2, product).unwrap();
        txn.set_node_property(b, "price".to_string(), PropertyValue::Int(200))
            .unwrap();
        let c = txn.create_node(3, product).unwrap();
        txn.set_node_property(c, "price".to_string(), PropertyValue::Int(300))
            .unwrap();
        txn.commit().unwrap();
    }

    let snapshot = db.snapshot();
    assert_eq!(snapshot.node_count(Some(product)), 3);
    let total: i64 = snapshot
        .nodes_with_label(product)
        .filter_map(|node| snapshot.node_property(node, "price"))
        .filter_map(|value| match value {
            PropertyValue::Int(value) => Some(value),
            _ => None,
        })
        .sum();
    assert_eq!(total, 600);
}

// ─── Example 6: One-hop relationship traversal ─────────────────────────

#[test]
fn example_6_one_hop_relationship_traversal() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
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
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )
        .unwrap();
        txn.create_edge(alice, knows, bob).unwrap();
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (a:Person)-[:KNOWS]->(b) WHERE a.name = 'Alice' RETURN b.name",
        &Params::new(),
    )?;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::String("Bob".to_string()));
    Ok(())
}

// ─── Example 7: Label scan plus property filter ────────────────────────

#[test]
fn example_7_label_scan_and_property_filter() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let user = txn.get_or_create_label("User").unwrap();
        let a = txn.create_node(1, user).unwrap();
        txn.set_node_property(
            a,
            "email".to_string(),
            PropertyValue::String("a@example.com".to_string()),
        )
        .unwrap();
        let b = txn.create_node(2, user).unwrap();
        txn.set_node_property(
            b,
            "email".to_string(),
            PropertyValue::String("b@example.com".to_string()),
        )
        .unwrap();
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (u:User) WHERE u.email = 'a@example.com' RETURN u",
        &Params::new(),
    )?;
    assert_eq!(rows.len(), 1);
    Ok(())
}

// ─── Example 8: Reopen and verify durability ────────────────────────────

#[test]
fn example_8_reopen_durability() {
    let dir = tempdir().unwrap();

    {
        let db = Db::open(dir.path()).unwrap();
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        let alice = txn.create_node(1, person).unwrap();
        txn.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )
        .unwrap();
        txn.commit().unwrap();
    }

    // Reopen the same database
    let db = Db::open(dir.path()).unwrap();
    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.name = 'Alice' RETURN n",
        &Params::new(),
    )
    .unwrap();

    assert_eq!(rows.len(), 1);
}

// ─── Example 9: Delete with DELETE ─────────────────────────────────────

#[test]
fn example_9_delete_operations() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        prepare("CREATE (n:Person {name: 'Temp'})")?.execute_write(
            &snapshot,
            &mut txn,
            &Params::new(),
        )?;
        txn.commit().unwrap();
    }

    // Verify it exists
    let before = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.name = 'Temp' RETURN n",
        &Params::new(),
    )?;
    assert_eq!(before.len(), 1);

    // Delete it
    {
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        prepare("MATCH (n:Person) WHERE n.name = 'Temp' DELETE n")?.execute_write(
            &snapshot,
            &mut txn,
            &Params::new(),
        )?;
        txn.commit().unwrap();
    }

    // Verify gone
    let after = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) WHERE n.name = 'Temp' RETURN n",
        &Params::new(),
    )?;
    assert_eq!(after.len(), 0);
    Ok(())
}

// ─── Example 10: EXPLAIN query plans ────────────────────────────────────

#[test]
fn example_10_explain_query_plan() -> QueryResult<()> {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person").unwrap();
        txn.create_node(1, person).unwrap();
        txn.commit().unwrap();
    }

    let rows = query_collect(
        &db.snapshot(),
        "EXPLAIN MATCH (n:Person) RETURN n LIMIT 5",
        &Params::new(),
    )?;

    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].columns()[0].1, Value::String(_)));
    Ok(())
}
