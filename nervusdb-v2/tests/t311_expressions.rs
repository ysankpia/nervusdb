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
