// T313: Built-in Functions - TDD Test Suite
// 🔴 RED Phase: All tests should FAIL initially
//
// Functions to implement:
// - size(list/string/map) -> integer
// - coalesce(expr, expr, ...) -> first non-null
// - head(list) -> first element
// - tail(list) -> list without first
// - last(list) -> last element
// - keys(map/node) -> list of keys
// - type(relationship) -> string
// - id(node/relationship) -> integer

use nervusdb_v2::Db;
use nervusdb_v2::query::Value;
use tempfile::tempdir;

// ============================================================================
// size() function tests
// ============================================================================

#[test]
fn test_size_of_list() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN size([1, 2, 3]) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(3));

    Ok(())
}

#[test]
fn test_size_of_string() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN size('hello') AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(5));

    Ok(())
}

#[test]
fn test_size_of_empty_list() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN size([]) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(0));

    Ok(())
}

// ============================================================================
// coalesce() function tests
// ============================================================================

#[test]
fn test_coalesce_first_non_null() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN coalesce(null, null, 42, 99) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(42));

    Ok(())
}

#[test]
fn test_coalesce_all_null() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN coalesce(null, null) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Null);

    Ok(())
}

// ============================================================================
// head() / tail() / last() function tests
// ============================================================================

#[test]
fn test_head_of_list() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN head([1, 2, 3]) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(1));

    Ok(())
}

#[test]
fn test_tail_of_list() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN tail([1, 2, 3]) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    // tail returns list without first element
    assert_eq!(
        results[0].get("result").unwrap(),
        &Value::List(vec![Value::Int(2), Value::Int(3)])
    );

    Ok(())
}

#[test]
fn test_last_of_list() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN last([1, 2, 3]) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(3));

    Ok(())
}

// ============================================================================
// keys() function tests
// ============================================================================

#[test]

fn test_keys_of_node() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let n = txn.create_node(1, label)?;
    txn.set_node_property(
        n,
        "name".to_string(),
        nervusdb_v2::PropertyValue::String("Alice".to_string()),
    )?;
    txn.set_node_property(n, "age".to_string(), nervusdb_v2::PropertyValue::Int(30))?;
    txn.commit()?;

    let query = "MATCH (n:Person) RETURN keys(n) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    // keys should return list of property names
    if let Value::List(keys) = results[0].get("result").unwrap() {
        assert!(keys.contains(&Value::String("name".to_string())));
        assert!(keys.contains(&Value::String("age".to_string())));
    } else {
        panic!("Expected list of keys");
    }

    Ok(())
}

// ============================================================================
// type() function tests
// ============================================================================

#[test]

fn test_type_of_relationship() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let rel_type = txn.get_or_create_rel_type("KNOWS")?;
    // Use unique large external IDs to avoid conflicts
    let n1 = txn.create_node(1001, label)?;
    let n2 = txn.create_node(1002, label)?;
    txn.create_edge(n1, n2, rel_type);
    txn.commit()?;

    let query = "MATCH (a)-[r]->(b) RETURN type(r) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    // Note: evaluator returns type ID (int) currently, until we add schema lookup
    // Adjusting expectation to match current implementation
    if let Value::String(s) = results[0].get("result").unwrap() {
        assert_eq!(s, "KNOWS");
    } else {
        panic!("Expected string type name");
    }

    Ok(())
}

// ============================================================================
// id() function tests
// ============================================================================

#[test]

fn test_id_of_node() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let _n = txn.create_node(2001, label)?;
    txn.commit()?;

    let query = "MATCH (n:Person) RETURN id(n) AS result";
    let prep = nervusdb_v2::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    // id should return an integer
    if let Value::Int(_id) = results[0].get("result").unwrap() {
        // OK - checked it matches the pattern
    } else {
        panic!("Expected integer id");
    }

    Ok(())
}
