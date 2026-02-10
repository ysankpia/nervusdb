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

    let query =
        "RETURN null IS NULL AS a, null IS NOT NULL AS b, 1 IS NULL AS c, 1 IS NOT NULL AS d";
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

#[test]
fn test_precedence_not_with_comparisons() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    // openCypher Precedence1 [20]: (NOT (a = b)) == ((NOT a) = b)
    let pq = prepare(
        "UNWIND [true, false, null] AS a \
         UNWIND [true, false, null] AS b \
         WITH collect((NOT (a = b)) = ((NOT a) = b)) AS eq \
         RETURN all(x IN eq WHERE x) AS result",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("result"), Some(&Value::Bool(true)));
}

#[test]
fn test_list_comprehension_basic() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq =
        prepare("WITH [1,2,3,4] AS xs RETURN [x IN xs WHERE x % 2 = 0 | x + 1] AS ys").unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("ys"),
        Some(&Value::List(vec![Value::Int(3), Value::Int(5)]))
    );
}

#[test]
fn test_map_property_access() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare("WITH {a: 1, b: 'x'} AS m RETURN m.a AS a, m.b AS b").unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("a"), Some(&Value::Int(1)));
    assert_eq!(rows[0].get("b"), Some(&Value::String("x".to_string())));
}

#[test]
fn test_list_concatenation_plus() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare("RETURN [1,2] + 3 AS a, 0 + [1,2] AS b, [1] + [2] AS c").unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("a"),
        Some(&Value::List(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3)
        ]))
    );
    assert_eq!(
        rows[0].get("b"),
        Some(&Value::List(vec![
            Value::Int(0),
            Value::Int(1),
            Value::Int(2)
        ]))
    );
    assert_eq!(
        rows[0].get("c"),
        Some(&Value::List(vec![Value::Int(1), Value::Int(2)]))
    );
}

#[test]
fn test_temporal_constructors_from_map_literals() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = "RETURN \
        date({year: 1910, month: 5, day: 6}) AS d, \
        localtime({hour: 10, minute: 35}) AS lt, \
        time({hour: 12, minute: 35, second: 15, timezone: '+05:00'}) AS t, \
        localdatetime({year: 1984, month: 10, day: 11, hour: 12, minute: 30, second: 14, nanosecond: 12}) AS ldt, \
        datetime({year: 1984, month: 10, day: 11, hour: 12, minute: 30, second: 14, nanosecond: 12, timezone: '+00:15'}) AS dt";
    let pq = prepare(query).unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String("1910-05-06".to_string()))
    );
    assert_eq!(rows[0].get("lt"), Some(&Value::String("10:35".to_string())));
    assert_eq!(
        rows[0].get("t"),
        Some(&Value::String("12:35:15+05:00".to_string()))
    );
    assert_eq!(
        rows[0].get("ldt"),
        Some(&Value::String("1984-10-11T12:30:14.000000012".to_string()))
    );
    assert_eq!(
        rows[0].get("dt"),
        Some(&Value::String(
            "1984-10-11T12:30:14.000000012+00:15".to_string()
        ))
    );
}

#[test]
fn test_order_by_temporal_time_with_duration_offset() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH [
            time({hour: 10, minute: 35, timezone: '-08:00'}),
            time({hour: 12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}),
            time({hour: 12, minute: 31, second: 14, nanosecond: 645876124, timezone: '+01:00'}),
            time({hour: 12, minute: 35, second: 15, timezone: '+05:00'}),
            time({hour: 12, minute: 30, second: 14, nanosecond: 645876123, timezone: '+01:01'})
        ] AS ts
        UNWIND ts AS t
        RETURN t
        ORDER BY t + duration({minutes: 6}) ASC
        LIMIT 3",
    )
    .unwrap();

    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let values: Vec<String> = rows
        .iter()
        .map(|r| match r.get("t").unwrap() {
            Value::String(s) => s.clone(),
            other => panic!("expected string time, got {other:?}"),
        })
        .collect();

    assert_eq!(
        values,
        vec![
            "12:35:15+05:00".to_string(),
            "12:30:14.645876123+01:01".to_string(),
            "12:31:14.645876123+01:00".to_string(),
        ]
    );
}

#[test]
fn test_boolean_three_valued_logic() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let query = "RETURN null AND false AS and_false, \
                        null AND true AS and_true, \
                        null OR true AS or_true, \
                        null OR false AS or_false, \
                        true XOR null AS xor_null";
    let pq = prepare(query).unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("and_false"), Some(&Value::Bool(false)));
    assert_eq!(rows[0].get("and_true"), Some(&Value::Null));
    assert_eq!(rows[0].get("or_true"), Some(&Value::Bool(true)));
    assert_eq!(rows[0].get("or_false"), Some(&Value::Null));
    assert_eq!(rows[0].get("xor_null"), Some(&Value::Null));
}
