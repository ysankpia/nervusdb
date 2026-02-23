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

use nervusdb::Db;
use nervusdb::query::Value;
use tempfile::tempdir;

// ============================================================================
// size() function tests
// ============================================================================

#[test]
fn test_size_of_list() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN size([1, 2, 3]) AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(3));

    Ok(())
}

#[test]
fn test_size_of_string() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN size('hello') AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(5));

    Ok(())
}

#[test]
fn test_size_of_empty_list() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN size([]) AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(0));

    Ok(())
}

#[test]
fn test_left_and_right_string_functions() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN left('hello', 3) AS l, right('hello', 2) AS r";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("l").unwrap(), &Value::String("hel".into()));
    assert_eq!(results[0].get("r").unwrap(), &Value::String("lo".into()));

    Ok(())
}

#[test]
fn test_floor_round_log_and_constants() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN floor(2.7) AS f, round(2.5) AS r, log(1) AS l, e() AS e, pi() AS p";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);

    let f = match results[0].get("f").unwrap() {
        Value::Float(v) => *v,
        other => panic!("expected float floor result, got {other:?}"),
    };
    assert!((f - 2.0).abs() < 1e-9, "floor(2.7) should be 2.0");

    let r = match results[0].get("r").unwrap() {
        Value::Float(v) => *v,
        other => panic!("expected float round result, got {other:?}"),
    };
    assert!((r - 3.0).abs() < 1e-9, "round(2.5) should be 3.0");

    let l = match results[0].get("l").unwrap() {
        Value::Float(v) => *v,
        other => panic!("expected float log result, got {other:?}"),
    };
    assert!(l.abs() < 1e-9, "log(1) should be 0.0");

    let e = match results[0].get("e").unwrap() {
        Value::Float(v) => *v,
        other => panic!("expected float e() result, got {other:?}"),
    };
    assert!((e - std::f64::consts::E).abs() < 1e-9, "e() mismatch");

    let p = match results[0].get("p").unwrap() {
        Value::Float(v) => *v,
        other => panic!("expected float pi() result, got {other:?}"),
    };
    assert!((p - std::f64::consts::PI).abs() < 1e-9, "pi() mismatch");

    Ok(())
}

#[test]
fn test_size_of_path_is_compile_error() {
    let err = nervusdb::query::prepare("MATCH p = (a)-[*]->(b) RETURN size(p)")
        .expect_err("size(path) should be rejected at compile time")
        .to_string();
    assert!(
        err.contains("InvalidArgumentType"),
        "expected InvalidArgumentType, got {err}"
    );
}

// ============================================================================
// coalesce() function tests
// ============================================================================

#[test]
fn test_to_float_from_integer() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN toFloat(3) AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Float(3.0));

    Ok(())
}

#[test]
fn test_to_float_from_numeric_string() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN toFloat('5.25') AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Float(5.25));

    Ok(())
}

#[test]
fn test_to_float_from_invalid_string_returns_null() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN toFloat('foo') AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Null);

    Ok(())
}

#[test]
fn test_to_boolean_from_boolean() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN toBoolean(true) AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Bool(true));

    Ok(())
}

#[test]
fn test_to_boolean_from_valid_string() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN toBoolean('false') AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Bool(false));

    Ok(())
}

#[test]
fn test_to_boolean_from_invalid_string_returns_null() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN toBoolean(' tru ') AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Null);

    Ok(())
}

#[test]
fn test_coalesce_first_non_null() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN coalesce(null, null, 42, 99) AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(42));

    Ok(())
}

#[test]
fn test_coalesce_all_null() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN coalesce(null, null) AS result";
    let prep = nervusdb::query::prepare(query)?;
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
fn test_head_of_list() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN head([1, 2, 3]) AS result";
    let prep = nervusdb::query::prepare(query)?;
    let snapshot = db.snapshot();
    let results: Vec<_> = prep
        .execute_streaming(&snapshot, &Default::default())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Int(1));

    Ok(())
}

#[test]
fn test_tail_of_list() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN tail([1, 2, 3]) AS result";
    let prep = nervusdb::query::prepare(query)?;
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
fn test_last_of_list() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;

    let query = "RETURN last([1, 2, 3]) AS result";
    let prep = nervusdb::query::prepare(query)?;
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

fn test_keys_of_node() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let n = txn.create_node(1, label)?;
    txn.set_node_property(
        n,
        "name".to_string(),
        nervusdb::PropertyValue::String("Alice".to_string()),
    )?;
    txn.set_node_property(n, "age".to_string(), nervusdb::PropertyValue::Int(30))?;
    txn.commit()?;

    let query = "MATCH (n:Person) RETURN keys(n) AS result";
    let prep = nervusdb::query::prepare(query)?;
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

fn test_type_of_relationship() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let rel_type = txn.get_or_create_rel_type("KNOWS")?;
    // Use unique large external IDs to avoid conflicts
    let n1 = txn.create_node(1001, label)?;
    let n2 = txn.create_node(1002, label)?;
    txn.create_edge(n1, rel_type, n2);
    txn.commit()?;

    let query = "MATCH (a)-[r]->(b) RETURN type(r) AS result";
    let prep = nervusdb::query::prepare(query)?;
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

fn test_id_of_node() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t313.ndb");
    let db = Db::open(&db_path)?;
    let mut txn = db.begin_write();

    let label = txn.get_or_create_label("Person")?;
    let _n = txn.create_node(2001, label)?;
    txn.commit()?;

    let query = "MATCH (n:Person) RETURN id(n) AS result";
    let prep = nervusdb::query::prepare(query)?;
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
