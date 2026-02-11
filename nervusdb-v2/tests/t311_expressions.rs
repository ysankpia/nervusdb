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

#[test]
fn test_namespaced_temporal_functions_parse() {
    assert!(prepare("RETURN date.truncate('month', date('1984-10-11'), {}) AS d").is_ok());
    assert!(
        prepare("RETURN duration.between(date('2018-01-01'), date('2018-01-03')) AS d").is_ok()
    );
    assert!(
        prepare(
            "RETURN datetime.fromepoch(416779, 999999999) AS d1, datetime.fromepochmillis(237821673987) AS d2"
        )
        .is_ok()
    );
}

#[test]
fn test_datetime_from_epoch_builders() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN datetime.fromepoch(416779, 999999999) AS d1, datetime.fromepochmillis(237821673987) AS d2",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d1"),
        Some(&Value::String("1970-01-05T19:46:19.999999999Z".to_string()))
    );
    assert_eq!(
        rows[0].get("d2"),
        Some(&Value::String("1977-07-15T13:34:33.987Z".to_string()))
    );
}

#[test]
fn test_date_truncate_month() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare("RETURN date.truncate('month', date('1984-10-11'), {}) AS d").unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String("1984-10-01".to_string()))
    );
}

#[test]
fn test_duration_between_dates_exposes_components() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH duration.between(date('2018-01-01'), date('2018-01-03')) AS d \
         RETURN d AS d, d.days AS days, d.seconds AS seconds, d.nanosecondsOfSecond AS nanos",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("days"), Some(&Value::Int(2)));
    assert_eq!(rows[0].get("seconds"), Some(&Value::Int(172800)));
    assert_eq!(rows[0].get("nanos"), Some(&Value::Int(0)));

    let duration = rows[0].get("d").expect("duration field must exist");
    match duration {
        Value::Map(map) => {
            assert_eq!(map.get("days"), Some(&Value::Int(2)));
            assert_eq!(map.get("seconds"), Some(&Value::Int(172800)));
            assert_eq!(map.get("nanosecondsOfSecond"), Some(&Value::Int(0)));
        }
        other => panic!("expected duration map, got {other:?}"),
    }
}

#[test]
fn test_temporal_projection_from_localdatetime_and_datetime_map() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH localdatetime({year: 1984, week: 10, dayOfWeek: 3, hour: 12, minute: 31, second: 14, millisecond: 645}) AS other \
         RETURN date(other) AS d, localtime(other) AS lt, time(other) AS t, datetime({datetime: other, timezone: '+05:00'}) AS dt",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String("1984-03-07".to_string()))
    );
    assert_eq!(
        rows[0].get("lt"),
        Some(&Value::String("12:31:14.645".to_string()))
    );
    assert_eq!(
        rows[0].get("t"),
        Some(&Value::String("12:31:14.645Z".to_string()))
    );
    assert_eq!(
        rows[0].get("dt"),
        Some(&Value::String("1984-03-07T12:31:14.645+05:00".to_string()))
    );
}

#[test]
fn test_property_access_after_index_and_keyword_property_name() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH [{existing: 42}] AS list, {name: 'Apa', `exists`: 1} AS n \
         RETURN (list[0]).existing AS v, n['name'] AS name, n.exists AS e",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("v"), Some(&Value::Int(42)));
    assert_eq!(rows[0].get("name"), Some(&Value::String("Apa".to_string())));
    assert_eq!(rows[0].get("e"), Some(&Value::Int(1)));
}

#[test]
fn test_properties_function_and_sqrt_function() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH {name: 'Apa', level: 9001} AS n RETURN properties(n) AS props, sqrt(12.96) AS s",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    let mut expected = std::collections::BTreeMap::new();
    expected.insert("name".to_string(), Value::String("Apa".to_string()));
    expected.insert("level".to_string(), Value::Int(9001));
    assert_eq!(rows[0].get("props"), Some(&Value::Map(expected)));
    assert_eq!(rows[0].get("s"), Some(&Value::Float(3.6)));
}

#[test]
fn test_properties_function_invalid_argument_type_compile_error() {
    for query in [
        "RETURN properties(1)",
        "RETURN properties('Cypher')",
        "RETURN properties([true, false])",
    ] {
        let err = prepare(query).expect_err("properties() should reject non-graph/map arguments");
        assert!(
            err.to_string().contains("InvalidArgumentType"),
            "unexpected error for {query}: {err}"
        );
    }
}

#[test]
fn test_optional_match_property_access_existing_value() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let mut txn = db.begin_write();
    let unlabeled = txn.get_or_create_label("").unwrap();
    let n = txn.create_node(1, unlabeled).unwrap();
    txn.set_node_property(
        n,
        "existing".to_string(),
        nervusdb_v2::PropertyValue::Int(42),
    )
    .unwrap();
    txn.set_node_property(n, "missing".to_string(), nervusdb_v2::PropertyValue::Null)
        .unwrap();
    txn.commit().unwrap();

    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare("OPTIONAL MATCH (n) RETURN n AS n, n.missing AS m, n.existing AS e").unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert!(
        rows[0]
            .get("n")
            .is_some_and(|value| !matches!(value, Value::Null)),
        "OPTIONAL MATCH should keep matched node binding"
    );
    assert_eq!(rows[0].get("m"), Some(&Value::Null));
    assert_eq!(rows[0].get("e"), Some(&Value::Int(42)));
}

#[test]
fn test_date_truncate_week_respects_day_of_week_override() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq =
        prepare("RETURN date.truncate('week', date('1984-10-11'), {dayOfWeek: 2}) AS d").unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String("1984-10-09".to_string()))
    );
}

#[test]
fn test_localtime_truncate_millisecond_preserves_fraction_when_overriding_nanosecond() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN localtime.truncate('millisecond', localtime('12:31:14.645876123'), {nanosecond: 2}) AS t",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("t"),
        Some(&Value::String("12:31:14.645000002".to_string()))
    );
}

#[test]
fn test_localtime_truncate_microsecond_preserves_fraction_when_overriding_nanosecond() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN localtime.truncate('microsecond', localtime('12:31:14.645876123'), {nanosecond: 2}) AS t",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("t"),
        Some(&Value::String("12:31:14.645876002".to_string()))
    );
}

#[test]
fn test_time_truncate_accepts_localdatetime_input() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN time.truncate('millisecond', localdatetime('1984-10-11T12:31:14.645876123'), {}) AS t",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("t"),
        Some(&Value::String("12:31:14.645Z".to_string()))
    );
}

#[test]
fn test_localtime_truncate_accepts_datetime_input() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN localtime.truncate('hour', datetime('1984-10-11T12:31:14.645876123+01:00'), {}) AS t",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("t"), Some(&Value::String("12:00".to_string())));
}

#[test]
fn test_localdatetime_truncate_weekyear_from_date_input() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq =
        prepare("RETURN localdatetime.truncate('weekYear', date('1984-02-01'), {day: 5}) AS d")
            .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String("1984-01-05T00:00".to_string()))
    );
}

#[test]
fn test_localtime_truncate_day_from_datetime_input() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN localtime.truncate('day', datetime('1984-10-11T12:31:14.645876123+01:00'), {}) AS t",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("t"), Some(&Value::String("00:00".to_string())));
}

#[test]
fn test_time_truncate_day_from_localdatetime_input() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN time.truncate('day', localdatetime('1984-10-11T12:31:14.645876123'), {}) AS t",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("t"), Some(&Value::String("00:00Z".to_string())));
}

#[test]
fn test_datetime_named_timezone_uses_dst_offset_in_summer() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN datetime({year: 1984, month: 7, day: 20, hour: 12, minute: 31, second: 14, nanosecond: 645876123, timezone: 'Europe/Stockholm'}) AS d",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String(
            "1984-07-20T12:31:14.645876123+02:00[Europe/Stockholm]".to_string(),
        ))
    );
}

#[test]
fn test_temporal8_date_add_sub_duration_with_time_components() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH date({year: 1984, month: 10, day: 11}) AS x, \
         duration({years: 12, months: 5, days: 14, hours: 16, minutes: 12, seconds: 70, nanoseconds: 2}) AS d \
         RETURN x + d AS sum, x - d AS diff",
    )
    .unwrap();

    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("sum"),
        Some(&Value::String("1997-03-25".to_string()))
    );
    assert_eq!(
        rows[0].get("diff"),
        Some(&Value::String("1972-04-27".to_string()))
    );
}

#[test]
fn test_temporal8_duration_arithmetic_scale_and_add_sub() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH duration({years: 12, months: 5, days: 14, hours: 16, minutes: 12, seconds: 70, nanoseconds: 1}) AS d \
         RETURN d + d AS sum, d - d AS diff, d * 2 AS prod, d / 2 AS div, d * 0.5 AS half",
    )
    .unwrap();

    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let display = |value: Option<&Value>| -> Option<String> {
        match value {
            Some(Value::Map(map)) => match map.get("__display") {
                Some(Value::String(s)) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        }
    };

    assert_eq!(rows.len(), 1);
    assert_eq!(
        display(rows[0].get("sum")),
        Some("P24Y10M28DT32H26M20.000000002S".to_string())
    );
    assert_eq!(display(rows[0].get("diff")), Some("PT0S".to_string()));
    assert_eq!(
        display(rows[0].get("prod")),
        Some("P24Y10M28DT32H26M20.000000002S".to_string())
    );
    assert_eq!(
        display(rows[0].get("div")),
        Some("P6Y2M22DT13H21M8S".to_string())
    );
    assert_eq!(
        display(rows[0].get("half")),
        Some("P6Y2M22DT13H21M8S".to_string())
    );
}

#[test]
fn test_temporal1_week_constructor_inherits_day_of_week_from_base_date() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN date({date: date('1816-12-31'), week: 2}) AS d, \
         localdatetime({date: date('1816-12-31'), week: 2}) AS ld, \
         datetime({date: date('1816-12-31'), week: 2}) AS dt",
    )
    .unwrap();

    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String("1817-01-07".to_string()))
    );
    assert_eq!(
        rows[0].get("ld"),
        Some(&Value::String("1817-01-07T00:00".to_string()))
    );
    assert_eq!(
        rows[0].get("dt"),
        Some(&Value::String("1817-01-07T00:00Z".to_string()))
    );
}

#[test]
fn test_temporal1_time_subsecond_components_are_combined() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN localtime({hour: 12, minute: 31, second: 14, nanosecond: 789, millisecond: 123, microsecond: 456}) AS lt, \
         time({hour: 12, minute: 31, second: 14, nanosecond: 789, millisecond: 123, microsecond: 456}) AS t, \
         localdatetime({year: 1984, month: 10, day: 11, hour: 12, minute: 31, second: 14, nanosecond: 789, millisecond: 123, microsecond: 456}) AS ldt, \
         datetime({year: 1984, month: 10, day: 11, hour: 12, minute: 31, second: 14, nanosecond: 789, millisecond: 123, microsecond: 456}) AS dt",
    )
    .unwrap();

    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("lt"),
        Some(&Value::String("12:31:14.123456789".to_string()))
    );
    assert_eq!(
        rows[0].get("t"),
        Some(&Value::String("12:31:14.123456789Z".to_string()))
    );
    assert_eq!(
        rows[0].get("ldt"),
        Some(&Value::String("1984-10-11T12:31:14.123456789".to_string()))
    );
    assert_eq!(
        rows[0].get("dt"),
        Some(&Value::String("1984-10-11T12:31:14.123456789Z".to_string()))
    );
}

#[test]
fn test_temporal1_stockholm_october_uses_standard_offset() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN datetime({year: 1984, month: 10, day: 11, hour: 12, minute: 31, second: 14, timezone: 'Europe/Stockholm'}) AS d",
    )
    .unwrap();

    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String(
            "1984-10-11T12:31:14+01:00[Europe/Stockholm]".to_string()
        ))
    );
}

fn duration_display(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::Map(map)) => match map.get("__display") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

#[test]
fn test_temporal3_quarter_override_inherits_day_from_base_date() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare("RETURN date({date: date('1984-11-11'), quarter: 3}) AS d").unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String("1984-08-11".to_string()))
    );
}

#[test]
fn test_temporal3_datetime_from_time_preserves_named_zone_suffix() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH datetime({year: 1984, month: 10, day: 11, hour: 12, timezone: 'Europe/Stockholm'}) AS other \
         RETURN datetime({year: 1984, month: 10, day: 11, time: other}) AS d",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String(
            "1984-10-11T12:00+01:00[Europe/Stockholm]".to_string(),
        ))
    );
}

#[test]
fn test_temporal3_datetime_from_time_with_honolulu_timezone() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH datetime({year: 1984, month: 10, day: 11, hour: 12, timezone: 'Europe/Stockholm'}) AS other \
         RETURN datetime({year: 1984, month: 10, day: 11, time: other, second: 42, timezone: 'Pacific/Honolulu'}) AS d",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String(
            "1984-10-11T01:00:42-10:00[Pacific/Honolulu]".to_string(),
        ))
    );
}

#[test]
fn test_temporal3_datetime_named_zone_recomputes_offset_after_date_override() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "WITH localdatetime({year: 1984, week: 10, dayOfWeek: 3, hour: 12, minute: 31, second: 14, millisecond: 645}) AS otherDate, \
              datetime({year: 1984, month: 10, day: 11, hour: 12, timezone: 'Europe/Stockholm'}) AS otherTime \
         RETURN datetime({date: otherDate, time: otherTime, day: 28, second: 42}) AS d",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("d"),
        Some(&Value::String(
            "1984-03-28T12:00:42+02:00[Europe/Stockholm]".to_string(),
        ))
    );
}

#[test]
fn test_temporal10_between_respects_dst_boundary() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN duration.between( \
            datetime('2017-10-28T23:00+02:00[Europe/Stockholm]'), \
            datetime('2017-10-29T04:00+01:00[Europe/Stockholm]') \
        ) AS d",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(duration_display(rows[0].get("d")), Some("PT6H".to_string()));
}

#[test]
fn test_temporal10_inmonths_year_boundary_is_not_downgraded_to_11_months() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN duration.inMonths( \
            datetime('2014-07-21T21:40:36.143+0200'), \
            datetime('2015-07-21T21:40:32.142+0100') \
        ) AS d",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(duration_display(rows[0].get("d")), Some("P1Y".to_string()));
}

#[test]
fn test_temporal10_large_date_range_is_supported() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq =
        prepare("RETURN duration.between(date('-999999999-01-01'), date('+999999999-12-31')) AS d")
            .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        duration_display(rows[0].get("d")),
        Some("P1999999998Y11M30D".to_string())
    );
}

#[test]
fn test_temporal10_large_localdatetime_inseconds_is_supported() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN duration.inSeconds(localdatetime('-999999999-01-01'), localdatetime('+999999999-12-31T23:59:59')) AS d",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        duration_display(rows[0].get("d")),
        Some("PT17531639991215H59M59S".to_string())
    );
}

#[test]
fn test_temporal10_inseconds_no_diff_with_now_functions_is_zero() {
    let dir = tempdir().unwrap();
    let db = Db::open(dir.path().join("test.ndb")).unwrap();
    let snapshot = db.snapshot();
    let params = Params::new();

    let pq = prepare(
        "RETURN duration.inSeconds(localtime(), localtime()) AS lt, \
                duration.inSeconds(time(), time()) AS t, \
                duration.inSeconds(date(), date()) AS d, \
                duration.inSeconds(localdatetime(), localdatetime()) AS ldt, \
                duration.inSeconds(datetime(), datetime()) AS dt",
    )
    .unwrap();
    let rows: Vec<_> = pq
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        duration_display(rows[0].get("lt")),
        Some("PT0S".to_string())
    );
    assert_eq!(duration_display(rows[0].get("t")), Some("PT0S".to_string()));
    assert_eq!(duration_display(rows[0].get("d")), Some("PT0S".to_string()));
    assert_eq!(
        duration_display(rows[0].get("ldt")),
        Some("PT0S".to_string())
    );
    assert_eq!(
        duration_display(rows[0].get("dt")),
        Some("PT0S".to_string())
    );
}
