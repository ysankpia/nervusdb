use nervusdb_v2::query::{Params, Value};
use nervusdb_v2::{Db, GraphSnapshot, PropertyValue};
use tempfile::tempdir;

#[test]
fn test_arithmetic_in_where_and_set() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t301_expr.ndb");
    let db = Db::open(&db_path)?;

    // Seed data.
    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;
        let alice = txn.create_node(1, person)?;
        txn.set_node_property(
            alice,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )?;
        txn.set_node_property(alice, "age".to_string(), PropertyValue::Int(20))?;

        let bob = txn.create_node(2, person)?;
        txn.set_node_property(
            bob,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )?;
        txn.set_node_property(bob, "age".to_string(), PropertyValue::Int(30))?;

        txn.commit()?;
    }

    // Arithmetic in WHERE.
    {
        let snapshot = db.snapshot();
        let q = "MATCH (n:Person) WHERE n.age + 1 = 21 RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);

        let n = rows[0].get("n").unwrap();
        let Value::NodeId(iid) = n else {
            panic!("expected node id, got {n:?}");
        };
        assert_eq!(
            snapshot.node_property(*iid, "name"),
            Some(PropertyValue::String("Alice".to_string()))
        );
    }

    // Arithmetic in SET (increment age).
    {
        let read_snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let q = "MATCH (n:Person) WHERE n.name = 'Alice' SET n.age = n.age + 1";
        let prep = nervusdb_v2::query::prepare(q)?;
        let n = prep.execute_write(&read_snapshot, &mut txn, &Params::default())?;
        assert_eq!(n, 1);
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let alice = snapshot
        .nodes()
        .find(|&iid| snapshot.resolve_external(iid) == Some(1))
        .expect("node 1 should exist");
    assert_eq!(
        snapshot.node_property(alice, "age"),
        Some(PropertyValue::Int(21))
    );

    Ok(())
}

#[test]
fn test_string_ops_in_and_count_star() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("t301_str.ndb");
    let db = Db::open(&db_path)?;

    {
        let mut txn = db.begin_write();
        let person = txn.get_or_create_label("Person")?;

        let a = txn.create_node(1, person)?;
        txn.set_node_property(
            a,
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        )?;
        txn.set_node_property(a, "age".to_string(), PropertyValue::Int(20))?;

        let b = txn.create_node(2, person)?;
        txn.set_node_property(
            b,
            "name".to_string(),
            PropertyValue::String("Bob".to_string()),
        )?;
        txn.set_node_property(b, "age".to_string(), PropertyValue::Int(30))?;

        txn.commit()?;
    }

    let snapshot = db.snapshot();

    // STARTS WITH / ENDS WITH / CONTAINS
    {
        let q = "MATCH (n:Person) WHERE n.name STARTS WITH 'A' RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }
    {
        let q = "MATCH (n:Person) WHERE n.name ENDS WITH 'b' RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }
    {
        let q = "MATCH (n:Person) WHERE n.name CONTAINS 'li' RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
    }

    // IN with list literal.
    {
        let q = "MATCH (n:Person) WHERE n.age IN [20, 30] RETURN n";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 2);
    }

    // COUNT(*)
    {
        let q = "MATCH (n:Person) RETURN count(*)";
        let prep = nervusdb_v2::query::prepare(q)?;
        let rows: Vec<_> = prep
            .execute_streaming(&snapshot, &Params::default())
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), 1);
        let count = rows[0].get("count(*)").or_else(|| rows[0].get("agg_0"));
        assert!(matches!(
            count,
            Some(Value::Int(2)) | Some(Value::Float(2.0))
        ));
    }

    Ok(())
}

#[test]
fn test_in_operator_null_semantics_for_nested_lists() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t301_in_nulls.ndb"))?;
    let snapshot = db.snapshot();

    let q = "RETURN [1, null] IN [[1, null]] AS same_with_null, \
             [] IN [1, 2, null] AS empty_vs_null_tail, \
             [1, null] IN [[1, 2], null] AS needs_null_cmp, \
             [1, 2] IN [[1, null], [1, 2]] AS true_beats_null";
    let rows: Vec<_> = nervusdb_v2::query::prepare(q)?
        .execute_streaming(&snapshot, &Params::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].get("same_with_null"), Some(Value::Null)));
    assert!(matches!(
        rows[0].get("empty_vs_null_tail"),
        Some(Value::Null)
    ));
    assert!(matches!(rows[0].get("needs_null_cmp"), Some(Value::Null)));
    assert!(matches!(
        rows[0].get("true_beats_null"),
        Some(Value::Bool(true))
    ));

    Ok(())
}

#[test]
fn test_in_operator_rejects_non_list_literal_rhs_at_compile_time() {
    let cases = [
        "RETURN 1 IN true AS r",
        "RETURN 1 IN 123 AS r",
        "RETURN 1 IN 123.4 AS r",
        "RETURN 1 IN 'foo' AS r",
        "RETURN 1 IN {x: []} AS r",
    ];

    for query in cases {
        let err = nervusdb_v2::query::prepare(query)
            .expect_err("prepare should reject non-list literal rhs")
            .to_string();
        assert!(
            err.contains("InvalidArgumentType"),
            "unexpected compile error for `{query}`: {err}"
        );
    }
}

#[test]
fn test_comparison_list_and_map_null_semantics() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t301_cmp_nulls.ndb"))?;
    let snapshot = db.snapshot();

    let list_q = "RETURN [null] = [1] AS a, [[1], [2]] = [[1], [null]] AS b";
    let list_rows: Vec<_> = nervusdb_v2::query::prepare(list_q)?
        .execute_streaming(&snapshot, &Params::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(list_rows.len(), 1);
    assert!(matches!(list_rows[0].get("a"), Some(Value::Null)));
    assert!(matches!(list_rows[0].get("b"), Some(Value::Null)));

    let map_q = "RETURN {k: null} = {k: null} AS eq1, {k: 1} = {k: null} AS eq2";
    let map_rows: Vec<_> = nervusdb_v2::query::prepare(map_q)?
        .execute_streaming(&snapshot, &Params::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(map_rows.len(), 1);
    assert!(matches!(map_rows[0].get("eq1"), Some(Value::Null)));
    assert!(matches!(map_rows[0].get("eq2"), Some(Value::Null)));

    Ok(())
}

#[test]
fn test_comparison_nan_equality_behavior() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t301_cmp_nan.ndb"))?;
    let snapshot = db.snapshot();

    let q = "RETURN 0.0 / 0.0 = 1 AS is_equal, 0.0 / 0.0 <> 1 AS is_not_equal";
    let rows: Vec<_> = nervusdb_v2::query::prepare(q)?
        .execute_streaming(&snapshot, &Params::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].get("is_equal"), Some(Value::Bool(false))));
    assert!(matches!(
        rows[0].get("is_not_equal"),
        Some(Value::Bool(true))
    ));

    Ok(())
}

#[test]
fn test_large_integer_literal_keeps_precision_in_match() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t301_bigint_match.ndb"))?;
    let params = Params::new();

    let mut txn = db.begin_write();
    nervusdb_v2::query::prepare("CREATE (:TheLabel {id: 4611686018427387905})")?.execute_write(
        &db.snapshot(),
        &mut txn,
        &params,
    )?;
    txn.commit()?;

    let snapshot = db.snapshot();
    let equal_rows: Vec<_> = nervusdb_v2::query::prepare(
        "MATCH (p:TheLabel) WHERE p.id = 4611686018427387905 RETURN p.id",
    )?
    .execute_streaming(&snapshot, &params)
    .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(equal_rows.len(), 1);
    assert!(matches!(
        equal_rows[0].get("p.id"),
        Some(Value::Int(4611686018427387905))
    ));

    let non_equal_rows: Vec<_> = nervusdb_v2::query::prepare(
        "MATCH (p:TheLabel) WHERE p.id = 4611686018427387900 RETURN p.id",
    )?
    .execute_streaming(&snapshot, &params)
    .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(non_equal_rows.len(), 0);

    Ok(())
}

#[test]
fn test_range_comparison_cross_type_returns_null_except_numbers() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t301_range_types.ndb"))?;
    let snapshot = db.snapshot();

    let q =
        "RETURN '1' < 1 AS s_lt_i, '1.0' < 1.0 AS s_lt_f, 1 < 3.14 AS i_lt_f, 3.14 > 1 AS f_gt_i";
    let rows: Vec<_> = nervusdb_v2::query::prepare(q)?
        .execute_streaming(&snapshot, &Params::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].get("s_lt_i"), Some(Value::Null)));
    assert!(matches!(rows[0].get("s_lt_f"), Some(Value::Null)));
    assert!(matches!(rows[0].get("i_lt_f"), Some(Value::Bool(true))));
    assert!(matches!(rows[0].get("f_gt_i"), Some(Value::Bool(true))));

    Ok(())
}

#[test]
fn test_range_comparison_list_and_nan_semantics() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t301_range_list_nan.ndb"))?;
    let snapshot = db.snapshot();

    let q = "RETURN [1, 0] >= [1] AS list_gt_shorter, [1, 2] >= [1, null] AS list_vs_null, [1, 2] >= [3, null] AS list_first_cmp, 0.0 / 0.0 > 1 AS nan_gt_num, 0.0 / 0.0 > 'a' AS nan_gt_str";
    let rows: Vec<_> = nervusdb_v2::query::prepare(q)?
        .execute_streaming(&snapshot, &Params::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        rows[0].get("list_gt_shorter"),
        Some(Value::Bool(true))
    ));
    assert!(matches!(rows[0].get("list_vs_null"), Some(Value::Null)));
    assert!(matches!(
        rows[0].get("list_first_cmp"),
        Some(Value::Bool(false))
    ));
    assert!(matches!(
        rows[0].get("nan_gt_num"),
        Some(Value::Bool(false))
    ));
    assert!(matches!(rows[0].get("nan_gt_str"), Some(Value::Null)));

    Ok(())
}

#[test]
fn test_range_comparison_unwind_numeric_pairs() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t301_range_unwind.ndb"))?;
    let params = Params::new();

    let mut txn = db.begin_write();
    nervusdb_v2::query::prepare("CREATE ()-[:T]->()")?.execute_write(
        &db.snapshot(),
        &mut txn,
        &params,
    )?;
    txn.commit()?;

    let snapshot = db.snapshot();
    let direct_rows: Vec<_> = nervusdb_v2::query::prepare(
        "RETURN 1 < 3.14 AS lt, [1, 3.14][0] < [1, 3.14][1] AS idx_lt",
    )?
    .execute_streaming(&snapshot, &params)
    .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(direct_rows.len(), 1);
    assert!(matches!(direct_rows[0].get("lt"), Some(Value::Bool(true))));
    assert!(matches!(
        direct_rows[0].get("idx_lt"),
        Some(Value::Bool(true))
    ));

    let q = "MATCH p = (n)-[r]->() \
             WITH [n, r, p, '', 1, 3.14, true, null, [], {}] AS types \
             UNWIND range(0, size(types) - 1) AS i \
             UNWIND range(0, size(types) - 1) AS j \
             WITH types[i] AS lhs, types[j] AS rhs \
             WHERE i <> j \
             WITH lhs, rhs, lhs < rhs AS result \
             WHERE result \
             RETURN lhs, rhs";
    let rows: Vec<_> = nervusdb_v2::query::prepare(q)?
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()?;
    let expected_rhs = 157.0_f64 / 50.0_f64;
    assert!(
        rows.iter().any(|row| {
            matches!(row.get("lhs"), Some(Value::Int(1)))
                && matches!(row.get("rhs"), Some(Value::Float(v)) if (*v - expected_rhs).abs() < 1e-12)
        }),
        "expected to find lhs=1, rhs=3.14 in rows: {rows:?}"
    );

    Ok(())
}

#[test]
fn test_hex_and_octal_integer_literals_roundtrip() -> nervusdb_v2::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t301_hex_octal_literals.ndb"))?;
    let snapshot = db.snapshot();

    let q = "RETURN \
             0x1 AS h1, \
             0x7FFFFFFFFFFFFFFF AS hmax, \
             -0x8000000000000000 AS hmin, \
             0o1 AS o1, \
             0o777777777777777777777 AS omax, \
             -0o1000000000000000000000 AS omin";
    let rows: Vec<_> = nervusdb_v2::query::prepare(q)?
        .execute_streaming(&snapshot, &Params::default())
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(rows.len(), 1);

    assert!(matches!(rows[0].get("h1"), Some(Value::Int(1))));
    assert!(matches!(rows[0].get("hmax"), Some(Value::Int(i64::MAX))));
    assert!(matches!(rows[0].get("hmin"), Some(Value::Int(i64::MIN))));
    assert!(matches!(rows[0].get("o1"), Some(Value::Int(1))));
    assert!(matches!(rows[0].get("omax"), Some(Value::Int(i64::MAX))));
    assert!(matches!(rows[0].get("omin"), Some(Value::Int(i64::MIN))));

    Ok(())
}

#[test]
fn test_hex_octal_invalid_and_overflow_compile_errors() {
    let invalid_cases = [
        "RETURN 0x AS literal",
        "RETURN 0x1A2b3j4D5E6f7 AS literal",
        "RETURN 0o AS literal",
        "RETURN 0o9 AS literal",
    ];
    for query in invalid_cases {
        let err = nervusdb_v2::query::prepare(query)
            .expect_err("prepare should fail on invalid prefixed integer literal")
            .to_string();
        assert!(
            err.contains("InvalidNumberLiteral") || err.contains("Invalid number"),
            "unexpected compile error for `{query}`: {err}"
        );
    }

    let overflow_cases = [
        "RETURN 0x8000000000000000 AS literal",
        "RETURN -0x8000000000000001 AS literal",
        "RETURN 0o1000000000000000000000 AS literal",
        "RETURN -0o1000000000000000000001 AS literal",
    ];
    for query in overflow_cases {
        let err = nervusdb_v2::query::prepare(query)
            .expect_err("prepare should fail on overflow prefixed integer literal")
            .to_string();
        assert!(
            err.contains("IntegerOverflow"),
            "unexpected compile error for `{query}`: {err}"
        );
    }
}
