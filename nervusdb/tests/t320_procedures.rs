use nervusdb::query::{Params, Value, prepare};
use nervusdb::{Db, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_procedure_db_info() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = "CALL db.info() YIELD version";
    let pq = prepare(query).unwrap();
    let results: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("version").unwrap(),
        &Value::String("2.0.0".to_string())
    );
}

#[test]
fn test_procedure_math_add() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = "CALL math.add(10, 20) YIELD result";
    let pq = prepare(query).unwrap();
    let results: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("result").unwrap(), &Value::Float(30.0));
}

#[test]
fn test_procedure_with_alias() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = "CALL math.add(5, 5) YIELD result AS ten";
    let pq = prepare(query).unwrap();
    let results: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("ten").unwrap(), &Value::Float(10.0));
}

#[test]
fn test_correlated_procedure_call() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();

    let mut txn = db.begin_write();
    let label_id = txn.get_or_create_label("Person").unwrap();

    let n1 = txn.create_node(1, label_id).unwrap();
    txn.set_node_property(n1, "val".to_string(), PropertyValue::Int(100))
        .unwrap();

    let n2 = txn.create_node(2, label_id).unwrap();
    txn.set_node_property(n2, "val".to_string(), PropertyValue::Int(200))
        .unwrap();

    txn.commit().unwrap();

    let snapshot = db.snapshot();
    let params = Params::new();

    // Correlated call: math.add(n.val, 1) for each Person
    let query = "MATCH (n:Person) CALL math.add(n.val, 1) YIELD result RETURN result";
    let pq = prepare(query).unwrap();
    let results: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(results.len(), 2);
    let vals: Vec<f64> = results
        .iter()
        .map(|r| match r.get("result").unwrap() {
            Value::Float(f) => *f,
            _ => panic!("Expected float"),
        })
        .collect();

    assert!(vals.contains(&101.0));
    assert!(vals.contains(&201.0));
}
