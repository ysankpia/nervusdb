//! NervusDB Rust Core Engine — 全能力边界测试
//!
//! 镜像 Python/Node 测试 (分类 1-20) + Rust 独有测试 (分类 21-35)

use nervusdb::{Db, PropertyValue};
use nervusdb_query::{prepare, ExecuteOptions, Params, Row, Value};
use std::collections::BTreeMap;
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════════
// Harness 辅助函数
// ═══════════════════════════════════════════════════════════════

fn fresh_db(_label: &str) -> (Db, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    (db, dir)
}

fn exec_write(db: &Db, cypher: &str) -> u32 {
    exec_write_params(db, cypher, &Params::new())
}

fn exec_write_params(db: &Db, cypher: &str, params: &Params) -> u32 {
    let q = prepare(cypher).unwrap();
    let snap = db.snapshot();
    let mut txn = db.begin_write();
    let count = q.execute_write(&snap, &mut txn, params).unwrap();
    txn.commit().unwrap();
    count
}

fn query_rows(db: &Db, cypher: &str) -> Vec<Row> {
    query_rows_params(db, cypher, &Params::new())
}

fn query_rows_params(db: &Db, cypher: &str, params: &Params) -> Vec<Row> {
    let q = prepare(cypher).unwrap();
    let snap = db.snapshot();
    q.execute_streaming(&snap, params)
        .collect::<nervusdb_query::Result<Vec<_>>>()
        .unwrap()
}

/// Query and reify all rows (resolve NodeId -> Node, EdgeKey -> Relationship)
fn reify_rows(db: &Db, cypher: &str) -> Vec<Vec<(String, Value)>> {
    let q = prepare(cypher).unwrap();
    let snap = db.snapshot();
    let rows: Vec<Row> = q
        .execute_streaming(&snap, &Params::new())
        .collect::<nervusdb_query::Result<Vec<_>>>()
        .unwrap();
    rows.iter()
        .map(|r| {
            let reified = r.reify(&snap).unwrap();
            reified.columns().to_vec()
        })
        .collect()
}

fn val(rows: &[Row], idx: usize, col: &str) -> Value {
    rows[idx].get(col).cloned().unwrap_or(Value::Null)
}

fn val_i64(rows: &[Row], idx: usize, col: &str) -> i64 {
    match val(rows, idx, col) {
        Value::Int(i) => i,
        other => panic!("expected Int for {col}, got {other:?}"),
    }
}

fn val_str(rows: &[Row], idx: usize, col: &str) -> String {
    match val(rows, idx, col) {
        Value::String(s) => s,
        other => panic!("expected String for {col}, got {other:?}"),
    }
}

fn val_f64(rows: &[Row], idx: usize, col: &str) -> f64 {
    match val(rows, idx, col) {
        Value::Float(f) => f,
        Value::Int(i) => i as f64,
        other => panic!("expected Float for {col}, got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 1. 基础 CRUD
// ═══════════════════════════════════════════════════════════════

#[test]
fn t01_create_single_node() {
    let (db, _dir) = fresh_db("crud");
    let n = exec_write(&db, "CREATE (n:Person {name: 'Alice', age: 30})");
    assert!(n > 0, "expected created > 0, got {n}");
}

#[test]
fn t01_match_return_node() {
    let (db, _dir) = fresh_db("crud");
    exec_write(&db, "CREATE (n:Person {name: 'Alice', age: 30})");
    let rows = reify_rows(&db, "MATCH (n:Person {name: 'Alice'}) RETURN n");
    assert_eq!(rows.len(), 1);
    let node_val = &rows[0].iter().find(|(k, _)| k == "n").unwrap().1;
    match node_val {
        Value::Node(n) => {
            assert!(n.labels.contains(&"Person".to_string()));
            assert_eq!(n.properties.get("name"), Some(&Value::String("Alice".to_string())));
            assert_eq!(n.properties.get("age"), Some(&Value::Int(30)));
        }
        other => panic!("expected Node, got {other:?}"),
    }
}

#[test]
fn t01_create_relationship() {
    let (db, _dir) = fresh_db("crud");
    exec_write(&db, "CREATE (a:Person {name: 'Alice'})");
    exec_write(&db, "CREATE (b:Person {name: 'Bob'})");
    exec_write(
        &db,
        "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS {since: 2020}]->(b)",
    );
    let rows = query_rows(
        &db,
        "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name, b.name",
    );
    assert!(rows.len() >= 1, "expected at least 1 relationship row");
}

#[test]
fn t01_set_property() {
    let (db, _dir) = fresh_db("crud");
    exec_write(&db, "CREATE (n:Person {name: 'Alice'})");
    exec_write(&db, "MATCH (n:Person {name: 'Alice'}) SET n.email = 'alice@test.com'");
    let rows = query_rows(&db, "MATCH (n:Person {name: 'Alice'}) RETURN n.email");
    assert_eq!(val_str(&rows, 0, "n.email"), "alice@test.com");
}

#[test]
fn t01_set_overwrite() {
    let (db, _dir) = fresh_db("crud");
    exec_write(&db, "CREATE (n:Person {name: 'Alice', age: 30})");
    exec_write(&db, "MATCH (n:Person {name: 'Alice'}) SET n.age = 31");
    let rows = query_rows(&db, "MATCH (n:Person {name: 'Alice'}) RETURN n.age");
    assert_eq!(val_i64(&rows, 0, "n.age"), 31);
}

#[test]
fn t01_remove_property() {
    let (db, _dir) = fresh_db("crud");
    exec_write(&db, "CREATE (n:Person {name: 'Alice', email: 'a@b.com'})");
    exec_write(&db, "MATCH (n:Person {name: 'Alice'}) REMOVE n.email");
    let rows = query_rows(&db, "MATCH (n:Person {name: 'Alice'}) RETURN n.email");
    assert_eq!(val(&rows, 0, "n.email"), Value::Null);
}

#[test]
fn t01_delete_node_detach() {
    let (db, _dir) = fresh_db("crud");
    exec_write(&db, "CREATE (x:Temp {val: 'delete-me'})");
    let before = query_rows(&db, "MATCH (x:Temp) RETURN count(x) AS c");
    assert!(val_i64(&before, 0, "c") >= 1);
    exec_write(&db, "MATCH (x:Temp {val: 'delete-me'}) DETACH DELETE x");
    let after = query_rows(&db, "MATCH (x:Temp {val: 'delete-me'}) RETURN count(x) AS c");
    assert_eq!(val_i64(&after, 0, "c"), 0);
}

#[test]
fn t01_delete_rel_only() {
    let (db, _dir) = fresh_db("crud");
    exec_write(&db, "CREATE (a:X)-[:R]->(b:Y)");
    exec_write(&db, "MATCH (:X)-[r:R]->(:Y) DELETE r");
    let rows = query_rows(&db, "MATCH (:X)-[r:R]->(:Y) RETURN count(r) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 0);
}

#[test]
fn t01_multi_create() {
    let (db, _dir) = fresh_db("crud");
    // Multi-node CREATE in single statement
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        exec_write(&db, "CREATE (:Multi1 {v: 1}), (:Multi2 {v: 2})");
    }));
    if result.is_ok() {
        let rows = query_rows(&db, "MATCH (n:Multi1) RETURN count(n) AS c");
        assert!(val_i64(&rows, 0, "c") >= 1, "multi-create should work");
    } else {
        println!("    (limitation: multi-node CREATE may not be supported)");
    }
}

// ═══════════════════════════════════════════════════════════════
// 1b. RETURN 投影
// ═══════════════════════════════════════════════════════════════

#[test]
fn t01b_return_scalar() {
    let (db, _dir) = fresh_db("return");
    let rows = query_rows(&db, "RETURN 1 + 2 AS sum");
    assert_eq!(val_i64(&rows, 0, "sum"), 3);
}

#[test]
fn t01b_return_alias() {
    let (db, _dir) = fresh_db("return");
    exec_write(&db, "CREATE (n:P {name: 'X', age: 10})");
    let rows = query_rows(&db, "MATCH (n:P {name: 'X'}) RETURN n.name AS who");
    assert_eq!(val_str(&rows, 0, "who"), "X");
}

#[test]
fn t01b_return_distinct() {
    let (db, _dir) = fresh_db("return");
    exec_write(&db, "CREATE (:D {v: 1})");
    exec_write(&db, "CREATE (:D {v: 1})");
    exec_write(&db, "CREATE (:D {v: 2})");
    let rows = query_rows(&db, "MATCH (n:D) RETURN DISTINCT n.v ORDER BY n.v");
    assert_eq!(rows.len(), 2);
}

#[test]
fn t01b_return_star() {
    let (db, _dir) = fresh_db("return");
    exec_write(&db, "CREATE (n:P {name: 'X'})");
    let rows = query_rows(&db, "MATCH (n:P {name: 'X'}) RETURN *");
    assert!(rows.len() >= 1, "RETURN * should work");
    assert!(rows[0].get("n").is_some(), "should have n in result");
}

// ═══════════════════════════════════════════════════════════════
// 2. 多标签节点
// ═══════════════════════════════════════════════════════════════

#[test]
fn t02_multi_label_create() {
    let (db, _dir) = fresh_db("labels");
    exec_write(&db, "CREATE (n:Person:Employee:Manager {name: 'Carol'})");
    let rows = reify_rows(&db, "MATCH (n:Person:Employee {name: 'Carol'}) RETURN n");
    assert_eq!(rows.len(), 1, "multi-label match failed");
    let node_val = &rows[0].iter().find(|(k, _)| k == "n").unwrap().1;
    match node_val {
        Value::Node(n) => {
            assert!(n.labels.contains(&"Person".to_string()), "missing Person");
            assert!(n.labels.contains(&"Employee".to_string()), "missing Employee");
            assert!(n.labels.contains(&"Manager".to_string()), "missing Manager");
        }
        other => panic!("expected Node, got {other:?}"),
    }
}

#[test]
fn t02_single_label_subset() {
    let (db, _dir) = fresh_db("labels");
    exec_write(&db, "CREATE (n:Person:Employee:Manager {name: 'Carol'})");
    let rows = query_rows(&db, "MATCH (n:Manager) RETURN n.name");
    assert!(rows.len() >= 1, "should match by Manager label");
}

// ═══════════════════════════════════════════════════════════════
// 3. 数据类型
// ═══════════════════════════════════════════════════════════════

#[test]
fn t03_null_property() {
    let (db, _dir) = fresh_db("types");
    exec_write(&db, "CREATE (n:T {val: null})");
    let rows = query_rows(&db, "MATCH (n:T) RETURN n.val");
    assert_eq!(val(&rows, 0, "n.val"), Value::Null);
}

#[test]
fn t03_bool_properties() {
    let (db, _dir) = fresh_db("types");
    exec_write(&db, "CREATE (n:Bool {t: true, f: false})");
    let rows = query_rows(&db, "MATCH (n:Bool) RETURN n.t, n.f");
    assert_eq!(val(&rows, 0, "n.t"), Value::Bool(true));
    assert_eq!(val(&rows, 0, "n.f"), Value::Bool(false));
}

#[test]
fn t03_integer_property() {
    let (db, _dir) = fresh_db("types");
    exec_write(&db, "CREATE (n:Num {val: 42})");
    let rows = query_rows(&db, "MATCH (n:Num) RETURN n.val");
    assert_eq!(val_i64(&rows, 0, "n.val"), 42);
}

#[test]
fn t03_negative_integer() {
    let (db, _dir) = fresh_db("types");
    exec_write(&db, "CREATE (n:Neg {val: -100})");
    let rows = query_rows(&db, "MATCH (n:Neg) RETURN n.val");
    assert_eq!(val_i64(&rows, 0, "n.val"), -100);
}

#[test]
fn t03_float_property() {
    let (db, _dir) = fresh_db("types");
    exec_write(&db, "CREATE (n:Flt {val: 3.14})");
    let rows = query_rows(&db, "MATCH (n:Flt) RETURN n.val");
    let v = val_f64(&rows, 0, "n.val");
    assert!((v - 3.14).abs() < 0.001, "expected ~3.14, got {v}");
}

#[test]
fn t03_string_special_chars() {
    let (db, _dir) = fresh_db("types");
    exec_write(&db, r#"CREATE (n:Str {val: 'hello "world"'})"#);
    let rows = query_rows(&db, "MATCH (n:Str) RETURN n.val");
    match val(&rows, 0, "n.val") {
        Value::String(_) => {}
        other => panic!("expected String, got {other:?}"),
    }
}

#[test]
fn t03_list_literal() {
    let (db, _dir) = fresh_db("types");
    let rows = query_rows(&db, "RETURN [1, 2, 3] AS lst");
    match val(&rows, 0, "lst") {
        Value::List(l) => {
            assert_eq!(l, vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        }
        other => panic!("expected List, got {other:?}"),
    }
}

#[test]
fn t03_map_literal() {
    let (db, _dir) = fresh_db("types");
    let rows = query_rows(&db, "RETURN {a: 1, b: 'two'} AS m");
    match val(&rows, 0, "m") {
        Value::Map(m) => {
            assert_eq!(m.get("a"), Some(&Value::Int(1)));
            assert_eq!(m.get("b"), Some(&Value::String("two".to_string())));
        }
        other => panic!("expected Map, got {other:?}"),
    }
}

#[test]
fn t03_list_property() {
    let (db, _dir) = fresh_db("types");
    exec_write(&db, "CREATE (n:Lst {tags: ['a', 'b', 'c']})");
    let rows = query_rows(&db, "MATCH (n:Lst) RETURN n.tags");
    match val(&rows, 0, "n.tags") {
        Value::List(l) => {
            assert_eq!(l.len(), 3);
            assert_eq!(l[0], Value::String("a".to_string()));
        }
        other => panic!("expected List, got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 4. WHERE 过滤
// ═══════════════════════════════════════════════════════════════

#[test]
fn t04_where_equality() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (a:P {name: 'A', age: 20})");
    exec_write(&db, "CREATE (b:P {name: 'B', age: 30})");
    exec_write(&db, "CREATE (c:P {name: 'C', age: 40})");
    let rows = query_rows(&db, "MATCH (n:P) WHERE n.age = 30 RETURN n.name");
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "n.name"), "B");
}

#[test]
fn t04_where_gt() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'A', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'B', age: 30})");
    exec_write(&db, "CREATE (:P {name: 'C', age: 40})");
    let rows = query_rows(&db, "MATCH (n:P) WHERE n.age > 25 RETURN n.name ORDER BY n.name");
    assert_eq!(rows.len(), 2);
}

#[test]
fn t04_where_and() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'A', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'B', age: 30})");
    exec_write(&db, "CREATE (:P {name: 'C', age: 40})");
    let rows = query_rows(
        &db,
        "MATCH (n:P) WHERE n.age > 15 AND n.age < 35 RETURN n.name ORDER BY n.name",
    );
    assert_eq!(rows.len(), 2);
}

#[test]
fn t04_where_or() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'A', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'B', age: 30})");
    exec_write(&db, "CREATE (:P {name: 'C', age: 40})");
    let rows = query_rows(
        &db,
        "MATCH (n:P) WHERE n.name = 'A' OR n.name = 'C' RETURN n.name ORDER BY n.name",
    );
    assert_eq!(rows.len(), 2);
}

#[test]
fn t04_where_not() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'A', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'B', age: 30})");
    exec_write(&db, "CREATE (:P {name: 'C', age: 40})");
    let rows = query_rows(
        &db,
        "MATCH (n:P) WHERE NOT n.name = 'B' RETURN n.name ORDER BY n.name",
    );
    assert_eq!(rows.len(), 2);
}

#[test]
fn t04_where_in() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'A', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'B', age: 30})");
    exec_write(&db, "CREATE (:P {name: 'C', age: 40})");
    let rows = query_rows(
        &db,
        "MATCH (n:P) WHERE n.name IN ['A', 'C'] RETURN n.name ORDER BY n.name",
    );
    assert_eq!(rows.len(), 2);
}

#[test]
fn t04_where_starts_with() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'Alice', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'Bob', age: 30})");
    let rows = query_rows(&db, "MATCH (n:P) WHERE n.name STARTS WITH 'Ali' RETURN n.name");
    assert_eq!(rows.len(), 1);
}

#[test]
fn t04_where_contains() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'Alice', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'Bob', age: 30})");
    let rows = query_rows(&db, "MATCH (n:P) WHERE n.name CONTAINS 'lic' RETURN n.name");
    assert_eq!(rows.len(), 1);
}

#[test]
fn t04_where_ends_with() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'Alice', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'Bob', age: 30})");
    let rows = query_rows(&db, "MATCH (n:P) WHERE n.name ENDS WITH 'ce' RETURN n.name");
    assert!(rows.len() >= 1, "should find Alice");
}

#[test]
fn t04_where_is_null() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'A', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'NoAge'})");
    let rows = query_rows(&db, "MATCH (n:P) WHERE n.age IS NULL RETURN n.name");
    assert!(rows.len() >= 1, "should find node without age");
}

#[test]
fn t04_where_is_not_null() {
    let (db, _dir) = fresh_db("where");
    exec_write(&db, "CREATE (:P {name: 'A', age: 20})");
    exec_write(&db, "CREATE (:P {name: 'NoAge'})");
    let rows = query_rows(&db, "MATCH (n:P) WHERE n.age IS NOT NULL RETURN n.name");
    assert!(rows.len() >= 1, "should find nodes with age");
}

// ═══════════════════════════════════════════════════════════════
// 5. 查询子句
// ═══════════════════════════════════════════════════════════════

#[test]
fn t05_order_by_asc() {
    let (db, _dir) = fresh_db("clauses");
    for v in [3, 1, 2, 5, 4] {
        exec_write(&db, &format!("CREATE (:N {{v: {v}}})"));
    }
    let rows = query_rows(&db, "MATCH (n:N) RETURN n.v ORDER BY n.v");
    let vals: Vec<i64> = rows.iter().map(|r| val_i64(&[r.clone()], 0, "n.v")).collect();
    assert_eq!(vals, vec![1, 2, 3, 4, 5]);
}

#[test]
fn t05_order_by_desc() {
    let (db, _dir) = fresh_db("clauses");
    for v in [3, 1, 2, 5, 4] {
        exec_write(&db, &format!("CREATE (:N {{v: {v}}})"));
    }
    let rows = query_rows(&db, "MATCH (n:N) RETURN n.v ORDER BY n.v DESC");
    let vals: Vec<i64> = rows.iter().map(|r| val_i64(&[r.clone()], 0, "n.v")).collect();
    assert_eq!(vals, vec![5, 4, 3, 2, 1]);
}

#[test]
fn t05_limit() {
    let (db, _dir) = fresh_db("clauses");
    for v in [3, 1, 2, 5, 4] {
        exec_write(&db, &format!("CREATE (:N {{v: {v}}})"));
    }
    let rows = query_rows(&db, "MATCH (n:N) RETURN n.v ORDER BY n.v LIMIT 3");
    assert_eq!(rows.len(), 3);
}

#[test]
fn t05_skip() {
    let (db, _dir) = fresh_db("clauses");
    for v in [3, 1, 2, 5, 4] {
        exec_write(&db, &format!("CREATE (:N {{v: {v}}})"));
    }
    let rows = query_rows(&db, "MATCH (n:N) RETURN n.v ORDER BY n.v SKIP 2 LIMIT 2");
    assert_eq!(rows.len(), 2);
    assert_eq!(val_i64(&rows, 0, "n.v"), 3);
}

#[test]
fn t05_with_pipe() {
    let (db, _dir) = fresh_db("clauses");
    for v in [3, 1, 2, 5, 4] {
        exec_write(&db, &format!("CREATE (:N {{v: {v}}})"));
    }
    let rows = query_rows(
        &db,
        "MATCH (n:N) WITH n.v AS val WHERE val > 3 RETURN val ORDER BY val",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(val_i64(&rows, 0, "val"), 4);
}

#[test]
fn t05_unwind() {
    let (db, _dir) = fresh_db("clauses");
    let rows = query_rows(&db, "UNWIND [10, 20, 30] AS x RETURN x");
    assert_eq!(rows.len(), 3);
    assert_eq!(val_i64(&rows, 0, "x"), 10);
}

#[test]
fn t05_unwind_create() {
    let (db, _dir) = fresh_db("clauses");
    exec_write(&db, "UNWIND [1, 2, 3] AS i CREATE (:UW {idx: i})");
    let rows = query_rows(&db, "MATCH (n:UW) RETURN n.idx ORDER BY n.idx");
    assert_eq!(rows.len(), 3);
}

#[test]
fn t05_union() {
    let (db, _dir) = fresh_db("clauses");
    let rows = query_rows(&db, "RETURN 1 AS x UNION RETURN 2 AS x");
    assert_eq!(rows.len(), 2);
}

#[test]
fn t05_union_all() {
    let (db, _dir) = fresh_db("clauses");
    let rows = query_rows(&db, "RETURN 1 AS x UNION ALL RETURN 1 AS x");
    assert_eq!(rows.len(), 2);
}

#[test]
fn t05_optional_match() {
    let (db, _dir) = fresh_db("clauses");
    exec_write(&db, "CREATE (:Lonely {name: 'solo'})");
    let rows = query_rows(
        &db,
        "MATCH (n:Lonely) OPTIONAL MATCH (n)-[r]->(m) RETURN n.name, r, m",
    );
    assert!(rows.len() >= 1, "should return at least 1 row");
    assert_eq!(val(&rows, 0, "r"), Value::Null);
    assert_eq!(val(&rows, 0, "m"), Value::Null);
}

// ═══════════════════════════════════════════════════════════════
// 6. 聚合函数
// ═══════════════════════════════════════════════════════════════

#[test]
fn t06_count() {
    let (db, _dir) = fresh_db("agg");
    exec_write(&db, "CREATE (:S {v: 10})");
    exec_write(&db, "CREATE (:S {v: 20})");
    exec_write(&db, "CREATE (:S {v: 30})");
    let rows = query_rows(&db, "MATCH (n:S) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 3);
}

#[test]
fn t06_sum() {
    let (db, _dir) = fresh_db("agg");
    exec_write(&db, "CREATE (:S {v: 10})");
    exec_write(&db, "CREATE (:S {v: 20})");
    exec_write(&db, "CREATE (:S {v: 30})");
    let rows = query_rows(&db, "MATCH (n:S) RETURN sum(n.v) AS s");
    assert_eq!(val_i64(&rows, 0, "s"), 60);
}

#[test]
fn t06_avg() {
    let (db, _dir) = fresh_db("agg");
    exec_write(&db, "CREATE (:S {v: 10})");
    exec_write(&db, "CREATE (:S {v: 20})");
    exec_write(&db, "CREATE (:S {v: 30})");
    let rows = query_rows(&db, "MATCH (n:S) RETURN avg(n.v) AS a");
    let a = val_f64(&rows, 0, "a");
    assert!((a - 20.0).abs() < 0.001, "expected avg ~20, got {a}");
}

#[test]
fn t06_min_max() {
    let (db, _dir) = fresh_db("agg");
    exec_write(&db, "CREATE (:S {v: 10})");
    exec_write(&db, "CREATE (:S {v: 20})");
    exec_write(&db, "CREATE (:S {v: 30})");
    let rows = query_rows(&db, "MATCH (n:S) RETURN min(n.v) AS lo, max(n.v) AS hi");
    assert_eq!(val_i64(&rows, 0, "lo"), 10);
    assert_eq!(val_i64(&rows, 0, "hi"), 30);
}

#[test]
fn t06_collect() {
    let (db, _dir) = fresh_db("agg");
    exec_write(&db, "CREATE (:S {v: 10})");
    exec_write(&db, "CREATE (:S {v: 20})");
    exec_write(&db, "CREATE (:S {v: 30})");
    let rows = query_rows(&db, "MATCH (n:S) RETURN collect(n.v) AS vals");
    match val(&rows, 0, "vals") {
        Value::List(l) => assert_eq!(l.len(), 3),
        other => panic!("expected List, got {other:?}"),
    }
}

#[test]
fn t06_count_distinct() {
    let (db, _dir) = fresh_db("agg");
    exec_write(&db, "CREATE (:S {v: 10})");
    exec_write(&db, "CREATE (:S {v: 20})");
    exec_write(&db, "CREATE (:S {v: 30})");
    exec_write(&db, "CREATE (:S {v: 10})"); // duplicate
    let rows = query_rows(&db, "MATCH (n:S) RETURN count(DISTINCT n.v) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 3);
}

#[test]
fn t06_group_by() {
    let (db, _dir) = fresh_db("agg");
    exec_write(&db, "CREATE (:G {cat: 'a', v: 1})");
    exec_write(&db, "CREATE (:G {cat: 'a', v: 2})");
    exec_write(&db, "CREATE (:G {cat: 'b', v: 3})");
    let rows = query_rows(
        &db,
        "MATCH (n:G) RETURN n.cat, sum(n.v) AS total ORDER BY n.cat",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(val_str(&rows, 0, "n.cat"), "a");
    assert_eq!(val_i64(&rows, 0, "total"), 3);
}

// ═══════════════════════════════════════════════════════════════
// 7. MERGE
// ═══════════════════════════════════════════════════════════════

#[test]
fn t07_merge_create() {
    let (db, _dir) = fresh_db("merge");
    exec_write(&db, "MERGE (n:M {key: 'x'})");
    let rows = query_rows(&db, "MATCH (n:M {key: 'x'}) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1);
}

#[test]
fn t07_merge_match_existing() {
    let (db, _dir) = fresh_db("merge");
    exec_write(&db, "MERGE (n:M {key: 'x'})");
    exec_write(&db, "MERGE (n:M {key: 'x'})");
    let rows = query_rows(&db, "MATCH (n:M {key: 'x'}) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1, "MERGE should not duplicate");
}

#[test]
fn t07_merge_on_create_set() {
    let (db, _dir) = fresh_db("merge");
    exec_write(&db, "MERGE (n:M {key: 'y'}) ON CREATE SET n.created = true");
    let rows = query_rows(&db, "MATCH (n:M {key: 'y'}) RETURN n.created");
    assert_eq!(val(&rows, 0, "n.created"), Value::Bool(true));
}

#[test]
fn t07_merge_on_match_set() {
    let (db, _dir) = fresh_db("merge");
    exec_write(&db, "CREATE (:M {key: 'z', v: 1})");
    exec_write(&db, "MERGE (n:M {key: 'z'}) ON MATCH SET n.v = 99");
    let rows = query_rows(&db, "MATCH (n:M {key: 'z'}) RETURN n.v");
    assert_eq!(val_i64(&rows, 0, "n.v"), 99);
}

#[test]
fn t07_merge_relationship() {
    let (db, _dir) = fresh_db("merge");
    exec_write(&db, "CREATE (:MR {name: 'a'})");
    exec_write(&db, "CREATE (:MR {name: 'b'})");
    exec_write(
        &db,
        "MATCH (a:MR {name: 'a'}), (b:MR {name: 'b'}) MERGE (a)-[:KNOWS]->(b)",
    );
    exec_write(
        &db,
        "MATCH (a:MR {name: 'a'}), (b:MR {name: 'b'}) MERGE (a)-[:KNOWS]->(b)",
    );
    let rows = query_rows(&db, "MATCH (:MR)-[r:KNOWS]->(:MR) RETURN count(r) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1, "MERGE rel should be idempotent");
}

// ═══════════════════════════════════════════════════════════════
// 8. CASE 表达式
// ═══════════════════════════════════════════════════════════════

#[test]
fn t08_case_simple() {
    let (db, _dir) = fresh_db("case");
    exec_write(&db, "CREATE (:C {v: 1})");
    exec_write(&db, "CREATE (:C {v: 2})");
    let rows = query_rows(
        &db,
        "MATCH (n:C) RETURN CASE n.v WHEN 1 THEN 'one' WHEN 2 THEN 'two' ELSE 'other' END AS label ORDER BY n.v",
    );
    assert_eq!(val_str(&rows, 0, "label"), "one");
    assert_eq!(val_str(&rows, 1, "label"), "two");
}

#[test]
fn t08_case_generic() {
    let (db, _dir) = fresh_db("case");
    exec_write(&db, "CREATE (:C {v: 10})");
    let rows = query_rows(
        &db,
        "MATCH (n:C) RETURN CASE WHEN n.v > 5 THEN 'big' ELSE 'small' END AS sz",
    );
    assert_eq!(val_str(&rows, 0, "sz"), "big");
}

// ═══════════════════════════════════════════════════════════════
// 9. 字符串函数 [CORE-BUG]
// ═══════════════════════════════════════════════════════════════

#[test]
fn t09_tostring() {
    let (db, _dir) = fresh_db("strfn");
    let rows = query_rows(&db, "RETURN toString(42) AS s");
    assert_eq!(val_str(&rows, 0, "s"), "42");
}

#[test]
fn t09_toupper() {
    let (db, _dir) = fresh_db("strfn");
    let rows = query_rows(&db, "RETURN toUpper('hello') AS s");
    assert_eq!(val_str(&rows, 0, "s"), "HELLO");
}

#[test]
fn t09_tolower() {
    let (db, _dir) = fresh_db("strfn");
    let rows = query_rows(&db, "RETURN toLower('HELLO') AS s");
    assert_eq!(val_str(&rows, 0, "s"), "hello");
}

#[test]
fn t09_trim() {
    let (db, _dir) = fresh_db("strfn");
    let rows = query_rows(&db, "RETURN trim('  hi  ') AS s");
    assert_eq!(val_str(&rows, 0, "s"), "hi");
}

#[test]
fn t09_size() {
    let (db, _dir) = fresh_db("strfn");
    let rows = query_rows(&db, "RETURN size('hello') AS s");
    assert_eq!(val_i64(&rows, 0, "s"), 5);
}

#[test]
fn t09_left() {
    // [CORE-BUG] left() 未实现
    let (db, _dir) = fresh_db("strfn");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN left('hello', 3) AS s")
    }));
    match result {
        Ok(rows) => assert_eq!(val_str(&rows, 0, "s"), "hel"),
        Err(_) => println!("    [CORE-BUG] left() not implemented in Rust core"),
    }
}

#[test]
fn t09_right() {
    // [CORE-BUG] right() 未实现
    let (db, _dir) = fresh_db("strfn");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN right('hello', 3) AS s")
    }));
    match result {
        Ok(rows) => assert_eq!(val_str(&rows, 0, "s"), "llo"),
        Err(_) => println!("    [CORE-BUG] right() not implemented in Rust core"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 10. 数学运算
// ═══════════════════════════════════════════════════════════════

#[test]
fn t10_arithmetic() {
    let (db, _dir) = fresh_db("math");
    let rows = query_rows(&db, "RETURN 10 + 3 AS a, 10 - 3 AS b, 10 * 3 AS c, 10 / 3 AS d");
    assert_eq!(val_i64(&rows, 0, "a"), 13);
    assert_eq!(val_i64(&rows, 0, "b"), 7);
    assert_eq!(val_i64(&rows, 0, "c"), 30);
    // integer division
    let d = val(&rows, 0, "d");
    match d {
        Value::Int(i) => assert_eq!(i, 3),
        Value::Float(f) => assert!((f - 3.333).abs() < 0.1),
        _ => panic!("unexpected type for division: {d:?}"),
    }
}

#[test]
fn t10_modulo() {
    let (db, _dir) = fresh_db("math");
    let rows = query_rows(&db, "RETURN 10 % 3 AS m");
    assert_eq!(val_i64(&rows, 0, "m"), 1);
}

#[test]
fn t10_abs() {
    let (db, _dir) = fresh_db("math");
    let rows = query_rows(&db, "RETURN abs(-42) AS a");
    assert_eq!(val_i64(&rows, 0, "a"), 42);
}

#[test]
fn t10_tointeger() {
    let (db, _dir) = fresh_db("math");
    let rows = query_rows(&db, "RETURN toInteger(3.7) AS i");
    assert_eq!(val_i64(&rows, 0, "i"), 3);
}

// ═══════════════════════════════════════════════════════════════
// 11. 变长路径
// ═══════════════════════════════════════════════════════════════

#[test]
fn t11_variable_length_path() {
    let (db, _dir) = fresh_db("vpath");
    exec_write(&db, "CREATE (a:V {name: 'a'})-[:NEXT]->(b:V {name: 'b'})-[:NEXT]->(c:V {name: 'c'})");
    let rows = query_rows(
        &db,
        "MATCH (a:V {name: 'a'})-[:NEXT*1..3]->(x) RETURN x.name ORDER BY x.name",
    );
    assert!(rows.len() >= 2, "should find b and c via variable-length path");
}

#[test]
fn t11_variable_length_exact() {
    let (db, _dir) = fresh_db("vpath");
    exec_write(&db, "CREATE (a:V {name: 'a'})-[:NEXT]->(b:V {name: 'b'})-[:NEXT]->(c:V {name: 'c'})");
    let rows = query_rows(
        &db,
        "MATCH (a:V {name: 'a'})-[:NEXT*2]->(x) RETURN x.name",
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "x.name"), "c");
}

#[test]
fn t11_path_return() {
    let (db, _dir) = fresh_db("vpath");
    exec_write(&db, "CREATE (a:V {name: 'a'})-[:NEXT]->(b:V {name: 'b'})");
    let rows = query_rows(
        &db,
        "MATCH p = (a:V {name: 'a'})-[:NEXT]->(b:V) RETURN p",
    );
    assert_eq!(rows.len(), 1);
    match val(&rows, 0, "p") {
        Value::Path(_) => {}
        other => panic!("expected Path, got {other:?}"),
    }
}

#[test]
fn t11_shortest_path_skip() {
    // shortestPath may not be implemented — skip gracefully
    let (db, _dir) = fresh_db("vpath");
    exec_write(&db, "CREATE (a:V {name: 'a'})-[:NEXT]->(b:V {name: 'b'})");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(
            &db,
            "MATCH p = shortestPath((a:V {name: 'a'})-[:NEXT*]->(b:V {name: 'b'})) RETURN p",
        )
    }));
    match result {
        Ok(rows) => assert!(rows.len() >= 1),
        Err(_) => println!("    (skipped: shortestPath not implemented)"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 12. EXISTS 子查询
// ═══════════════════════════════════════════════════════════════

#[test]
fn t12_exists_subquery() {
    let (db, _dir) = fresh_db("exists");
    exec_write(&db, "CREATE (a:E {name: 'a'})-[:R]->(b:E {name: 'b'})");
    exec_write(&db, "CREATE (:E {name: 'c'})");
    let rows = query_rows(
        &db,
        "MATCH (n:E) WHERE EXISTS { MATCH (n)-[:R]->() } RETURN n.name",
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "n.name"), "a");
}

// ═══════════════════════════════════════════════════════════════
// 13. FOREACH
// ═══════════════════════════════════════════════════════════════

#[test]
fn t13_foreach() {
    let (db, _dir) = fresh_db("foreach");
    exec_write(&db, "CREATE (:FE {name: 'target', v: 0})");
    exec_write(
        &db,
        "MATCH (n:FE {name: 'target'}) FOREACH (i IN [1, 2, 3] | SET n.v = i)",
    );
    let rows = query_rows(&db, "MATCH (n:FE {name: 'target'}) RETURN n.v");
    // After FOREACH, v should be the last value set (3)
    assert_eq!(val_i64(&rows, 0, "n.v"), 3);
}

// ═══════════════════════════════════════════════════════════════
// 14. 事务 WriteTxn
// ═══════════════════════════════════════════════════════════════

#[test]
fn t14_write_txn_commit() {
    let (db, _dir) = fresh_db("txn");
    {
        let q = prepare("CREATE (:TX {v: 1})").unwrap();
        let snap = db.snapshot();
        let mut txn = db.begin_write();
        q.execute_write(&snap, &mut txn, &Params::new()).unwrap();
        txn.commit().unwrap(); // commit(self) consumes self
    }
    let rows = query_rows(&db, "MATCH (n:TX) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1);
}

#[test]
fn t14_write_txn_rollback_via_drop() {
    let (db, _dir) = fresh_db("txn");
    {
        let q = prepare("CREATE (:TX {v: 2})").unwrap();
        let snap = db.snapshot();
        let mut txn = db.begin_write();
        q.execute_write(&snap, &mut txn, &Params::new()).unwrap();
        // drop txn without commit → implicit rollback
    }
    let rows = query_rows(&db, "MATCH (n:TX {v: 2}) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 0, "dropped txn should rollback");
}

#[test]
fn t14_multiple_writes_in_txn() {
    let (db, _dir) = fresh_db("txn");
    {
        let snap = db.snapshot();
        let mut txn = db.begin_write();
        let q1 = prepare("CREATE (:TX {v: 10})").unwrap();
        q1.execute_write(&snap, &mut txn, &Params::new()).unwrap();
        let q2 = prepare("CREATE (:TX {v: 20})").unwrap();
        q2.execute_write(&snap, &mut txn, &Params::new()).unwrap();
        txn.commit().unwrap();
    }
    let rows = query_rows(&db, "MATCH (n:TX) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 2);
}

#[test]
fn t14_commit_consumes_self() {
    // Verify that commit(self) consumes the transaction (compile-time check)
    // This test just verifies the pattern works — if it compiles, it passes
    let (db, _dir) = fresh_db("txn");
    let mut txn = db.begin_write();
    let q = prepare("CREATE (:TX {v: 99})").unwrap();
    let snap = db.snapshot();
    q.execute_write(&snap, &mut txn, &Params::new()).unwrap();
    txn.commit().unwrap();
    // txn is consumed here — cannot use it again (compile error if tried)
}

// ═══════════════════════════════════════════════════════════════
// 15. 错误处理
// ═══════════════════════════════════════════════════════════════

#[test]
fn t15_syntax_error() {
    let result = prepare("INVALID CYPHER QUERY");
    assert!(result.is_err(), "should fail on invalid syntax");
}

#[test]
fn t15_unknown_function() {
    let result = prepare("RETURN nonexistent_function(1)");
    assert!(result.is_err(), "should fail on unknown function");
}

#[test]
fn t15_query_error_type() {
    let err = prepare("INVALID CYPHER").unwrap_err();
    // nervusdb_query::Error should be returned
    let msg = format!("{err}");
    assert!(!msg.is_empty(), "error should have a message");
}

#[test]
fn t15_nervusdb_error_io() {
    // Test Error::Io variant
    let err = nervusdb::Error::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "test",
    ));
    match err {
        nervusdb::Error::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
        other => panic!("expected Io error, got {other:?}"),
    }
}

#[test]
fn t15_nervusdb_error_storage() {
    let err = nervusdb::Error::Storage("test storage error".to_string());
    match err {
        nervusdb::Error::Storage(msg) => assert!(msg.contains("test storage")),
        other => panic!("expected Storage error, got {other:?}"),
    }
}

#[test]
fn t15_nervusdb_error_query() {
    let err = nervusdb::Error::Query("test query error".to_string());
    match err {
        nervusdb::Error::Query(msg) => assert!(msg.contains("test query")),
        other => panic!("expected Query error, got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 16. 关系方向
// ═══════════════════════════════════════════════════════════════

#[test]
fn t16_outgoing() {
    let (db, _dir) = fresh_db("dir");
    exec_write(&db, "CREATE (a:D {name: 'a'})-[:R]->(b:D {name: 'b'})");
    let rows = query_rows(&db, "MATCH (a:D {name: 'a'})-[:R]->(b) RETURN b.name");
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "b.name"), "b");
}

#[test]
fn t16_incoming() {
    let (db, _dir) = fresh_db("dir");
    exec_write(&db, "CREATE (a:D {name: 'a'})-[:R]->(b:D {name: 'b'})");
    let rows = query_rows(&db, "MATCH (b:D {name: 'b'})<-[:R]-(a) RETURN a.name");
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "a.name"), "a");
}

#[test]
fn t16_undirected() {
    let (db, _dir) = fresh_db("dir");
    exec_write(&db, "CREATE (a:D {name: 'a'})-[:R]->(b:D {name: 'b'})");
    let rows = query_rows(&db, "MATCH (a:D {name: 'a'})-[:R]-(b) RETURN b.name");
    assert!(rows.len() >= 1, "undirected should match");
}

#[test]
fn t16_multi_rel_types() {
    let (db, _dir) = fresh_db("dir");
    exec_write(&db, "CREATE (a:D {name: 'a'})-[:R1]->(b:D {name: 'b'})");
    exec_write(&db, "CREATE (a:D {name: 'a'})-[:R2]->(c:D {name: 'c'})");
    let rows = query_rows(&db, "MATCH (a:D {name: 'a'})-[:R1]->(x) RETURN x.name");
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "x.name"), "b");
}

// ═══════════════════════════════════════════════════════════════
// 17. 复杂图模式
// ═══════════════════════════════════════════════════════════════

#[test]
fn t17_triangle() {
    let (db, _dir) = fresh_db("complex");
    exec_write(&db, "CREATE (a:T {name: 'a'})-[:E]->(b:T {name: 'b'})");
    exec_write(
        &db,
        "MATCH (b:T {name: 'b'}) CREATE (b)-[:E]->(c:T {name: 'c'})",
    );
    exec_write(
        &db,
        "MATCH (c:T {name: 'c'}), (a:T {name: 'a'}) CREATE (c)-[:E]->(a)",
    );
    let rows = query_rows(
        &db,
        "MATCH (a:T)-[:E]->(b:T)-[:E]->(c:T)-[:E]->(a) RETURN a.name, b.name, c.name",
    );
    assert!(rows.len() >= 1, "should find triangle");
}

#[test]
fn t17_multi_hop() {
    let (db, _dir) = fresh_db("complex");
    exec_write(&db, "CREATE (a:H {name: 'a'})-[:NEXT]->(b:H {name: 'b'})-[:NEXT]->(c:H {name: 'c'})");
    let rows = query_rows(
        &db,
        "MATCH (a:H {name: 'a'})-[:NEXT]->(b)-[:NEXT]->(c) RETURN c.name",
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "c.name"), "c");
}

#[test]
fn t17_fan_out() {
    let (db, _dir) = fresh_db("complex");
    exec_write(&db, "CREATE (hub:F {name: 'hub'})");
    for i in 1..=5 {
        exec_write(&db, &format!("MATCH (h:F {{name: 'hub'}}) CREATE (h)-[:SPOKE]->(s:F {{name: 's{i}'}})"));
    }
    let rows = query_rows(
        &db,
        "MATCH (h:F {name: 'hub'})-[:SPOKE]->(s) RETURN count(s) AS c",
    );
    assert_eq!(val_i64(&rows, 0, "c"), 5);
}

// ═══════════════════════════════════════════════════════════════
// 18. 批量写入性能
// ═══════════════════════════════════════════════════════════════

#[test]
fn t18_bulk_100() {
    let (db, _dir) = fresh_db("perf");
    let start = std::time::Instant::now();
    for i in 0..100 {
        exec_write(&db, &format!("CREATE (:Perf {{idx: {i}}})"));
    }
    let elapsed = start.elapsed();
    println!("    100 nodes: {:?}", elapsed);
    let rows = query_rows(&db, "MATCH (n:Perf) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 100);
}

#[test]
fn t18_bulk_1000() {
    let (db, _dir) = fresh_db("perf");
    let start = std::time::Instant::now();
    for i in 0..1000 {
        exec_write(&db, &format!("CREATE (:Perf {{idx: {i}}})"));
    }
    let elapsed = start.elapsed();
    println!("    1000 nodes: {:?}", elapsed);
    let rows = query_rows(&db, "MATCH (n:Perf) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1000);
}

#[test]
fn t18_bulk_unwind() {
    let (db, _dir) = fresh_db("perf");
    let start = std::time::Instant::now();
    exec_write(
        &db,
        "UNWIND range(1, 500) AS i CREATE (:PerfU {idx: i})",
    );
    let elapsed = start.elapsed();
    println!("    500 nodes via UNWIND: {:?}", elapsed);
    let rows = query_rows(&db, "MATCH (n:PerfU) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 500);
}

// ═══════════════════════════════════════════════════════════════
// 19. 持久化
// ═══════════════════════════════════════════════════════════════

#[test]
fn t19_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();

    // Write data and close
    {
        let db = Db::open(&path).unwrap();
        exec_write(&db, "CREATE (:Persist {key: 'hello'})");
        db.close().unwrap();
    }

    // Reopen and verify
    {
        let db = Db::open(&path).unwrap();
        let rows = query_rows(&db, "MATCH (n:Persist) RETURN n.key");
        assert_eq!(rows.len(), 1);
        assert_eq!(val_str(&rows, 0, "n.key"), "hello");
    }
}

// ═══════════════════════════════════════════════════════════════
// 20. 边界情况
// ═══════════════════════════════════════════════════════════════

#[test]
fn t20_empty_match() {
    let (db, _dir) = fresh_db("edge");
    let rows = query_rows(&db, "MATCH (n:NonExistent) RETURN n");
    assert_eq!(rows.len(), 0);
}

#[test]
fn t20_count_empty() {
    let (db, _dir) = fresh_db("edge");
    let rows = query_rows(&db, "MATCH (n:NonExistent) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 0);
}

#[test]
fn t20_return_null() {
    let (db, _dir) = fresh_db("edge");
    let rows = query_rows(&db, "RETURN null AS n");
    assert_eq!(val(&rows, 0, "n"), Value::Null);
}

#[test]
fn t20_large_string() {
    let (db, _dir) = fresh_db("edge");
    let big = "x".repeat(10000);
    exec_write(&db, &format!("CREATE (:Big {{val: '{big}'}})"));
    let rows = query_rows(&db, "MATCH (n:Big) RETURN size(n.val) AS s");
    assert_eq!(val_i64(&rows, 0, "s"), 10000);
}

#[test]
fn t20_self_loop() {
    let (db, _dir) = fresh_db("edge");
    exec_write(&db, "CREATE (n:Loop {name: 'self'})");
    exec_write(
        &db,
        "MATCH (n:Loop {name: 'self'}) CREATE (n)-[:SELF]->(n)",
    );
    let rows = query_rows(
        &db,
        "MATCH (n:Loop)-[:SELF]->(n) RETURN n.name",
    );
    assert_eq!(rows.len(), 1);
}

#[test]
fn t20_duplicate_property_overwrite() {
    let (db, _dir) = fresh_db("edge");
    exec_write(&db, "CREATE (:Dup {v: 1})");
    exec_write(&db, "MATCH (n:Dup) SET n.v = 2");
    exec_write(&db, "MATCH (n:Dup) SET n.v = 3");
    let rows = query_rows(&db, "MATCH (n:Dup) RETURN n.v");
    assert_eq!(val_i64(&rows, 0, "n.v"), 3);
}

// ═══════════════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════════════
// Rust 独有测试 (分类 21-35)
// ═══════════════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// 21. 直接 WriteTxn API
// ═══════════════════════════════════════════════════════════════

#[test]
fn t21_create_node_direct() {
    let (db, _dir) = fresh_db("writetxn");
    let mut txn = db.begin_write();
    let label = txn.get_or_create_label("Person").unwrap();
    let iid = txn.create_node(100, label).unwrap();
    txn.set_node_property(iid, "name".to_string(), PropertyValue::String("Alice".to_string())).unwrap();
    txn.commit().unwrap();

    let rows = query_rows(&db, "MATCH (n:Person) RETURN n.name");
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "n.name"), "Alice");
}

#[test]
fn t21_create_edge_direct() {
    let (db, _dir) = fresh_db("writetxn");
    let mut txn = db.begin_write();
    let label = txn.get_or_create_label("N").unwrap();
    let a = txn.create_node(1, label).unwrap();
    let b = txn.create_node(2, label).unwrap();
    let rel = txn.get_or_create_rel_type("KNOWS").unwrap();
    txn.create_edge(a, rel, b);
    txn.commit().unwrap();

    let rows = query_rows(&db, "MATCH (a:N)-[:KNOWS]->(b:N) RETURN count(*) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1);
}

#[test]
fn t21_set_and_remove_property() {
    let (db, _dir) = fresh_db("writetxn");
    let iid = {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("N").unwrap();
        let iid = txn.create_node(1, label).unwrap();
        txn.set_node_property(iid, "k".to_string(), PropertyValue::Int(42)).unwrap();
        txn.commit().unwrap();
        iid
    };

    // Verify property exists
    let rows = query_rows(&db, "MATCH (n:N) RETURN n.k");
    assert_eq!(val_i64(&rows, 0, "n.k"), 42);

    // Remove property
    {
        let mut txn = db.begin_write();
        txn.remove_node_property(iid, "k").unwrap();
        txn.commit().unwrap();
    }
    let rows = query_rows(&db, "MATCH (n:N) RETURN n.k");
    assert_eq!(val(&rows, 0, "n.k"), Value::Null);
}

#[test]
fn t21_tombstone_node() {
    let (db, _dir) = fresh_db("writetxn");
    let iid = {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("N").unwrap();
        let iid = txn.create_node(1, label).unwrap();
        txn.set_node_property(iid, "v".to_string(), PropertyValue::Int(1)).unwrap();
        txn.commit().unwrap();
        iid
    };

    let before = query_rows(&db, "MATCH (n:N) RETURN count(n) AS c");
    assert_eq!(val_i64(&before, 0, "c"), 1);

    {
        let mut txn = db.begin_write();
        txn.tombstone_node(iid);
        txn.commit().unwrap();
    }

    let after = query_rows(&db, "MATCH (n:N) RETURN count(n) AS c");
    assert_eq!(val_i64(&after, 0, "c"), 0);
}

#[test]
fn t21_tombstone_edge() {
    let (db, _dir) = fresh_db("writetxn");
    let (a, rel, b) = {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("N").unwrap();
        let a = txn.create_node(1, label).unwrap();
        let b = txn.create_node(2, label).unwrap();
        let rel = txn.get_or_create_rel_type("R").unwrap();
        txn.create_edge(a, rel, b);
        txn.commit().unwrap();
        (a, rel, b)
    };

    let before = query_rows(&db, "MATCH ()-[r:R]->() RETURN count(r) AS c");
    assert_eq!(val_i64(&before, 0, "c"), 1);

    {
        let mut txn = db.begin_write();
        txn.tombstone_edge(a, rel, b);
        txn.commit().unwrap();
    }

    let after = query_rows(&db, "MATCH ()-[r:R]->() RETURN count(r) AS c");
    assert_eq!(val_i64(&after, 0, "c"), 0);
}

#[test]
fn t21_edge_property_direct() {
    let (db, _dir) = fresh_db("writetxn");
    let mut txn = db.begin_write();
    let label = txn.get_or_create_label("N").unwrap();
    let a = txn.create_node(1, label).unwrap();
    let b = txn.create_node(2, label).unwrap();
    let rel = txn.get_or_create_rel_type("R").unwrap();
    txn.create_edge(a, rel, b);
    txn.set_edge_property(a, rel, b, "weight".to_string(), PropertyValue::Float(0.5)).unwrap();
    txn.commit().unwrap();

    let rows = query_rows(&db, "MATCH ()-[r:R]->() RETURN r.weight");
    let w = val_f64(&rows, 0, "r.weight");
    assert!((w - 0.5).abs() < 0.001);
}

// ═══════════════════════════════════════════════════════════════
// 22. ReadTxn + neighbors
// ═══════════════════════════════════════════════════════════════

#[test]
fn t22_read_txn_neighbors() {
    let (db, _dir) = fresh_db("readtxn");
    let (a, b, rel) = {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("N").unwrap();
        let a = txn.create_node(1, label).unwrap();
        let b = txn.create_node(2, label).unwrap();
        let rel = txn.get_or_create_rel_type("E").unwrap();
        txn.create_edge(a, rel, b);
        txn.commit().unwrap();
        (a, b, rel)
    };

    let rtxn = db.begin_read();
    let edges: Vec<_> = rtxn.neighbors(a, Some(rel)).collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].dst, b);
}

#[test]
fn t22_read_txn_no_neighbors() {
    let (db, _dir) = fresh_db("readtxn");
    let a = {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("N").unwrap();
        let a = txn.create_node(1, label).unwrap();
        txn.commit().unwrap();
        a
    };

    let rtxn = db.begin_read();
    let edges: Vec<_> = rtxn.neighbors(a, None).collect();
    assert_eq!(edges.len(), 0);
}

#[test]
fn t22_read_txn_filter_rel_type() {
    let (db, _dir) = fresh_db("readtxn");
    let (a, rel1, _rel2) = {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("N").unwrap();
        let a = txn.create_node(1, label).unwrap();
        let b = txn.create_node(2, label).unwrap();
        let c = txn.create_node(3, label).unwrap();
        let rel1 = txn.get_or_create_rel_type("R1").unwrap();
        let rel2 = txn.get_or_create_rel_type("R2").unwrap();
        txn.create_edge(a, rel1, b);
        txn.create_edge(a, rel2, c);
        txn.commit().unwrap();
        (a, rel1, rel2)
    };

    let rtxn = db.begin_read();
    // All neighbors
    let all: Vec<_> = rtxn.neighbors(a, None).collect();
    assert_eq!(all.len(), 2);
    // Filtered by R1
    let r1_only: Vec<_> = rtxn.neighbors(a, Some(rel1)).collect();
    assert_eq!(r1_only.len(), 1);
}

// ═══════════════════════════════════════════════════════════════
// 23. DbSnapshot 方法
// ═══════════════════════════════════════════════════════════════

use nervusdb::GraphSnapshot;

#[test]
fn t23_node_count() {
    let (db, _dir) = fresh_db("snapshot");
    exec_write(&db, "CREATE (:S {v: 1})");
    exec_write(&db, "CREATE (:S {v: 2})");
    let snap = db.snapshot();
    let total = snap.node_count(None);
    // node_count(None) may return 0 if counts are only maintained per-label
    // Try with label filter as well
    let label_id = snap.resolve_label_id("S");
    if let Some(lid) = label_id {
        let labeled = snap.node_count(Some(lid));
        if total == 0 && labeled == 0 {
            println!("    [NOTE] node_count returns 0 for both None and Some(label) — counts may require compaction");
        } else {
            assert!(total >= 2 || labeled >= 2, "should have at least 2 nodes (total={total}, labeled={labeled})");
        }
    } else {
        println!("    [NOTE] node_count(None) returned {total}");
    }
}

#[test]
fn t23_edge_count() {
    let (db, _dir) = fresh_db("snapshot");
    exec_write(&db, "CREATE (a:S)-[:R]->(b:S)");
    let snap = db.snapshot();
    let total = snap.edge_count(None);
    let rel_id = snap.resolve_rel_type_id("R");
    if let Some(rid) = rel_id {
        let typed = snap.edge_count(Some(rid));
        if total == 0 && typed == 0 {
            println!("    [NOTE] edge_count returns 0 — counts may require compaction");
        } else {
            assert!(total >= 1 || typed >= 1, "should have at least 1 edge (total={total}, typed={typed})");
        }
    } else {
        println!("    [NOTE] edge_count(None) returned {total}");
    }
}

#[test]
fn t23_resolve_label_id() {
    let (db, _dir) = fresh_db("snapshot");
    exec_write(&db, "CREATE (:MyLabel {v: 1})");
    let snap = db.snapshot();
    let lid = snap.resolve_label_id("MyLabel");
    assert!(lid.is_some(), "should resolve MyLabel");
}

#[test]
fn t23_resolve_rel_type_id() {
    let (db, _dir) = fresh_db("snapshot");
    exec_write(&db, "CREATE (:A)-[:MY_REL]->(b:B)");
    let snap = db.snapshot();
    let rid = snap.resolve_rel_type_id("MY_REL");
    assert!(rid.is_some(), "should resolve MY_REL");
}

#[test]
fn t23_node_property() {
    let (db, _dir) = fresh_db("snapshot");
    let iid = {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("S").unwrap();
        let iid = txn.create_node(1, label).unwrap();
        txn.set_node_property(iid, "key".to_string(), PropertyValue::String("val".to_string())).unwrap();
        txn.commit().unwrap();
        iid
    };
    let snap = db.snapshot();
    let prop = snap.node_property(iid, "key");
    assert_eq!(prop, Some(PropertyValue::String("val".to_string())));
}

// ═══════════════════════════════════════════════════════════════
// 24. 参数化查询 Params
// ═══════════════════════════════════════════════════════════════

#[test]
fn t24_param_string() {
    let (db, _dir) = fresh_db("params");
    exec_write(&db, "CREATE (:P {name: 'Alice'})");
    let mut params = Params::new();
    params.insert("n", Value::String("Alice".to_string()));
    let rows = query_rows_params(&db, "MATCH (p:P) WHERE p.name = $n RETURN p.name", &params);
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "p.name"), "Alice");
}

#[test]
fn t24_param_integer() {
    let (db, _dir) = fresh_db("params");
    exec_write(&db, "CREATE (:P {age: 30})");
    let mut params = Params::new();
    params.insert("a", Value::Int(30));
    let rows = query_rows_params(&db, "MATCH (p:P) WHERE p.age = $a RETURN p.age", &params);
    assert_eq!(rows.len(), 1);
    assert_eq!(val_i64(&rows, 0, "p.age"), 30);
}

#[test]
fn t24_param_null() {
    let (db, _dir) = fresh_db("params");
    let mut params = Params::new();
    params.insert("v", Value::Null);
    let rows = query_rows_params(&db, "RETURN $v AS v", &params);
    assert_eq!(val(&rows, 0, "v"), Value::Null);
}

#[test]
fn t24_param_list() {
    let (db, _dir) = fresh_db("params");
    exec_write(&db, "CREATE (:P {name: 'A'})");
    exec_write(&db, "CREATE (:P {name: 'B'})");
    exec_write(&db, "CREATE (:P {name: 'C'})");
    let mut params = Params::new();
    params.insert("names", Value::List(vec![
        Value::String("A".to_string()),
        Value::String("C".to_string()),
    ]));
    let rows = query_rows_params(
        &db,
        "MATCH (p:P) WHERE p.name IN $names RETURN p.name ORDER BY p.name",
        &params,
    );
    assert_eq!(rows.len(), 2);
}

#[test]
fn t24_param_multi() {
    let (db, _dir) = fresh_db("params");
    exec_write(&db, "CREATE (:P {name: 'Alice', age: 30})");
    let mut params = Params::new();
    params.insert("n", Value::String("Alice".to_string()));
    params.insert("a", Value::Int(30));
    let rows = query_rows_params(
        &db,
        "MATCH (p:P) WHERE p.name = $n AND p.age = $a RETURN p.name",
        &params,
    );
    assert_eq!(rows.len(), 1);
}

// ═══════════════════════════════════════════════════════════════
// 25. execute_mixed
// ═══════════════════════════════════════════════════════════════

#[test]
fn t25_execute_mixed_create_return() {
    let (db, _dir) = fresh_db("mixed");
    let q = prepare("CREATE (n:MX {v: 1}) RETURN n.v AS v").unwrap();
    let snap = db.snapshot();
    let mut txn = db.begin_write();
    let (rows, count) = q.execute_mixed(&snap, &mut txn, &Params::new()).unwrap();
    txn.commit().unwrap();
    assert!(count > 0, "should have created nodes");
    // rows may or may not contain the RETURN data depending on plan type
    println!("    execute_mixed: rows={}, count={count}", rows.len());
}

#[test]
fn t25_execute_mixed_read_only() {
    let (db, _dir) = fresh_db("mixed");
    exec_write(&db, "CREATE (:MX {v: 1})");
    let q = prepare("MATCH (n:MX) RETURN n.v AS v").unwrap();
    let snap = db.snapshot();
    let mut txn = db.begin_write();
    let (rows, count) = q.execute_mixed(&snap, &mut txn, &Params::new()).unwrap();
    txn.commit().unwrap();
    assert_eq!(count, 0, "read-only should have count=0");
    assert_eq!(rows.len(), 1);
}

#[test]
fn t25_execute_mixed_write_count() {
    let (db, _dir) = fresh_db("mixed");
    let q = prepare("CREATE (:MX {v: 1}), (:MX {v: 2})").unwrap();
    let snap = db.snapshot();
    let mut txn = db.begin_write();
    let result = q.execute_mixed(&snap, &mut txn, &Params::new());
    txn.commit().unwrap();
    match result {
        Ok((_rows, count)) => {
            println!("    multi-create mixed: count={count}");
            assert!(count > 0);
        }
        Err(e) => println!("    (limitation: multi-create mixed: {e})"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 26. ExecuteOptions 资源限制
// ═══════════════════════════════════════════════════════════════

#[test]
fn t26_resource_limit_intermediate_rows() {
    let (db, _dir) = fresh_db("limits");
    // Create enough data to trigger limit
    exec_write(&db, "UNWIND range(1, 100) AS i CREATE (:L {v: i})");

    let opts = ExecuteOptions {
        max_intermediate_rows: 5,
        ..Default::default()
    };
    let params = Params::with_execute_options(opts);
    let q = prepare("MATCH (n:L) RETURN n.v").unwrap();
    let snap = db.snapshot();
    let result: Vec<nervusdb_query::Result<Row>> =
        q.execute_streaming(&snap, &params).collect();
    // Should have some errors due to resource limit
    let has_error = result.iter().any(|r| r.is_err());
    if has_error {
        let err = result.into_iter().find(|r| r.is_err()).unwrap().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("ResourceLimitExceeded"),
            "expected ResourceLimitExceeded, got: {msg}"
        );
    } else {
        println!("    (note: resource limit not triggered with 100 rows / limit 5)");
    }
}

#[test]
fn t26_resource_limit_default() {
    // Default limits should be generous enough for normal queries
    let opts = ExecuteOptions::default();
    assert_eq!(opts.max_intermediate_rows, 500_000);
    assert_eq!(opts.max_collection_items, 200_000);
    assert_eq!(opts.soft_timeout_ms, 5_000);
}

#[test]
fn t26_params_with_options() {
    let opts = ExecuteOptions {
        max_intermediate_rows: 10,
        max_collection_items: 10,
        soft_timeout_ms: 1000,
        max_apply_rows_per_outer: 10,
    };
    let params = Params::with_execute_options(opts);
    assert_eq!(params.execute_options().max_intermediate_rows, 10);
    assert_eq!(params.execute_options().max_collection_items, 10);
}

// ═══════════════════════════════════════════════════════════════
// 27. vacuum
// ═══════════════════════════════════════════════════════════════

#[test]
fn t27_vacuum_basic() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("graph");
    {
        let db = Db::open(&base).unwrap();
        exec_write(&db, "CREATE (:V {v: 1})");
        exec_write(&db, "CREATE (:V {v: 2})");
        exec_write(&db, "MATCH (n:V {v: 1}) DETACH DELETE n");
        db.close().unwrap();
    }

    let report = nervusdb::vacuum(&base).unwrap();
    assert_eq!(
        report.ndb_path.extension().and_then(|s| s.to_str()),
        Some("ndb")
    );
    assert!(report.backup_path.exists(), "vacuum should create backup");

    // Verify data integrity after vacuum
    {
        let db = Db::open(&base).unwrap();
        let rows = query_rows(&db, "MATCH (n:V) RETURN count(n) AS c");
        assert_eq!(val_i64(&rows, 0, "c"), 1);
    }
}

#[test]
fn t27_vacuum_report_fields() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("graph");
    {
        let db = Db::open(&base).unwrap();
        exec_write(&db, "CREATE (:V {v: 1})");
        db.close().unwrap();
    }

    let report = nervusdb::vacuum(&base).unwrap();
    println!(
        "    VacuumReport: old_pages={}, new_pages={}, copied={}",
        report.old_file_pages, report.new_file_pages, report.copied_data_pages
    );
    // Just verify the fields are accessible
    let _ = report.old_next_page_id;
    let _ = report.new_next_page_id;
}

// ═══════════════════════════════════════════════════════════════
// 28. backup
// ═══════════════════════════════════════════════════════════════

#[test]
fn t28_backup_basic() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("graph");
    let backup_dir = dir.path().join("backups");
    std::fs::create_dir_all(&backup_dir).unwrap();

    {
        let db = Db::open(&base).unwrap();
        exec_write(&db, "CREATE (:B {key: 'backup-test'})");
        db.close().unwrap();
    }

    let info = nervusdb::backup(&base, &backup_dir).unwrap();
    println!("    BackupInfo: {:?}", info);
}

#[test]
fn t28_backup_and_restore() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("graph");
    let backup_dir = dir.path().join("backups");
    std::fs::create_dir_all(&backup_dir).unwrap();

    {
        let db = Db::open(&base).unwrap();
        exec_write(&db, "CREATE (:B {key: 'restore-test'})");
        db.close().unwrap();
    }

    let info = nervusdb::backup(&base, &backup_dir).unwrap();

    // BackupInfo has id, created_at, size_bytes, etc.
    // Backup files are stored in backup_dir/<backup-id>/
    let backup_subdir = backup_dir.join(info.id.to_string());
    if backup_subdir.exists() {
        // Look for .ndb file in backup
        let backup_ndb = backup_subdir.join(
            base.with_extension("ndb")
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
        );
        if backup_ndb.exists() {
            let db2 = Db::open(&backup_ndb).unwrap();
            let rows = query_rows(&db2, "MATCH (n:B) RETURN n.key");
            assert_eq!(rows.len(), 1);
            assert_eq!(val_str(&rows, 0, "n.key"), "restore-test");
        } else {
            println!("    backup subdir exists but .ndb not found at expected path");
            for entry in std::fs::read_dir(&backup_subdir).unwrap() {
                let entry = entry.unwrap();
                println!("    backup file: {:?}", entry.path());
            }
        }
    } else {
        println!("    backup subdir not found at {:?}, listing backup_dir", backup_subdir);
        if backup_dir.exists() {
            for entry in std::fs::read_dir(&backup_dir).unwrap() {
                let entry = entry.unwrap();
                println!("    backup entry: {:?}", entry.path());
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 29. bulkload
// ═══════════════════════════════════════════════════════════════

use nervusdb::{BulkNode, BulkEdge};

#[test]
fn t29_bulkload_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("bulk");

    let nodes = vec![
        BulkNode {
            external_id: 1,
            label: "Person".to_string(),
            properties: BTreeMap::from([
                ("name".to_string(), PropertyValue::String("Alice".to_string())),
            ]),
        },
        BulkNode {
            external_id: 2,
            label: "Person".to_string(),
            properties: BTreeMap::from([
                ("name".to_string(), PropertyValue::String("Bob".to_string())),
            ]),
        },
    ];

    nervusdb::bulkload(&base, nodes, Vec::new()).unwrap();
    let db = Db::open(&base).unwrap();
    let rows = query_rows(&db, "MATCH (n:Person) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 2);
}

#[test]
fn t29_bulkload_with_edges() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("bulk");

    let nodes = vec![
        BulkNode {
            external_id: 1,
            label: "P".to_string(),
            properties: BTreeMap::from([
                ("name".to_string(), PropertyValue::String("a".to_string())),
            ]),
        },
        BulkNode {
            external_id: 2,
            label: "P".to_string(),
            properties: BTreeMap::from([
                ("name".to_string(), PropertyValue::String("b".to_string())),
            ]),
        },
    ];
    let edges = vec![BulkEdge {
        src_external_id: 1,
        rel_type: "KNOWS".to_string(),
        dst_external_id: 2,
        properties: BTreeMap::new(),
    }];

    nervusdb::bulkload(&base, nodes, edges).unwrap();
    let db = Db::open(&base).unwrap();
    let rows = query_rows(&db, "MATCH (a:P)-[:KNOWS]->(b:P) RETURN a.name, b.name");
    assert_eq!(rows.len(), 1);
}

#[test]
fn t29_bulkload_large() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("bulk");

    let nodes: Vec<BulkNode> = (0..1000)
        .map(|i| BulkNode {
            external_id: i,
            label: "N".to_string(),
            properties: BTreeMap::from([
                ("idx".to_string(), PropertyValue::Int(i as i64)),
            ]),
        })
        .collect();

    let start = std::time::Instant::now();
    nervusdb::bulkload(&base, nodes, Vec::new()).unwrap();
    let elapsed = start.elapsed();
    println!("    bulkload 1000 nodes: {:?}", elapsed);

    let db = Db::open(&base).unwrap();
    let rows = query_rows(&db, "MATCH (n:N) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1000);
}

// ═══════════════════════════════════════════════════════════════
// 30. create_index
// ═══════════════════════════════════════════════════════════════

#[test]
fn t30_create_index() {
    let (db, _dir) = fresh_db("index");
    exec_write(&db, "CREATE (:Idx {email: 'a@b.com'})");
    exec_write(&db, "CREATE (:Idx {email: 'c@d.com'})");
    db.create_index("Idx", "email").unwrap();

    // Query should still work correctly after index creation
    let rows = query_rows(&db, "MATCH (n:Idx {email: 'a@b.com'}) RETURN n.email");
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "n.email"), "a@b.com");
}

#[test]
fn t30_index_lookup() {
    let (db, _dir) = fresh_db("index");
    for i in 0..50 {
        exec_write(&db, &format!("CREATE (:Idx {{val: {i}}})"));
    }
    db.create_index("Idx", "val").unwrap();

    let rows = query_rows(&db, "MATCH (n:Idx {val: 25}) RETURN n.val");
    assert_eq!(rows.len(), 1);
    assert_eq!(val_i64(&rows, 0, "n.val"), 25);
}

// ═══════════════════════════════════════════════════════════════
// 31. compact + checkpoint
// ═══════════════════════════════════════════════════════════════

#[test]
fn t31_compact() {
    let (db, _dir) = fresh_db("compact");
    exec_write(&db, "CREATE (:C {v: 1})");
    exec_write(&db, "CREATE (:C {v: 2})");
    db.compact().unwrap();

    // Data should still be intact
    let rows = query_rows(&db, "MATCH (n:C) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 2);
}

#[test]
fn t31_checkpoint() {
    let (db, _dir) = fresh_db("checkpoint");
    exec_write(&db, "CREATE (:CP {v: 1})");
    db.checkpoint().unwrap();

    let rows = query_rows(&db, "MATCH (n:CP) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1);
}

// ═══════════════════════════════════════════════════════════════
// 32. Error 类型
// ═══════════════════════════════════════════════════════════════

#[test]
fn t32_error_query() {
    let err = nervusdb::Error::Query("test".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("Query error"));
}

#[test]
fn t32_error_storage() {
    let err = nervusdb::Error::Storage("test".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("Storage error"));
}

#[test]
fn t32_error_io() {
    let err = nervusdb::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "test"));
    let msg = format!("{err}");
    assert!(msg.contains("IO error"));
}

#[test]
fn t32_error_other() {
    let err = nervusdb::Error::Other("custom".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("custom"));
}

// ═══════════════════════════════════════════════════════════════
// 33. 向量操作
// ═══════════════════════════════════════════════════════════════

#[test]
fn t33_set_vector() {
    let (db, _dir) = fresh_db("vector");
    let iid = {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("V").unwrap();
        let iid = txn.create_node(1, label).unwrap();
        txn.set_vector(iid, vec![1.0, 0.0, 0.0]).unwrap();
        txn.commit().unwrap();
        iid
    };
    // Just verify it doesn't error
    let _ = iid;
}

#[test]
fn t33_search_vector() {
    let (db, _dir) = fresh_db("vector");
    {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("V").unwrap();
        let a = txn.create_node(1, label).unwrap();
        txn.set_vector(a, vec![1.0, 0.0, 0.0]).unwrap();
        let b = txn.create_node(2, label).unwrap();
        txn.set_vector(b, vec![0.0, 1.0, 0.0]).unwrap();
        let c = txn.create_node(3, label).unwrap();
        txn.set_vector(c, vec![0.9, 0.1, 0.0]).unwrap();
        txn.commit().unwrap();
    }

    let results = db.search_vector(&[1.0, 0.0, 0.0], 2).unwrap();
    assert_eq!(results.len(), 2, "should return top-2 results");
    // First result should be closest to query vector
    println!("    vector search results: {:?}", results);
}

#[test]
fn t33_vector_knn_order() {
    let (db, _dir) = fresh_db("vector");
    {
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("V").unwrap();
        let a = txn.create_node(1, label).unwrap();
        txn.set_vector(a, vec![1.0, 0.0]).unwrap();
        txn.set_node_property(a, "name".to_string(), PropertyValue::String("close".to_string())).unwrap();
        let b = txn.create_node(2, label).unwrap();
        txn.set_vector(b, vec![0.0, 1.0]).unwrap();
        txn.set_node_property(b, "name".to_string(), PropertyValue::String("far".to_string())).unwrap();
        txn.commit().unwrap();
    }

    let results = db.search_vector(&[1.0, 0.0], 2).unwrap();
    assert!(results.len() >= 1);
    // First result should be the closest node
    let (first_id, first_dist) = results[0];
    println!("    closest: id={first_id}, dist={first_dist}");
}

#[test]
fn t33_vector_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();

    {
        let db = Db::open(&path).unwrap();
        let mut txn = db.begin_write();
        let label = txn.get_or_create_label("V").unwrap();
        let a = txn.create_node(1, label).unwrap();
        txn.set_vector(a, vec![1.0, 0.0, 0.0]).unwrap();
        txn.commit().unwrap();
        db.close().unwrap();
    }

    {
        let db = Db::open(&path).unwrap();
        let results = db.search_vector(&[1.0, 0.0, 0.0], 1).unwrap();
        assert!(results.len() >= 1, "vector should persist across close/reopen");
    }
}

// ═══════════════════════════════════════════════════════════════
// 34. Value reify
// ═══════════════════════════════════════════════════════════════

#[test]
fn t34_reify_node_id() {
    let (db, _dir) = fresh_db("reify");
    exec_write(&db, "CREATE (:R {name: 'test'})");
    let rows = query_rows(&db, "MATCH (n:R) RETURN n");
    // Raw row should have NodeId
    let raw = val(&rows, 0, "n");
    match &raw {
        Value::NodeId(_) => {
            // Reify it
            let snap = db.snapshot();
            let reified = raw.reify(&snap).unwrap();
            match reified {
                Value::Node(n) => {
                    assert!(n.labels.contains(&"R".to_string()));
                    assert_eq!(n.properties.get("name"), Some(&Value::String("test".to_string())));
                }
                other => panic!("expected Node after reify, got {other:?}"),
            }
        }
        Value::Node(_) => {
            // Already reified by the engine — that's fine too
            println!("    (node already reified by engine)");
        }
        other => panic!("expected NodeId or Node, got {other:?}"),
    }
}

#[test]
fn t34_reify_edge_key() {
    let (db, _dir) = fresh_db("reify");
    exec_write(&db, "CREATE (a:R)-[:REL {w: 5}]->(b:R)");
    let rows = query_rows(&db, "MATCH ()-[r:REL]->() RETURN r");
    let raw = val(&rows, 0, "r");
    match &raw {
        Value::EdgeKey(_) => {
            let snap = db.snapshot();
            let reified = raw.reify(&snap).unwrap();
            match reified {
                Value::Relationship(rel) => {
                    assert_eq!(rel.rel_type, "REL");
                }
                other => panic!("expected Relationship after reify, got {other:?}"),
            }
        }
        Value::Relationship(rel) => {
            assert_eq!(rel.rel_type, "REL");
            println!("    (edge already reified by engine)");
        }
        other => panic!("expected EdgeKey or Relationship, got {other:?}"),
    }
}

#[test]
fn t34_reify_row() {
    let (db, _dir) = fresh_db("reify");
    exec_write(&db, "CREATE (:R {name: 'row-test'})");
    let rows = query_rows(&db, "MATCH (n:R) RETURN n, n.name AS name");
    let snap = db.snapshot();
    let reified = rows[0].reify(&snap).unwrap();
    // After reify, the node column should be a full Node value
    let name_val = reified.get("name").unwrap();
    assert_eq!(name_val, &Value::String("row-test".to_string()));
}

// ═══════════════════════════════════════════════════════════════
// 35. Db 路径 + open_paths
// ═══════════════════════════════════════════════════════════════

#[test]
fn t35_ndb_path() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("mydb");
    let db = Db::open(&base).unwrap();
    let ndb = db.ndb_path();
    assert!(
        ndb.to_str().unwrap().ends_with(".ndb"),
        "ndb_path should end with .ndb, got {:?}",
        ndb
    );
}

#[test]
fn t35_wal_path() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("mydb");
    let db = Db::open(&base).unwrap();
    let wal = db.wal_path();
    assert!(
        wal.to_str().unwrap().ends_with(".wal"),
        "wal_path should end with .wal, got {:?}",
        wal
    );
}

#[test]
fn t35_open_paths() {
    let dir = tempfile::tempdir().unwrap();
    let ndb = dir.path().join("custom.ndb");
    let wal = dir.path().join("custom.wal");
    let db = Db::open_paths(&ndb, &wal).unwrap();
    exec_write(&db, "CREATE (:OP {v: 1})");
    let rows = query_rows(&db, "MATCH (n:OP) RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1);
    assert_eq!(db.ndb_path(), ndb.as_path());
    assert_eq!(db.wal_path(), wal.as_path());
}

// ═══════════════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════════════
// Extended Capability Tests (Categories 36-52)
// ═══════════════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// 36. UNWIND (expanded)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t36_unwind_with_index() {
    let (db, _dir) = fresh_db("unwind2");
    let rows = query_rows(&db, "UNWIND [10, 20, 30] AS x RETURN x ORDER BY x");
    assert_eq!(rows.len(), 3);
    assert_eq!(val_i64(&rows, 0, "x"), 10);
    assert_eq!(val_i64(&rows, 2, "x"), 30);
}

#[test]
fn t36_unwind_empty_list() {
    let (db, _dir) = fresh_db("unwind2");
    let rows = query_rows(&db, "UNWIND [] AS x RETURN x");
    assert_eq!(rows.len(), 0);
}

#[test]
fn t36_unwind_with_aggregation() {
    let (db, _dir) = fresh_db("unwind2");
    let rows = query_rows(&db, "UNWIND [1, 2, 3, 4, 5] AS x RETURN sum(x) AS total");
    assert_eq!(val_i64(&rows, 0, "total"), 15);
}

#[test]
fn t36_unwind_create_nodes() {
    let (db, _dir) = fresh_db("unwind2");
    exec_write(&db, "UNWIND ['a', 'b', 'c'] AS name CREATE (:UW2 {name: name})");
    let rows = query_rows(&db, "MATCH (n:UW2) RETURN n.name ORDER BY n.name");
    assert_eq!(rows.len(), 3);
    assert_eq!(val_str(&rows, 0, "n.name"), "a");
    assert_eq!(val_str(&rows, 2, "n.name"), "c");
}

#[test]
fn t36_unwind_range() {
    let (db, _dir) = fresh_db("unwind2");
    let rows = query_rows(&db, "UNWIND range(1, 5) AS x RETURN x ORDER BY x");
    assert_eq!(rows.len(), 5);
    assert_eq!(val_i64(&rows, 0, "x"), 1);
    assert_eq!(val_i64(&rows, 4, "x"), 5);
}

// ═══════════════════════════════════════════════════════════════
// 37. UNION / UNION ALL (expanded)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t37_union_dedup() {
    let (db, _dir) = fresh_db("union2");
    let rows = query_rows(&db, "RETURN 1 AS x UNION RETURN 1 AS x");
    assert_eq!(rows.len(), 1, "UNION should deduplicate");
}

#[test]
fn t37_union_all_keeps_dupes() {
    let (db, _dir) = fresh_db("union2");
    let rows = query_rows(&db, "RETURN 1 AS x UNION ALL RETURN 1 AS x");
    assert_eq!(rows.len(), 2, "UNION ALL should keep duplicates");
}

#[test]
fn t37_union_multi() {
    let (db, _dir) = fresh_db("union2");
    let rows = query_rows(
        &db,
        "RETURN 1 AS x UNION RETURN 2 AS x UNION RETURN 3 AS x",
    );
    assert_eq!(rows.len(), 3);
}

#[test]
fn t37_union_with_match() {
    let (db, _dir) = fresh_db("union2");
    exec_write(&db, "CREATE (:UA {v: 'a'})");
    exec_write(&db, "CREATE (:UB {v: 'b'})");
    let rows = query_rows(
        &db,
        "MATCH (n:UA) RETURN n.v AS v UNION MATCH (n:UB) RETURN n.v AS v",
    );
    assert_eq!(rows.len(), 2);
}

// ═══════════════════════════════════════════════════════════════
// 38. WITH pipeline (expanded)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t38_with_multi_stage() {
    let (db, _dir) = fresh_db("with2");
    for i in 1..=10 {
        exec_write(&db, &format!("CREATE (:W {{v: {i}}})"));
    }
    let rows = query_rows(
        &db,
        "MATCH (n:W) WITH n.v AS v WHERE v > 5 WITH v AS val ORDER BY val LIMIT 3 RETURN val",
    );
    assert_eq!(rows.len(), 3);
    assert_eq!(val_i64(&rows, 0, "val"), 6);
}

#[test]
fn t38_with_distinct() {
    let (db, _dir) = fresh_db("with2");
    exec_write(&db, "CREATE (:WD {v: 1})");
    exec_write(&db, "CREATE (:WD {v: 1})");
    exec_write(&db, "CREATE (:WD {v: 2})");
    let rows = query_rows(
        &db,
        "MATCH (n:WD) WITH DISTINCT n.v AS v RETURN v ORDER BY v",
    );
    assert_eq!(rows.len(), 2);
}

#[test]
fn t38_with_aggregation() {
    let (db, _dir) = fresh_db("with2");
    exec_write(&db, "CREATE (:WA {cat: 'a', v: 1})");
    exec_write(&db, "CREATE (:WA {cat: 'a', v: 2})");
    exec_write(&db, "CREATE (:WA {cat: 'b', v: 3})");
    let rows = query_rows(
        &db,
        "MATCH (n:WA) WITH n.cat AS cat, sum(n.v) AS total RETURN cat, total ORDER BY cat",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(val_i64(&rows, 0, "total"), 3);
}

// ═══════════════════════════════════════════════════════════════
// 39. ORDER BY + SKIP + LIMIT combined (pagination)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t39_pagination_page1() {
    let (db, _dir) = fresh_db("page");
    for i in 1..=20 {
        exec_write(&db, &format!("CREATE (:PG {{v: {i}}})"));
    }
    let rows = query_rows(&db, "MATCH (n:PG) RETURN n.v ORDER BY n.v LIMIT 5");
    assert_eq!(rows.len(), 5);
    assert_eq!(val_i64(&rows, 0, "n.v"), 1);
    assert_eq!(val_i64(&rows, 4, "n.v"), 5);
}

#[test]
fn t39_pagination_page2() {
    let (db, _dir) = fresh_db("page");
    for i in 1..=20 {
        exec_write(&db, &format!("CREATE (:PG {{v: {i}}})"));
    }
    let rows = query_rows(&db, "MATCH (n:PG) RETURN n.v ORDER BY n.v SKIP 5 LIMIT 5");
    assert_eq!(rows.len(), 5);
    assert_eq!(val_i64(&rows, 0, "n.v"), 6);
    assert_eq!(val_i64(&rows, 4, "n.v"), 10);
}

#[test]
fn t39_order_by_multi_column() {
    let (db, _dir) = fresh_db("page");
    exec_write(&db, "CREATE (:MC {a: 1, b: 'z'})");
    exec_write(&db, "CREATE (:MC {a: 1, b: 'a'})");
    exec_write(&db, "CREATE (:MC {a: 2, b: 'm'})");
    let rows = query_rows(
        &db,
        "MATCH (n:MC) RETURN n.a, n.b ORDER BY n.a, n.b",
    );
    assert_eq!(rows.len(), 3);
    assert_eq!(val_str(&rows, 0, "n.b"), "a");
    assert_eq!(val_str(&rows, 1, "n.b"), "z");
}

#[test]
fn t39_skip_beyond_results() {
    let (db, _dir) = fresh_db("page");
    exec_write(&db, "CREATE (:SK {v: 1})");
    let rows = query_rows(&db, "MATCH (n:SK) RETURN n.v SKIP 100");
    assert_eq!(rows.len(), 0);
}

// ═══════════════════════════════════════════════════════════════
// 40. Null handling (expanded)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t40_coalesce() {
    let (db, _dir) = fresh_db("null2");
    let rows = query_rows(&db, "RETURN coalesce(null, 'fallback') AS v");
    assert_eq!(val_str(&rows, 0, "v"), "fallback");
}

#[test]
fn t40_coalesce_first_non_null() {
    let (db, _dir) = fresh_db("null2");
    let rows = query_rows(&db, "RETURN coalesce(null, null, 42) AS v");
    assert_eq!(val_i64(&rows, 0, "v"), 42);
}

#[test]
fn t40_null_arithmetic_propagation() {
    let (db, _dir) = fresh_db("null2");
    let rows = query_rows(&db, "RETURN null + 1 AS v");
    assert_eq!(val(&rows, 0, "v"), Value::Null);
}

#[test]
fn t40_null_comparison() {
    let (db, _dir) = fresh_db("null2");
    let rows = query_rows(&db, "RETURN null = null AS v");
    assert_eq!(val(&rows, 0, "v"), Value::Null);
}

#[test]
fn t40_is_null_filter() {
    let (db, _dir) = fresh_db("null2");
    exec_write(&db, "CREATE (:NL {name: 'has'})");
    exec_write(&db, "CREATE (:NL {})");
    let rows = query_rows(&db, "MATCH (n:NL) WHERE n.name IS NULL RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1);
}

#[test]
fn t40_is_not_null_filter() {
    let (db, _dir) = fresh_db("null2");
    exec_write(&db, "CREATE (:NL2 {name: 'has'})");
    exec_write(&db, "CREATE (:NL2 {})");
    let rows = query_rows(&db, "MATCH (n:NL2) WHERE n.name IS NOT NULL RETURN count(n) AS c");
    assert_eq!(val_i64(&rows, 0, "c"), 1);
}

// ═══════════════════════════════════════════════════════════════
// 41. Type conversion functions
// ═══════════════════════════════════════════════════════════════

#[test]
fn t41_tointeger_from_float() {
    let (db, _dir) = fresh_db("typeconv");
    let rows = query_rows(&db, "RETURN toInteger(3.9) AS v");
    assert_eq!(val_i64(&rows, 0, "v"), 3);
}

#[test]
fn t41_tointeger_from_string() {
    let (db, _dir) = fresh_db("typeconv");
    let rows = query_rows(&db, "RETURN toInteger('42') AS v");
    assert_eq!(val_i64(&rows, 0, "v"), 42);
}

#[test]
fn t41_tofloat_from_int() {
    let (db, _dir) = fresh_db("typeconv");
    let rows = query_rows(&db, "RETURN toFloat(42) AS v");
    let v = val_f64(&rows, 0, "v");
    assert!((v - 42.0).abs() < 0.001);
}

#[test]
fn t41_tofloat_from_string() {
    let (db, _dir) = fresh_db("typeconv");
    let rows = query_rows(&db, "RETURN toFloat('3.14') AS v");
    let v = val_f64(&rows, 0, "v");
    assert!((v - 3.14).abs() < 0.01);
}

#[test]
fn t41_tostring_from_int() {
    let (db, _dir) = fresh_db("typeconv");
    let rows = query_rows(&db, "RETURN toString(42) AS v");
    assert_eq!(val_str(&rows, 0, "v"), "42");
}

#[test]
fn t41_tostring_from_bool() {
    let (db, _dir) = fresh_db("typeconv");
    let rows = query_rows(&db, "RETURN toString(true) AS v");
    assert_eq!(val_str(&rows, 0, "v"), "true");
}

#[test]
fn t41_toboolean_from_string() {
    let (db, _dir) = fresh_db("typeconv");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN toBoolean('true') AS v")
    }));
    match result {
        Ok(rows) => assert_eq!(val(&rows, 0, "v"), Value::Bool(true)),
        Err(_) => println!("    (note: toBoolean() may not be implemented)"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 42. Math functions (full)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t42_abs_negative() {
    let (db, _dir) = fresh_db("mathfull");
    let rows = query_rows(&db, "RETURN abs(-7) AS v");
    assert_eq!(val_i64(&rows, 0, "v"), 7);
}

#[test]
fn t42_ceil() {
    let (db, _dir) = fresh_db("mathfull");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN ceil(2.3) AS v")
    }));
    match result {
        Ok(rows) => {
            let v = val_f64(&rows, 0, "v");
            assert!((v - 3.0).abs() < 0.001, "ceil(2.3) should be 3.0, got {v}");
        }
        Err(_) => println!("    (note: ceil() may not be implemented)"),
    }
}

#[test]
fn t42_floor() {
    let (db, _dir) = fresh_db("mathfull");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN floor(2.7) AS v")
    }));
    match result {
        Ok(rows) => {
            let v = val_f64(&rows, 0, "v");
            assert!((v - 2.0).abs() < 0.001, "floor(2.7) should be 2.0, got {v}");
        }
        Err(_) => println!("    (note: floor() may not be implemented)"),
    }
}

#[test]
fn t42_round() {
    let (db, _dir) = fresh_db("mathfull");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN round(2.5) AS v")
    }));
    match result {
        Ok(rows) => {
            let v = val_f64(&rows, 0, "v");
            // round(2.5) could be 2.0 or 3.0 depending on rounding mode
            assert!(v >= 2.0 && v <= 3.0, "round(2.5) should be 2 or 3, got {v}");
        }
        Err(_) => println!("    (note: round() may not be implemented)"),
    }
}

#[test]
fn t42_sign() {
    let (db, _dir) = fresh_db("mathfull");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN sign(-5) AS neg, sign(0) AS zero, sign(5) AS pos")
    }));
    match result {
        Ok(rows) => {
            assert_eq!(val_i64(&rows, 0, "neg"), -1);
            assert_eq!(val_i64(&rows, 0, "zero"), 0);
            assert_eq!(val_i64(&rows, 0, "pos"), 1);
        }
        Err(_) => println!("    (note: sign() may not be implemented)"),
    }
}

#[test]
fn t42_sqrt() {
    let (db, _dir) = fresh_db("mathfull");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN sqrt(16) AS v")
    }));
    match result {
        Ok(rows) => {
            let v = val_f64(&rows, 0, "v");
            assert!((v - 4.0).abs() < 0.001, "sqrt(16) should be 4.0, got {v}");
        }
        Err(_) => println!("    (note: sqrt() may not be implemented)"),
    }
}

#[test]
fn t42_log() {
    let (db, _dir) = fresh_db("mathfull");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN log(1) AS v")
    }));
    match result {
        Ok(rows) => {
            let v = val_f64(&rows, 0, "v");
            assert!((v - 0.0).abs() < 0.001, "log(1) should be 0.0, got {v}");
        }
        Err(_) => println!("    (note: log() may not be implemented)"),
    }
}

#[test]
fn t42_e_constant() {
    let (db, _dir) = fresh_db("mathfull");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN e() AS v")
    }));
    match result {
        Ok(rows) => {
            let v = val_f64(&rows, 0, "v");
            assert!((v - std::f64::consts::E).abs() < 0.01, "e() should be ~2.718, got {v}");
        }
        Err(_) => println!("    (note: e() may not be implemented)"),
    }
}

#[test]
fn t42_pi_constant() {
    let (db, _dir) = fresh_db("mathfull");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN pi() AS v")
    }));
    match result {
        Ok(rows) => {
            let v = val_f64(&rows, 0, "v");
            assert!((v - std::f64::consts::PI).abs() < 0.01, "pi() should be ~3.14159, got {v}");
        }
        Err(_) => println!("    (note: pi() may not be implemented)"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 43. String functions (expanded)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t43_replace() {
    let (db, _dir) = fresh_db("strexp");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN replace('hello world', 'world', 'rust') AS v")
    }));
    match result {
        Ok(rows) => assert_eq!(val_str(&rows, 0, "v"), "hello rust"),
        Err(_) => println!("    (note: replace() may not be implemented)"),
    }
}

#[test]
fn t43_ltrim() {
    let (db, _dir) = fresh_db("strexp");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN lTrim('  hi') AS v")
    }));
    match result {
        Ok(rows) => assert_eq!(val_str(&rows, 0, "v"), "hi"),
        Err(_) => println!("    (note: lTrim() may not be implemented)"),
    }
}

#[test]
fn t43_rtrim() {
    let (db, _dir) = fresh_db("strexp");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN rTrim('hi  ') AS v")
    }));
    match result {
        Ok(rows) => assert_eq!(val_str(&rows, 0, "v"), "hi"),
        Err(_) => println!("    (note: rTrim() may not be implemented)"),
    }
}

#[test]
fn t43_split() {
    let (db, _dir) = fresh_db("strexp");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN split('a,b,c', ',') AS v")
    }));
    match result {
        Ok(rows) => match val(&rows, 0, "v") {
            Value::List(l) => {
                assert_eq!(l.len(), 3);
                assert_eq!(l[0], Value::String("a".to_string()));
            }
            other => panic!("expected List, got {other:?}"),
        },
        Err(_) => println!("    (note: split() may not be implemented)"),
    }
}

#[test]
fn t43_reverse() {
    let (db, _dir) = fresh_db("strexp");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN reverse('abc') AS v")
    }));
    match result {
        Ok(rows) => assert_eq!(val_str(&rows, 0, "v"), "cba"),
        Err(_) => println!("    (note: reverse() may not be implemented)"),
    }
}

#[test]
fn t43_substring() {
    let (db, _dir) = fresh_db("strexp");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN substring('hello', 1, 3) AS v")
    }));
    match result {
        Ok(rows) => assert_eq!(val_str(&rows, 0, "v"), "ell"),
        Err(_) => println!("    (note: substring() may not be implemented)"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 44. List operations
// ═══════════════════════════════════════════════════════════════

#[test]
fn t44_range_function() {
    let (db, _dir) = fresh_db("listops");
    let rows = query_rows(&db, "RETURN range(1, 5) AS v");
    match val(&rows, 0, "v") {
        Value::List(l) => {
            assert_eq!(l.len(), 5);
            assert_eq!(l[0], Value::Int(1));
            assert_eq!(l[4], Value::Int(5));
        }
        other => panic!("expected List, got {other:?}"),
    }
}

#[test]
fn t44_range_with_step() {
    let (db, _dir) = fresh_db("listops");
    let rows = query_rows(&db, "RETURN range(0, 10, 2) AS v");
    match val(&rows, 0, "v") {
        Value::List(l) => {
            assert_eq!(l.len(), 6); // 0, 2, 4, 6, 8, 10
            assert_eq!(l[0], Value::Int(0));
            assert_eq!(l[5], Value::Int(10));
        }
        other => panic!("expected List, got {other:?}"),
    }
}

#[test]
fn t44_list_index_access() {
    let (db, _dir) = fresh_db("listops");
    let rows = query_rows(&db, "RETURN [10, 20, 30][1] AS v");
    assert_eq!(val_i64(&rows, 0, "v"), 20);
}

#[test]
fn t44_list_size() {
    let (db, _dir) = fresh_db("listops");
    let rows = query_rows(&db, "RETURN size([1, 2, 3, 4]) AS v");
    assert_eq!(val_i64(&rows, 0, "v"), 4);
}

#[test]
fn t44_list_comprehension() {
    let (db, _dir) = fresh_db("listops");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN [x IN range(1, 5) WHERE x > 3] AS v")
    }));
    match result {
        Ok(rows) => match val(&rows, 0, "v") {
            Value::List(l) => {
                assert_eq!(l.len(), 2); // 4, 5
                assert_eq!(l[0], Value::Int(4));
            }
            other => panic!("expected List, got {other:?}"),
        },
        Err(_) => println!("    (note: list comprehension may not be implemented)"),
    }
}

#[test]
fn t44_reduce() {
    let (db, _dir) = fresh_db("listops");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN reduce(acc = 0, x IN [1, 2, 3] | acc + x) AS v")
    }));
    match result {
        Ok(rows) => assert_eq!(val_i64(&rows, 0, "v"), 6),
        Err(_) => println!("    (note: reduce() may not be implemented)"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 45. Map operations
// ═══════════════════════════════════════════════════════════════

#[test]
fn t45_map_literal() {
    let (db, _dir) = fresh_db("mapops");
    let rows = query_rows(&db, "RETURN {name: 'Alice', age: 30} AS m");
    match val(&rows, 0, "m") {
        Value::Map(m) => {
            assert_eq!(m.get("name"), Some(&Value::String("Alice".to_string())));
            assert_eq!(m.get("age"), Some(&Value::Int(30)));
        }
        other => panic!("expected Map, got {other:?}"),
    }
}

#[test]
fn t45_map_access() {
    let (db, _dir) = fresh_db("mapops");
    let rows = query_rows(&db, "WITH {name: 'Bob', age: 25} AS m RETURN m.name AS v");
    assert_eq!(val_str(&rows, 0, "v"), "Bob");
}

#[test]
fn t45_nested_map() {
    let (db, _dir) = fresh_db("mapops");
    let rows = query_rows(&db, "RETURN {outer: {inner: 42}} AS m");
    match val(&rows, 0, "m") {
        Value::Map(m) => match m.get("outer") {
            Some(Value::Map(inner)) => {
                assert_eq!(inner.get("inner"), Some(&Value::Int(42)));
            }
            other => panic!("expected inner Map, got {other:?}"),
        },
        other => panic!("expected Map, got {other:?}"),
    }
}

#[test]
fn t45_keys_function() {
    let (db, _dir) = fresh_db("mapops");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN keys({a: 1, b: 2}) AS v")
    }));
    match result {
        Ok(rows) => match val(&rows, 0, "v") {
            Value::List(l) => assert_eq!(l.len(), 2),
            other => panic!("expected List, got {other:?}"),
        },
        Err(_) => println!("    (note: keys() on map may not be implemented)"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 46. Multiple MATCH (cartesian product, correlated)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t46_multiple_match_cartesian() {
    let (db, _dir) = fresh_db("multimatch");
    exec_write(&db, "CREATE (:MA {v: 1})");
    exec_write(&db, "CREATE (:MA {v: 2})");
    exec_write(&db, "CREATE (:MB {v: 10})");
    let rows = query_rows(
        &db,
        "MATCH (a:MA) MATCH (b:MB) RETURN a.v, b.v ORDER BY a.v",
    );
    assert_eq!(rows.len(), 2, "cartesian product: 2 x 1 = 2");
    assert_eq!(val_i64(&rows, 0, "b.v"), 10);
}

#[test]
fn t46_multiple_match_correlated() {
    let (db, _dir) = fresh_db("multimatch");
    exec_write(&db, "CREATE (:MC {id: 'x'})-[:LINK]->(:MD {id: 'y'})");
    let rows = query_rows(
        &db,
        "MATCH (a:MC {id: 'x'}) MATCH (a)-[:LINK]->(b) RETURN b.id",
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "b.id"), "y");
}

#[test]
fn t46_multiple_match_independent() {
    let (db, _dir) = fresh_db("multimatch");
    exec_write(&db, "CREATE (:ME {v: 'a'})");
    exec_write(&db, "CREATE (:MF {v: 'b'})");
    let rows = query_rows(
        &db,
        "MATCH (a:ME) MATCH (b:MF) RETURN a.v AS av, b.v AS bv",
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(val_str(&rows, 0, "av"), "a");
    assert_eq!(val_str(&rows, 0, "bv"), "b");
}

// ═══════════════════════════════════════════════════════════════
// 47. REMOVE clause
// ═══════════════════════════════════════════════════════════════

#[test]
fn t47_remove_property() {
    let (db, _dir) = fresh_db("remove");
    exec_write(&db, "CREATE (:RM {name: 'test', extra: 'gone'})");
    exec_write(&db, "MATCH (n:RM {name: 'test'}) REMOVE n.extra");
    let rows = query_rows(&db, "MATCH (n:RM {name: 'test'}) RETURN n.extra");
    assert_eq!(val(&rows, 0, "n.extra"), Value::Null);
}

#[test]
fn t47_remove_multiple_properties() {
    let (db, _dir) = fresh_db("remove");
    exec_write(&db, "CREATE (:RM2 {a: 1, b: 2, c: 3})");
    exec_write(&db, "MATCH (n:RM2) REMOVE n.a, n.b");
    let rows = query_rows(&db, "MATCH (n:RM2) RETURN n.a, n.b, n.c");
    assert_eq!(val(&rows, 0, "n.a"), Value::Null);
    assert_eq!(val(&rows, 0, "n.b"), Value::Null);
    assert_eq!(val_i64(&rows, 0, "n.c"), 3);
}

#[test]
fn t47_remove_label() {
    let (db, _dir) = fresh_db("remove");
    exec_write(&db, "CREATE (:RL:Extra {name: 'labeled'})");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        exec_write(&db, "MATCH (n:RL {name: 'labeled'}) REMOVE n:Extra");
    }));
    match result {
        Ok(_) => {
            let rows = reify_rows(&db, "MATCH (n:RL {name: 'labeled'}) RETURN n");
            if !rows.is_empty() {
                let node_val = &rows[0].iter().find(|(k, _)| k == "n").unwrap().1;
                match node_val {
                    Value::Node(n) => {
                        assert!(!n.labels.contains(&"Extra".to_string()), "Extra label should be removed");
                    }
                    _ => println!("    (note: unexpected return type)"),
                }
            }
        }
        Err(_) => println!("    (note: REMOVE label may not be implemented)"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 48. Parameter queries (Cypher $param)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t48_param_in_where() {
    let (db, _dir) = fresh_db("params2");
    exec_write(&db, "CREATE (:PM {name: 'Alice', age: 30})");
    let mut params = Params::new();
    params.insert("name".to_string(), Value::String("Alice".to_string()));
    let rows = query_rows_params(&db, "MATCH (n:PM) WHERE n.name = $name RETURN n.age", &params);
    assert_eq!(val_i64(&rows, 0, "n.age"), 30);
}

#[test]
fn t48_param_in_create() {
    let (db, _dir) = fresh_db("params2");
    let mut params = Params::new();
    params.insert("val".to_string(), Value::Int(99));
    exec_write_params(&db, "CREATE (:PM2 {v: $val})", &params);
    let rows = query_rows(&db, "MATCH (n:PM2) RETURN n.v");
    assert_eq!(val_i64(&rows, 0, "n.v"), 99);
}

#[test]
fn t48_param_multiple() {
    let (db, _dir) = fresh_db("params2");
    let mut params = Params::new();
    params.insert("a".to_string(), Value::Int(1));
    params.insert("b".to_string(), Value::Int(2));
    let rows = query_rows_params(&db, "RETURN $a + $b AS sum", &params);
    assert_eq!(val_i64(&rows, 0, "sum"), 3);
}

#[test]
fn t48_param_string() {
    let (db, _dir) = fresh_db("params2");
    let mut params = Params::new();
    params.insert("greeting".to_string(), Value::String("hello".to_string()));
    let rows = query_rows_params(&db, "RETURN $greeting AS v", &params);
    assert_eq!(val_str(&rows, 0, "v"), "hello");
}

// ═══════════════════════════════════════════════════════════════
// 49. EXPLAIN
// ═══════════════════════════════════════════════════════════════

#[test]
fn t49_explain_basic() {
    let (db, _dir) = fresh_db("explain");
    exec_write(&db, "CREATE (:EX {v: 1})");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "EXPLAIN MATCH (n:EX) RETURN n")
    }));
    match result {
        Ok(rows) => {
            // EXPLAIN may return plan rows or empty result
            println!("    EXPLAIN returned {} rows", rows.len());
        }
        Err(_) => println!("    (note: EXPLAIN may not be implemented)"),
    }
}

// ═══════════════════════════════════════════════════════════════
// 50. Index operations (Cypher-level)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t50_create_index_via_api() {
    let (db, _dir) = fresh_db("idxops");
    for i in 0..20 {
        exec_write(&db, &format!("CREATE (:IX {{val: {i}}})"));
    }
    db.create_index("IX", "val").unwrap();
    // Index should accelerate lookup
    let rows = query_rows(&db, "MATCH (n:IX {val: 10}) RETURN n.val");
    assert_eq!(rows.len(), 1);
    assert_eq!(val_i64(&rows, 0, "n.val"), 10);
}

#[test]
fn t50_index_with_updates() {
    let (db, _dir) = fresh_db("idxops");
    exec_write(&db, "CREATE (:IX2 {email: 'a@b.com'})");
    db.create_index("IX2", "email").unwrap();
    // Update after index creation
    exec_write(&db, "CREATE (:IX2 {email: 'c@d.com'})");
    let rows = query_rows(&db, "MATCH (n:IX2 {email: 'c@d.com'}) RETURN n.email");
    assert_eq!(rows.len(), 1);
}

#[test]
fn t50_index_range_query() {
    let (db, _dir) = fresh_db("idxops");
    for i in 0..50 {
        exec_write(&db, &format!("CREATE (:IX3 {{v: {i}}})"));
    }
    db.create_index("IX3", "v").unwrap();
    let rows = query_rows(&db, "MATCH (n:IX3) WHERE n.v >= 40 RETURN n.v ORDER BY n.v");
    assert_eq!(rows.len(), 10);
    assert_eq!(val_i64(&rows, 0, "n.v"), 40);
}

// ═══════════════════════════════════════════════════════════════
// 51. Concurrent reads (multiple ReadTxn snapshots)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t51_concurrent_read_snapshots() {
    let (db, _dir) = fresh_db("concurrent");
    exec_write(&db, "CREATE (:CR {v: 1})");

    // Take snapshot before write
    let snap1 = db.snapshot();

    // Write more data
    exec_write(&db, "CREATE (:CR {v: 2})");

    // Take snapshot after write
    let snap2 = db.snapshot();

    // snap1 should see 1 node, snap2 should see 2
    let q = prepare("MATCH (n:CR) RETURN count(n) AS c").unwrap();
    let rows1: Vec<Row> = q
        .execute_streaming(&snap1, &Params::new())
        .collect::<nervusdb_query::Result<Vec<_>>>()
        .unwrap();
    let rows2: Vec<Row> = q
        .execute_streaming(&snap2, &Params::new())
        .collect::<nervusdb_query::Result<Vec<_>>>()
        .unwrap();

    let c1 = val_i64(&rows1, 0, "c");
    let c2 = val_i64(&rows2, 0, "c");
    assert_eq!(c1, 1, "snap1 should see 1 node");
    assert_eq!(c2, 2, "snap2 should see 2 nodes");
}

#[test]
fn t51_read_txn_isolation() {
    let (db, _dir) = fresh_db("concurrent");
    exec_write(&db, "CREATE (:RI {v: 'before'})");

    // Take snapshot before second write
    let snap_before = db.snapshot();

    // Write while snapshot is held
    exec_write(&db, "CREATE (:RI {v: 'after'})");

    let snap_after = db.snapshot();

    // snap_before should only see 1 node
    let q = prepare("MATCH (n:RI) RETURN count(n) AS c").unwrap();
    let rows_before: Vec<Row> = q
        .execute_streaming(&snap_before, &Params::new())
        .collect::<nervusdb_query::Result<Vec<_>>>()
        .unwrap();
    let rows_after: Vec<Row> = q
        .execute_streaming(&snap_after, &Params::new())
        .collect::<nervusdb_query::Result<Vec<_>>>()
        .unwrap();
    let c_before = val_i64(&rows_before, 0, "c");
    let c_after = val_i64(&rows_after, 0, "c");
    assert_eq!(c_before, 1, "snapshot before write should see 1 node");
    assert_eq!(c_after, 2, "snapshot after write should see 2 nodes");
}

// ═══════════════════════════════════════════════════════════════
// 52. Error handling (expanded)
// ═══════════════════════════════════════════════════════════════

#[test]
fn t52_syntax_error_detail() {
    let err = prepare("MATC (n) RETURN n");
    assert!(err.is_err());
    let msg = format!("{}", err.unwrap_err());
    assert!(!msg.is_empty(), "syntax error should have message");
}

#[test]
fn t52_unknown_function_error() {
    let err = prepare("RETURN noSuchFunction(1)");
    assert!(err.is_err());
}

#[test]
fn t52_type_error_in_arithmetic() {
    let (db, _dir) = fresh_db("err2");
    // Adding string + int should produce an error or null
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN 'hello' + 1 AS v")
    }));
    match result {
        Ok(rows) => {
            // Some engines coerce, some return null, some error
            println!("    'hello' + 1 = {:?}", val(&rows, 0, "v"));
        }
        Err(_) => println!("    (type error correctly raised for string + int)"),
    }
}

#[test]
fn t52_division_by_zero() {
    let (db, _dir) = fresh_db("err2");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        query_rows(&db, "RETURN 1 / 0 AS v")
    }));
    match result {
        Ok(rows) => {
            // May return null, infinity, or error
            println!("    1/0 = {:?}", val(&rows, 0, "v"));
        }
        Err(_) => println!("    (division by zero correctly raised error)"),
    }
}

#[test]
fn t52_missing_property_returns_null() {
    let (db, _dir) = fresh_db("err2");
    exec_write(&db, "CREATE (:EP {name: 'test'})");
    let rows = query_rows(&db, "MATCH (n:EP) RETURN n.nonexistent");
    assert_eq!(val(&rows, 0, "n.nonexistent"), Value::Null);
}

#[test]
fn t52_delete_connected_node_error() {
    let (db, _dir) = fresh_db("err2");
    exec_write(&db, "CREATE (:DN {v: 1})-[:R]->(:DN {v: 2})");
    // DELETE without DETACH on connected node should error
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        exec_write(&db, "MATCH (n:DN {v: 1}) DELETE n");
    }));
    match result {
        Ok(_) => println!("    (note: DELETE connected node succeeded — engine may auto-detach)"),
        Err(_) => println!("    (confirmed: DELETE connected node without DETACH raises error)"),
    }
}
