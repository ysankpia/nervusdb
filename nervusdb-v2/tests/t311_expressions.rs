use nervusdb_v2::Db;
use nervusdb_v2::query::{Params, Value, prepare};
use tempfile::tempdir;

#[test]
fn test_complex_expressions() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    // Test: arithmetic precedence and projection expressions
    let query = "RETURN 1 + 2 * 3 AS res, (1 + 2) * 3 AS res2, -10 + 5 AS res3";
    let pq = prepare(query).unwrap();
    let results: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(results.len(), 1);
    // Integer arithmetic returns Int, not Float
    assert_eq!(results[0].get("res").unwrap(), &Value::Int(7));
    assert_eq!(results[0].get("res2").unwrap(), &Value::Int(9));
    assert_eq!(results[0].get("res3").unwrap(), &Value::Int(-5));
}

#[test]
fn test_with_expressions() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    // Test: WITH with expressions
    let query = "WITH 10 + 20 AS x RETURN x * 2 AS y";
    let pq = prepare(query).unwrap();
    let results: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(results.len(), 1);
    // Integer arithmetic returns Int, not Float
    assert_eq!(results[0].get("y").unwrap(), &Value::Int(60));
}

#[test]
fn test_is_null_and_is_not_null() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = "RETURN null IS NULL AS a, null IS NOT NULL AS b, 1 IS NULL AS c, 1 IS NOT NULL AS d";
    let pq = prepare(query).unwrap();
    let results: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("a"), Some(&Value::Bool(true)));
    assert_eq!(results[0].get("b"), Some(&Value::Bool(false)));
    assert_eq!(results[0].get("c"), Some(&Value::Bool(false)));
    assert_eq!(results[0].get("d"), Some(&Value::Bool(true)));
}

#[test]
fn test_order_by_accepts_ascending_keyword() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare("UNWIND [3,1,2] AS n RETURN n ORDER BY n ASCENDING").unwrap();
    let results: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let values: Vec<i64> = results
        .iter()
        .filter_map(|r| match r.get("n") {
            Some(Value::Int(v)) => Some(*v),
            _ => None,
        })
        .collect();
    assert_eq!(values, vec![1, 2, 3]);
}
