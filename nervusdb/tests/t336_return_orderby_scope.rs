use nervusdb::Db;
use nervusdb::query::{Params, Value, prepare};
use tempfile::tempdir;

fn assert_compile_error_contains(cypher: &str, needle: &str) {
    let err = prepare(cypher)
        .expect_err("query should fail at compile time")
        .to_string();
    assert!(
        err.contains(needle),
        "expected compile error containing {needle:?}, got: {err}"
    );
}

#[test]
fn t336_return_order_by_allows_projected_aggregate_expression() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t336-agg-order.ndb"))?;

    {
        let mut txn = db.begin_write();
        let seed = "CREATE ({division: 'A', age: 22}), ({division: 'B', age: 33}), ({division: 'B', age: 44}), ({division: 'C', age: 55})";
        prepare(seed)?.execute_write(&db.snapshot(), &mut txn, &Params::new())?;
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let query = "MATCH (n) RETURN n.division, max(n.age) ORDER BY max(n.age)";
    let rows: Vec<_> = prepare(query)?
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(rows.len(), 3);

    let mut got = Vec::new();
    for row in rows {
        let division = match row.get("n.division").expect("missing n.division") {
            Value::String(s) => s.clone(),
            other => panic!("expected string division, got {other:?}"),
        };
        let max_age = match row.get("max(n.age)").expect("missing max(n.age)") {
            Value::Int(i) => *i,
            Value::Float(f) => *f as i64,
            other => panic!("expected numeric max age, got {other:?}"),
        };
        got.push((division, max_age));
    }

    assert_eq!(
        got,
        vec![
            ("A".to_string(), 22),
            ("B".to_string(), 44),
            ("C".to_string(), 55),
        ]
    );
    Ok(())
}

#[test]
fn t336_return_distinct_order_by_removed_variable_is_undefined_variable() {
    let cypher = "MATCH (a) RETURN DISTINCT a.name ORDER BY a.age";
    assert_compile_error_contains(cypher, "UndefinedVariable");
}

#[test]
fn t336_return_order_by_non_projected_aggregate_is_invalid_aggregation() {
    let cypher = "MATCH (n) RETURN n.num1 ORDER BY max(n.num2)";
    assert_compile_error_contains(cypher, "InvalidAggregation");
}

#[test]
fn t336_return_order_by_complex_projection_plus_aggregate_is_ambiguous() {
    let cypher = "MATCH (me:Person)--(you:Person) RETURN me.age + you.age, count(*) AS cnt ORDER BY me.age + you.age + count(*)";
    assert_compile_error_contains(cypher, "AmbiguousAggregationExpression");
}

#[test]
fn t336_with_order_by_non_projected_existing_variable_is_visible() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t336-with-order.ndb"))?;

    {
        let mut txn = db.begin_write();
        let seed = "CREATE (:A {num: 1, num2: 4}), (:A {num: 5, num2: 2}), (:A {num: 9, num2: 0}), (:A {num: 3, num2: 3}), (:A {num: 7, num2: 1})";
        prepare(seed)?.execute_write(&db.snapshot(), &mut txn, &Params::new())?;
        txn.commit()?;
    }

    let snapshot = db.snapshot();
    let query = "MATCH (a:A) WITH a, a.num + a.num2 AS sum WITH a, a.num2 % 3 AS mod ORDER BY sum LIMIT 3 RETURN mod";
    let rows: Vec<_> = prepare(query)?
        .execute_streaming(&snapshot, &Params::new())
        .collect::<Result<Vec<_>, _>>()?;

    let mut mods = Vec::new();
    for row in rows {
        let value = row.get("mod").expect("missing mod");
        match value {
            Value::Int(v) => mods.push(*v),
            other => panic!("expected integer mod, got {other:?}"),
        }
    }

    mods.sort_unstable();
    assert_eq!(mods, vec![0, 1, 2]);
    Ok(())
}
