use nervusdb::{Db, GraphSnapshot};
use nervusdb_query::{Value, prepare};
use tempfile::tempdir;

#[test]
fn test_create_single_node() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (n)").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn test_create_node_with_properties() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (n {name: 'Alice', age: 30})").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn test_create_relationship() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 3);
}

#[test]
fn test_create_relationship_with_properties() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (a {name: 'A'})-[:1 {weight: 2.5}]->(b {name: 'B'})").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 3);
}

#[test]
fn test_create_multiple_nodes() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // M3: Create nodes one at a time (no comma-separated list)
    let query = prepare("CREATE (a)").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 1);

    // Create second node
    let query = prepare("CREATE (b)").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_create_complex_pattern() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (a {x: 1})-[:1]->(b {y: 2})").unwrap();
    let mut txn = db.begin_write();
    let count = query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 3);
}

#[test]
fn test_delete_basic() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    let count = create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 3);

    // Now delete
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(b) DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(deleted, 1);
}

#[test]
fn test_delete_second_node() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Delete the second node
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(b) DELETE b").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(deleted, 1);
}

#[test]
fn test_detach_delete() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create first
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // DETACH DELETE
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(b) DETACH DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Should delete edge + node = 2
    assert_eq!(deleted, 2);
}

#[test]
fn test_detach_delete_standalone() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create a pattern: a -> b
    let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // DETACH DELETE with MATCH
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(b) DETACH DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // a has 1 edge = 2 deletions (edge + node)
    assert_eq!(deleted, 2);
}

#[test]
fn test_delete_multiple_nodes() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    // Create two disconnected nodes
    let create_query = prepare("CREATE (a)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    let create_query = prepare("CREATE (b)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Delete first node by matching with a self-loop (create one first)
    let create_query = prepare("CREATE (a)-[:1]->(a)").unwrap();
    let mut txn = db.begin_write();
    create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    // Delete node with self-loop
    let snapshot = db.snapshot();
    let delete_query = prepare("MATCH (a)-[:1]->(a) DELETE a").unwrap();
    let mut txn = db.begin_write();
    let deleted = delete_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();
    assert_eq!(deleted, 1);
}

#[test]
fn test_execute_mixed_create_returns_rows() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("CREATE (n {p: 'foo'}) RETURN n.p AS p").unwrap();
    let mut txn = db.begin_write();
    let (rows, count) = query
        .execute_mixed(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("p"), Some(&Value::String("foo".to_string())));
}

#[test]
fn test_execute_mixed_create_with_unwind_skip_limit() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare(
        "UNWIND [42, 42, 42, 42, 42] AS x CREATE (n:N {num: x}) RETURN n.num AS num SKIP 2 LIMIT 2",
    )
    .unwrap();
    let mut txn = db.begin_write();
    let (rows, count) = query
        .execute_mixed(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 5);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("num"), Some(&Value::Int(42)));
    assert_eq!(rows[1].get("num"), Some(&Value::Int(42)));
}

#[test]
fn test_execute_mixed_create_relationship_with_unwind_skip_limit() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare(
        "UNWIND [42, 42, 42, 42, 42] AS x CREATE ()-[r:R {num: x}]->() RETURN r.num AS num SKIP 2 LIMIT 2",
    )
    .unwrap();
    let mut txn = db.begin_write();
    let (rows, count) = query
        .execute_mixed(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 15);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("num"), Some(&Value::Int(42)));
    assert_eq!(rows[1].get("num"), Some(&Value::Int(42)));
}

#[test]
fn test_delete_edge_variable() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    // Create a pattern: a -> b
    {
        let snapshot = db.snapshot();
        let create_query = prepare("CREATE (a)-[:1]->(b)").unwrap();
        let mut txn = db.begin_write();
        create_query
            .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    // Delete edge by binding it to a variable.
    {
        let snapshot = db.snapshot();
        let delete_query = prepare("MATCH (a)-[r:1]->(b) DELETE r").unwrap();
        let mut txn = db.begin_write();
        let deleted = delete_query
            .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
            .unwrap();
        txn.commit().unwrap();
        assert_eq!(deleted, 1);
    }

    // Verify there are no edges left.
    let snap = db.snapshot();
    let nodes: Vec<_> = snap.nodes().collect();
    let mut total_edges = 0usize;
    for &n in &nodes {
        total_edges += snap.neighbors(n, None).count();
    }
    assert_eq!(total_edges, 0);
}

#[test]
fn test_create_reuses_bound_variables_across_create_clauses() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    let snapshot = db.snapshot();
    let create_query = prepare(
        "CREATE (a:A {name: 'n0'}), (b:B {name: 'n1'})
         CREATE (a)-[:LIKES]->(b)",
    )
    .unwrap();

    let mut txn = db.begin_write();
    let created = create_query
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(created, 3);

    let snapshot = db.snapshot();
    let read_query =
        prepare("MATCH (a:A)-[:LIKES]->(b) RETURN a.name AS a_name, b.name AS b_name").unwrap();
    let rows: Vec<_> = read_query
        .execute_streaming(&snapshot, &nervusdb_query::Params::new())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("a_name"),
        Some(&Value::String("n0".to_string()))
    );
    assert_eq!(
        rows[0].get("b_name"),
        Some(&Value::String("n1".to_string()))
    );
}

#[test]
fn test_delete_optional_null_node_keeps_row() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("OPTIONAL MATCH (a:DoesNotExist) DELETE a RETURN a").unwrap();
    let mut txn = db.begin_write();
    let (rows, count) = query
        .execute_mixed(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("a"), Some(&Value::Null));
}

#[test]
fn test_delete_optional_null_relationship_keeps_row() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let query = prepare("OPTIONAL MATCH ()-[r:DoesNotExist]-() DELETE r RETURN r").unwrap();
    let mut txn = db.begin_write();
    let (rows, count) = query
        .execute_mixed(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .unwrap();
    txn.commit().unwrap();

    assert_eq!(count, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("r"), Some(&Value::Null));
}

#[test]
fn test_delete_label_predicate_rejected_at_compile_time() {
    let err = prepare("MATCH (n) DELETE n:Person")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("syntax error: InvalidDelete"),
        "unexpected compile error: {err}"
    );
}

#[test]
fn test_delete_undefined_variable_rejected_at_compile_time() {
    let err = prepare("MATCH (a) DELETE x").unwrap_err().to_string();
    assert!(
        err.contains("syntax error: UndefinedVariable (x)"),
        "unexpected compile error: {err}"
    );
}

#[test]
fn test_delete_scalar_expression_rejected_at_compile_time() {
    let err = prepare("MATCH () DELETE 1 + 1").unwrap_err().to_string();
    assert!(
        err.contains("syntax error: InvalidArgumentType"),
        "unexpected compile error: {err}"
    );
}

#[test]
fn test_delete_list_index_with_invalid_index_type_raises_runtime_type_error() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();

    {
        let mut txn = db.begin_write();
        prepare("CREATE (u:User)-[:FRIEND]->()")
            .unwrap()
            .execute_write(&db.snapshot(), &mut txn, &nervusdb_query::Params::new())
            .unwrap();
        txn.commit().unwrap();
    }

    let q = prepare(
        "MATCH (:User)-[:FRIEND]->(n) \
         WITH collect(n) AS friends, true AS idx \
         DETACH DELETE friends[idx]",
    )
    .unwrap();

    let mut txn = db.begin_write();
    let err = q
        .execute_write(&db.snapshot(), &mut txn, &nervusdb_query::Params::new())
        .expect_err("invalid list index type in DELETE should raise runtime TypeError")
        .to_string();

    assert!(
        err.contains("InvalidArgumentType"),
        "expected InvalidArgumentType, got: {err}"
    );
}

#[test]
fn test_create_property_with_invalid_toboolean_argument_raises_runtime_type_error() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let snapshot = db.snapshot();

    let q = prepare("CREATE (:N {flag: toBoolean(1)})").unwrap();
    let mut txn = db.begin_write();
    let err = q
        .execute_write(&snapshot, &mut txn, &nervusdb_query::Params::new())
        .expect_err("invalid toBoolean argument in CREATE property should raise runtime TypeError")
        .to_string();

    assert!(
        err.contains("InvalidArgumentValue"),
        "expected InvalidArgumentValue, got: {err}"
    );
}
