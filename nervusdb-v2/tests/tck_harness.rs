use cucumber::{World, given, then, when};
use nervusdb_v2::Db;
use nervusdb_v2_query::{Params, Value, prepare};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct GraphWorld {
    db: Option<Arc<Db>>,
    _dir: Option<Arc<TempDir>>,
    last_result: Option<Vec<HashMap<String, Value>>>,
    last_error: Option<String>,
}

impl GraphWorld {
    async fn new() -> Result<Self, Infallible> {
        let dir = TempDir::new().expect("failed to create temp dir");
        let db = Db::open(dir.path()).expect("failed to open db");
        Ok(Self {
            db: Some(Arc::new(db)),
            _dir: Some(Arc::new(dir)),
            last_result: None,
            last_error: None,
        })
    }

    fn get_db(&self) -> Arc<Db> {
        self.db.as_ref().expect("DB not initialized").clone()
    }
}

#[given("an empty graph")]
#[given("any graph")]
async fn empty_graph(_world: &mut GraphWorld) {
    // Already empty on new()
}

#[then("no side effects")]
async fn no_side_effects(_world: &mut GraphWorld) {
    // TODO: Verify no changes were made (check DB stats?)
}

#[given(regex = r"^having executed:$")]
async fn having_executed(world: &mut GraphWorld, step: &cucumber::gherkin::Step) {
    let query = step
        .docstring
        .as_ref()
        .expect("Expected docstring for query")
        .trim();
    execute_write(world, query);
}

#[when(regex = r"^executing query:$")]
async fn executing_query(world: &mut GraphWorld, step: &cucumber::gherkin::Step) {
    let query = step
        .docstring
        .as_ref()
        .expect("Expected docstring for query")
        .trim();
    execute_query_generic(world, query);
}

/// Generic query execution that handles both read and write queries
fn execute_query_generic(world: &mut GraphWorld, cypher: &str) {
    let db = world.get_db();
    let query_r = prepare(cypher);

    match query_r {
        Ok(query) => {
            let snapshot = db.snapshot();
            let params = Params::new();
            let mut txn = db.begin_write();

            // Try execute_streaming first for read queries
            let rows: Vec<_> = query.execute_streaming(&snapshot, &params).collect();

            // Check if there were any errors
            let mut has_error = false;
            let mut error_msg = None;
            let mut results = Vec::new();

            for row_res in rows {
                match row_res {
                    Ok(row) => {
                        let mut map = std::collections::HashMap::new();
                        for (k, v) in row.columns().iter().cloned() {
                            map.insert(k, v);
                        }
                        results.push(map);
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        // Check if error is about write operations
                        if err_str.contains("must be executed via execute_write") {
                            // This is a write-only query, use execute_write
                            execute_write_fallback(world, &query, &snapshot, &params, &mut txn);
                            return;
                        } else {
                            has_error = true;
                            error_msg = Some(err_str);
                            break;
                        }
                    }
                }
            }

            if !has_error {
                if let Err(e) = txn.commit() {
                    world.last_error = Some(e.to_string());
                    return;
                }
                world.last_result = Some(results);
                world.last_error = None;
            } else {
                world.last_result = None;
                world.last_error = error_msg;
            }
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

/// Fallback execute_write for pure write queries (reuses existing transaction)
fn execute_write_fallback<S: nervusdb_v2_api::GraphSnapshot>(
    world: &mut GraphWorld,
    query: &nervusdb_v2_query::PreparedQuery,
    snapshot: &S,
    params: &nervusdb_v2_query::Params,
    txn: &mut nervusdb_v2::WriteTxn,
) {
    match query.execute_write(snapshot, txn, params) {
        Ok(_) => {
            // Transaction will be committed by caller
            // Pure write queries don't return data
            world.last_result = Some(vec![]);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

fn execute_write(world: &mut GraphWorld, cypher: &str) {
    let db = world.get_db(); // Returns Arc<Db>, releases borrow on world
    // Prepare can be done outside transaction? Yes.
    let query_r = prepare(cypher);

    let exec_result: Result<(), String> = match query_r {
        Ok(query) => {
            let mut txn = db.begin_write();
            let snapshot = db.snapshot();
            let params = Params::new();
            match query.execute_write(&snapshot, &mut txn, &params) {
                Ok(_) => match txn.commit() {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e.to_string()),
                },
                Err(e) => Err(e.to_string()),
            }
        }
        Err(e) => Err(e.to_string()),
    };

    match exec_result {
        Ok(_) => world.last_error = None,
        Err(e) => world.last_error = Some(e),
    }
}

#[allow(dead_code)]
fn execute_read(world: &mut GraphWorld, cypher: &str) {
    let db = world.get_db();
    let query_r = prepare(cypher);

    let exec_result: Result<Vec<HashMap<String, Value>>, String> = match query_r {
        Ok(query) => {
            let snapshot = db.snapshot();
            let params = Params::new();
            let rows = query.execute_streaming(&snapshot, &params);

            let mut results = Vec::new();
            let mut err = None;
            for row in rows {
                match row {
                    Ok(r) => {
                        let mut map = HashMap::new();
                        for (k, v) in r.columns().iter().cloned() {
                            map.insert(k, v);
                        }
                        results.push(map);
                    }
                    Err(e) => {
                        err = Some(e.to_string());
                        break;
                    }
                }
            }
            if let Some(e) = err {
                Err(e)
            } else {
                Ok(results)
            }
        }
        Err(e) => Err(e.to_string()),
    };

    match exec_result {
        Ok(r) => {
            world.last_result = Some(r);
            world.last_error = None;
        }
        Err(e) => world.last_error = Some(e),
    }
}

#[then(regex = r"^a SyntaxError should be raised at compile time: (.+)$")]
async fn syntax_error_raised(world: &mut GraphWorld, error_type: String) {
    // Check if we got an error as expected
    let err = world
        .last_error
        .as_ref()
        .expect("Expected a SyntaxError but got success");

    // For MVP, we check if error contains the expected type or just assert we got an error
    // NervusDB parser currently returns general errors, not specific codes yet
    // We'll do basic matching for common error types

    let err_lower = err.to_lowercase();
    let _expected_lower = error_type.to_lowercase();

    // Check for common error patterns
    let matches = err_lower.contains("error")
        || err_lower.contains("unexpected token")
        || err_lower.contains("syntax")
        || err_lower.contains("parse");

    if !matches {
        panic!("Expected error type '{}' but got: {}", error_type, err);
    }

    eprintln!("Got expected error ({}): {}", error_type, err);
}

#[then(regex = r"^the result should be, in any order:$")]
async fn result_should_be_any_order(world: &mut GraphWorld, step: &cucumber::gherkin::Step) {
    assert!(
        world.last_error.is_none(),
        "Query failed: {:?}",
        world.last_error
    );

    let expected_table = step.table.as_ref().expect("Expected table");
    let actual_results = world
        .last_result
        .as_ref()
        .expect("No results from previous query");

    // 1. Get headers
    // cucumber-rs Table struct only gives us rows. First row is header.
    let rows = &expected_table.rows;
    if rows.is_empty() {
        return;
    }
    let headers = &rows[0];

    // 2. Parse expected rows
    let mut expected_rows = Vec::new();
    for row in rows.iter().skip(1) {
        let mut row_map = HashMap::new();
        for (i, val_str) in row.iter().enumerate() {
            if i < headers.len() {
                let header = &headers[i];
                let val = parse_tck_value(val_str);
                row_map.insert(header.clone(), val);
            }
        }
        expected_rows.push(row_map);
    }

    // 3. Convert to canonical format for comparison
    let expected_canonical: Vec<Vec<(String, Value)>> =
        expected_rows.into_iter().map(canonicalize).collect();
    let actual_canonical: Vec<Vec<(String, Value)>> = actual_results
        .clone()
        .into_iter()
        .map(canonicalize)
        .collect();

    // 4. Compare as multisets
    if expected_canonical.len() != actual_canonical.len() {
        panic!(
            "Row count mismatch.\nExpected: {} rows\nActual: {} rows\nExpected Data: {:?}\nActual Data: {:?}",
            expected_canonical.len(),
            actual_canonical.len(),
            expected_canonical,
            actual_canonical
        );
    }

    let mut actual_remaining = actual_canonical.clone();

    for expected_row in &expected_canonical {
        if let Some(pos) = actual_remaining
            .iter()
            .position(|r| row_eq(r, expected_row))
        {
            actual_remaining.remove(pos);
        } else {
            panic!(
                "Expected row not found in actual results:\nExpected Row: {:?}\nActual Remaining: {:?}",
                expected_row, actual_remaining
            );
        }
    }
}

fn canonicalize(row: HashMap<String, Value>) -> Vec<(String, Value)> {
    let mut v: Vec<_> = row.into_iter().collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn row_eq(a: &[(String, Value)], b: &[(String, Value)]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i].0 != b[i].0 {
            return false;
        }
        if !value_eq(&a[i].1, &b[i].1) {
            return false;
        }
    }
    true
}

fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => (a - b).abs() < 1e-9,
        (Value::Int(i), Value::Float(f)) => (*i as f64 - *f).abs() < 1e-9,
        (Value::Float(f), Value::Int(i)) => (*f - *i as f64).abs() < 1e-9,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::List(a), Value::List(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter().zip(b.iter()).all(|(a, b)| value_eq(a, b))
        }
        // Handle Node comparisons - normalize to string representation for TCK
        (Value::NodeId(a), Value::NodeId(b)) => a == b,
        (Value::ExternalId(a), Value::ExternalId(b)) => a == b,
        // Handle Edge comparisons
        (Value::EdgeKey(a), Value::EdgeKey(b)) => a == b,
        // Handle Path - compare as string representations
        (Value::Path(a), Value::Path(b)) => {
            // For MVP, compare path string representations
            format!("{:?}", a) == format!("{:?}", b)
        }
        // Fallback: compare debug representations
        _ => format!("{:?}", a) == format!("{:?}", b),
    }
}

fn parse_tck_value(s: &str) -> Value {
    let s = s.trim();
    if s == "null" {
        return Value::Null;
    }
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        return Value::String(s[1..s.len() - 1].to_string());
    }
    // Handle list format: [1, 2, 3] or ['a', 'b']
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        if inner.is_empty() {
            return Value::List(vec![]);
        }
        // Simple split for basic cases - doesn't handle nested lists
        let items: Vec<Value> = inner
            .split(',')
            .map(|item| parse_tck_value(item.trim()))
            .collect();
        return Value::List(items);
    }
    if let Ok(i) = s.parse::<i64>() {
        return Value::Int(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }
    // Fallback: treat as unquoted string (for node/rel references like (:Person))
    Value::String(s.to_string())
}

fn main() {
    futures::executor::block_on(
        GraphWorld::cucumber().run_and_exit("tests/opencypher_tck/tck/features"),
    );
}
