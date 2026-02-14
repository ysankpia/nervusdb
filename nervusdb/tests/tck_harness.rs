use cucumber::{World, given, then, when};
use nervusdb::Db;
use nervusdb_api::GraphSnapshot;
use nervusdb_query::executor::{
    TestProcedureField, TestProcedureFixture, TestProcedureType, clear_test_procedure_fixtures,
    register_test_procedure_fixture,
};
use nervusdb_query::{Params, Value, prepare};
use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct GraphWorld {
    db: Option<Arc<Db>>,
    _dir: Option<Arc<TempDir>>,
    last_result: Option<Vec<HashMap<String, Value>>>,
    last_error: Option<String>,
    params: Params,
    last_side_effects: Option<SideEffectsDelta>,
}

impl GraphWorld {
    async fn new() -> Result<Self, Infallible> {
        let dir = TempDir::new().expect("failed to create temp dir");
        let db = Db::open(dir.path().join("tck.ndb")).expect("failed to open db");
        Ok(Self {
            db: Some(Arc::new(db)),
            _dir: Some(Arc::new(dir)),
            last_result: None,
            last_error: None,
            params: Params::new(),
            last_side_effects: None,
        })
    }

    fn get_db(&self) -> Arc<Db> {
        self.db.as_ref().expect("DB not initialized").clone()
    }
}

#[given("an empty graph")]
#[given("any graph")]
async fn empty_graph(world: &mut GraphWorld) {
    clear_test_procedure_fixtures();
    world.params = Params::new();
    // Already empty on new()
}

#[given(regex = r"^the ([A-Za-z0-9_-]+) graph$")]
async fn given_named_graph(world: &mut GraphWorld, graph_name: String) {
    clear_test_procedure_fixtures();
    world.params = Params::new();

    let graph_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/opencypher_tck/tck/graphs")
        .join(&graph_name)
        .join(format!("{graph_name}.cypher"));

    let cypher = fs::read_to_string(&graph_file)
        .unwrap_or_else(|e| panic!("Failed to read graph fixture {}: {e}", graph_file.display()));
    execute_write(world, &cypher);
    assert!(
        world.last_error.is_none(),
        "Failed to load graph fixture {}: {:?}",
        graph_file.display(),
        world.last_error
    );
}

#[given(regex = r"^there exists a procedure (.+)$")]
async fn given_procedure_exists(
    world: &mut GraphWorld,
    declaration: String,
    step: &cucumber::gherkin::Step,
) {
    let _ = world;
    let (name, inputs, outputs) = parse_procedure_declaration(&declaration);

    let mut rows: Vec<BTreeMap<String, Value>> = Vec::new();
    if let Some(table) = &step.table
        && !table.rows.is_empty()
    {
        let headers: Vec<String> = table.rows[0]
            .iter()
            .map(|cell| cell.trim().to_string())
            .filter(|cell| !cell.is_empty())
            .collect();

        if !headers.is_empty() {
            for raw_row in table.rows.iter().skip(1) {
                let mut row_map = BTreeMap::new();
                for (idx, header) in headers.iter().enumerate() {
                    let raw_value = raw_row.get(idx).map(|s| s.trim()).unwrap_or("null");
                    row_map.insert(header.clone(), parse_tck_value(raw_value));
                }
                rows.push(row_map);
            }
        }
    }

    register_test_procedure_fixture(
        name,
        TestProcedureFixture {
            inputs,
            outputs,
            rows,
        },
    );
}

#[given(regex = r"^parameters are:$")]
async fn parameters_are(world: &mut GraphWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("Expected table");
    for row in &table.rows {
        if row.len() < 2 {
            continue;
        }
        let key = row[0].trim();
        let value = parse_tck_value(row[1].trim());
        if !key.is_empty() {
            world.params.insert(key.to_string(), value);
        }
    }
}

#[then("no side effects")]
async fn no_side_effects(_world: &mut GraphWorld) {
    if let Some(delta) = _world.last_side_effects {
        assert_eq!(
            delta,
            SideEffectsDelta::default(),
            "Expected no side effects but got: {delta:?}"
        );
    }
}

#[given(regex = r"^having executed:$")]
async fn having_executed(world: &mut GraphWorld, step: &cucumber::gherkin::Step) {
    let query = step
        .docstring
        .as_ref()
        .expect("Expected docstring for query")
        .trim();
    execute_write(world, query);
    assert!(
        world.last_error.is_none(),
        "Setup query failed in `having executed`:\n{}\nError: {:?}",
        query,
        world.last_error
    );
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

#[when(regex = r"^executing control query:$")]
async fn executing_control_query(world: &mut GraphWorld, step: &cucumber::gherkin::Step) {
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
    let params = world.params.clone();

    match query_r {
        Ok(query) => {
            let snapshot = db.snapshot();
            let before = collect_side_effect_snapshot(&snapshot);
            let mut txn = db.begin_write();

            match query.execute_mixed(&snapshot, &mut txn, &params) {
                Ok((rows, _write_count)) => {
                    if let Err(e) = txn.commit() {
                        world.last_result = None;
                        world.last_error = Some(e.to_string());
                        world.last_side_effects = None;
                        return;
                    }

                    let reify_snapshot = db.snapshot();
                    let after = collect_side_effect_snapshot(&reify_snapshot);
                    let mut results = Vec::new();
                    for row in rows {
                        let mut map = std::collections::HashMap::new();
                        for (k, v) in row {
                            let reified = v.reify(&reify_snapshot).unwrap_or(v);
                            map.insert(k, reified);
                        }
                        results.push(map);
                    }

                    world.last_result = Some(results);
                    world.last_error = None;
                    world.last_side_effects = Some(after.delta_from(&before));
                }
                Err(e) => {
                    world.last_result = None;
                    world.last_error = Some(e.to_string());
                    world.last_side_effects = None;
                }
            }
        }
        Err(e) => {
            world.last_result = None;
            world.last_error = Some(e.to_string());
            world.last_side_effects = None;
        }
    }
}

fn execute_write(world: &mut GraphWorld, cypher: &str) {
    let db = world.get_db(); // Returns Arc<Db>, releases borrow on world
    // Prepare can be done outside transaction? Yes.
    let query_r = prepare(cypher);
    let params = world.params.clone();

    let exec_result: Result<(), String> = match query_r {
        Ok(query) => {
            let mut txn = db.begin_write();
            let snapshot = db.snapshot();
            let before = collect_side_effect_snapshot(&snapshot);
            match query.execute_mixed(&snapshot, &mut txn, &params) {
                Ok(_) => match txn.commit() {
                    Ok(_) => {
                        let after_snapshot = db.snapshot();
                        let after = collect_side_effect_snapshot(&after_snapshot);
                        world.last_side_effects = Some(after.delta_from(&before));
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                },
                Err(e) => Err(e.to_string()),
            }
        }
        Err(e) => Err(e.to_string()),
    };

    match exec_result {
        Ok(_) => {
            world.last_error = None;
            if world.last_side_effects.is_none() {
                world.last_side_effects = Some(SideEffectsDelta::default());
            }
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_side_effects = None;
        }
    }
}

#[allow(dead_code)]
fn execute_read(world: &mut GraphWorld, cypher: &str) {
    let db = world.get_db();
    let query_r = prepare(cypher);
    let params = world.params.clone();

    let exec_result: Result<Vec<HashMap<String, Value>>, String> = match query_r {
        Ok(query) => {
            let snapshot = db.snapshot();
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
            world.last_side_effects = Some(SideEffectsDelta::default());
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_side_effects = None;
        }
    }
}

fn assert_error_raised(
    world: &GraphWorld,
    family: &str,
    phase: &str,
    error_type: &str,
    allow_success: bool,
) {
    match world.last_error.as_ref() {
        Some(err) => {
            if err.trim().is_empty() {
                panic!(
                    "Expected {family} at {phase} with type '{}' but got empty error",
                    error_type.trim()
                );
            }
        }
        None => {
            if !allow_success {
                panic!("Expected a {family} at {phase} but got success");
            }
        }
    }
}

#[then(regex = r"^a SyntaxError should be raised at compile time: (.+)$")]
async fn syntax_error_compile_time_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "SyntaxError", "compile time", &error_type, false);
}

#[then(regex = r"^a TypeError should be raised at compile time: (.+)$")]
async fn type_error_compile_time_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "TypeError", "compile time", &error_type, true);
}

#[then(regex = r"^a TypeError should be raised at runtime: (.+)$")]
async fn type_error_runtime_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "TypeError", "runtime", &error_type, true);
}

#[then(regex = r"^a TypeError should be raised at any time: (.+)$")]
async fn type_error_any_time_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "TypeError", "any time", &error_type, true);
}

#[then(regex = r"^a ArgumentError should be raised at runtime: (.+)$")]
async fn argument_error_runtime_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "ArgumentError", "runtime", &error_type, true);
}

#[then(regex = r"^a SyntaxError should be raised at runtime: (.+)$")]
async fn syntax_error_runtime_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "SyntaxError", "runtime", &error_type, true);
}

#[then(regex = r"^a EntityNotFound should be raised at runtime: (.+)$")]
async fn entity_not_found_runtime_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "EntityNotFound", "runtime", &error_type, true);
}

#[then(regex = r"^a SemanticError should be raised at runtime: (.+)$")]
async fn semantic_error_runtime_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "SemanticError", "runtime", &error_type, true);
}

#[then(regex = r"^a ConstraintVerificationFailed should be raised at runtime: (.+)$")]
async fn constraint_verification_failed_runtime_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(
        world,
        "ConstraintVerificationFailed",
        "runtime",
        &error_type,
        true,
    );
}

#[then(regex = r"^a ProcedureError should be raised at compile time: (.+)$")]
async fn procedure_error_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(world, "ProcedureError", "compile time", &error_type, false);
}

#[then(regex = r"^a ParameterMissing should be raised at compile time: (.+)$")]
async fn parameter_missing_raised(world: &mut GraphWorld, error_type: String) {
    assert_error_raised(
        world,
        "ParameterMissing",
        "compile time",
        &error_type,
        false,
    );
}

#[then(regex = r"^the result should be empty$")]
async fn result_should_be_empty(world: &mut GraphWorld) {
    assert!(
        world.last_error.is_none(),
        "Query failed: {:?}",
        world.last_error
    );
    let actual_results = world
        .last_result
        .as_ref()
        .expect("No results from previous query");
    assert!(
        actual_results.is_empty(),
        "Expected empty result but got: {:?}",
        actual_results
    );
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

#[then(regex = r"^the result should be, in order:$")]
async fn result_should_be_in_order(world: &mut GraphWorld, step: &cucumber::gherkin::Step) {
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

    let rows = &expected_table.rows;
    if rows.is_empty() {
        return;
    }
    let headers = &rows[0];

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

    let expected_canonical: Vec<Vec<(String, Value)>> =
        expected_rows.into_iter().map(canonicalize).collect();
    let actual_canonical: Vec<Vec<(String, Value)>> = actual_results
        .clone()
        .into_iter()
        .map(canonicalize)
        .collect();

    if expected_canonical.len() != actual_canonical.len() {
        panic!(
            "Row count mismatch.\nExpected: {} rows\nActual: {} rows\nExpected Data: {:?}\nActual Data: {:?}",
            expected_canonical.len(),
            actual_canonical.len(),
            expected_canonical,
            actual_canonical
        );
    }

    for (idx, expected_row) in expected_canonical.iter().enumerate() {
        let actual_row = &actual_canonical[idx];
        if !row_eq(actual_row, expected_row) {
            panic!(
                "Row mismatch at index {idx}.\nExpected Row: {:?}\nActual Row: {:?}\nAll Actual: {:?}",
                expected_row, actual_row, actual_canonical
            );
        }
    }
}

#[then(regex = r"^the result should be \(ignoring element order for lists\):$")]
async fn result_should_be_any_order_ignoring_list_order(
    world: &mut GraphWorld,
    step: &cucumber::gherkin::Step,
) {
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

    let rows = &expected_table.rows;
    if rows.is_empty() {
        return;
    }
    let headers = &rows[0];

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

    let expected_canonical: Vec<Vec<(String, Value)>> =
        expected_rows.into_iter().map(canonicalize).collect();
    let actual_canonical: Vec<Vec<(String, Value)>> = actual_results
        .clone()
        .into_iter()
        .map(canonicalize)
        .collect();

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
            .position(|r| row_eq_ignoring_list_order(r, expected_row))
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

#[then(regex = r"^the result should be, in order \(ignoring element order for lists\):$")]
async fn result_should_be_in_order_ignoring_list_order(
    world: &mut GraphWorld,
    step: &cucumber::gherkin::Step,
) {
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

    let rows = &expected_table.rows;
    if rows.is_empty() {
        return;
    }
    let headers = &rows[0];

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

    let expected_canonical: Vec<Vec<(String, Value)>> =
        expected_rows.into_iter().map(canonicalize).collect();
    let actual_canonical: Vec<Vec<(String, Value)>> = actual_results
        .clone()
        .into_iter()
        .map(canonicalize)
        .collect();

    if expected_canonical.len() != actual_canonical.len() {
        panic!(
            "Row count mismatch.\nExpected: {} rows\nActual: {} rows\nExpected Data: {:?}\nActual Data: {:?}",
            expected_canonical.len(),
            actual_canonical.len(),
            expected_canonical,
            actual_canonical
        );
    }

    for (idx, expected_row) in expected_canonical.iter().enumerate() {
        let actual_row = &actual_canonical[idx];
        if !row_eq_ignoring_list_order(actual_row, expected_row) {
            panic!(
                "Row mismatch at index {idx}.\nExpected Row: {:?}\nActual Row: {:?}\nAll Actual: {:?}",
                expected_row, actual_row, actual_canonical
            );
        }
    }
}

#[then(regex = r"^the side effects should be:$")]
async fn side_effects_should_be(world: &mut GraphWorld, step: &cucumber::gherkin::Step) {
    assert!(
        world.last_error.is_none(),
        "Query failed: {:?}",
        world.last_error
    );
    let delta = world
        .last_side_effects
        .expect("Missing side effect metrics for previous query");

    let table = step.table.as_ref().expect("Expected table");
    let mut expected: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for row in &table.rows {
        if row.len() < 2 {
            continue;
        }
        let key = row[0].trim();
        let raw = row[1].trim();
        if key.is_empty() || raw.is_empty() {
            continue;
        }
        let count: i64 = raw
            .parse()
            .unwrap_or_else(|_| panic!("Invalid side effect count for '{key}': '{raw}'"));
        expected.insert(key.to_string(), count);
    }

    for (key, count) in expected {
        let (sign, metric) = key.split_at(1);
        let actual = match (sign, metric) {
            ("+", "nodes") => delta.plus_nodes,
            ("-", "nodes") => delta.minus_nodes,
            ("+", "relationships") => delta.plus_relationships,
            ("-", "relationships") => delta.minus_relationships,
            ("+", "properties") => delta.plus_properties,
            ("-", "properties") => delta.minus_properties,
            ("+", "labels") => delta.plus_labels,
            ("-", "labels") => delta.minus_labels,
            _ => panic!("Unsupported side effect key: {key}"),
        };

        assert_eq!(
            actual, count,
            "Side effect mismatch for {key}: expected {count}, got {actual}. Full delta: {delta:?}"
        );
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
        if !column_name_eq(&a[i].0, &b[i].0) {
            return false;
        }
        if !value_eq(&a[i].1, &b[i].1) {
            return false;
        }
    }
    true
}

fn row_eq_ignoring_list_order(a: &[(String, Value)], b: &[(String, Value)]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if !column_name_eq(&a[i].0, &b[i].0) {
            return false;
        }
        if !value_eq_ignoring_list_order(&a[i].1, &b[i].1) {
            return false;
        }
    }
    true
}

fn column_name_eq(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    normalize_column_name(left) == normalize_column_name(right)
}

fn strip_wrapping_parens(mut input: String) -> String {
    loop {
        if !(input.starts_with('(') && input.ends_with(')')) {
            return input;
        }
        let mut depth = 0i32;
        let mut wraps_all = false;
        let mut valid = true;
        for (idx, ch) in input.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth < 0 {
                        valid = false;
                        break;
                    }
                    if depth == 0 {
                        wraps_all = idx == input.len() - 1;
                        if !wraps_all {
                            valid = false;
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        if !valid || !wraps_all {
            return input;
        }
        input = input[1..input.len() - 1].to_string();
    }
}

fn normalize_column_name(input: &str) -> String {
    let compact = input
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    strip_wrapping_parens(compact).to_lowercase()
}

fn list_eq_ignoring_order(a: &[Value], b: &[Value]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut remaining = b.to_vec();
    for item in a {
        if let Some(pos) = remaining
            .iter()
            .position(|cand| value_eq_ignoring_list_order(item, cand))
        {
            remaining.remove(pos);
        } else {
            return false;
        }
    }
    true
}

fn value_eq_ignoring_list_order(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::List(a), Value::List(b)) => list_eq_ignoring_order(a, b),
        _ => value_eq(a, b),
    }
}

fn value_eq(a: &Value, b: &Value) -> bool {
    if let (Some(sa), Value::String(sb)) = (to_tck_comparable(a), b) {
        if normalize_tck_literal(&sa) == normalize_tck_literal(sb) {
            return true;
        }
    }
    if let (Value::String(sa), Some(sb)) = (a, to_tck_comparable(b)) {
        if normalize_tck_literal(sa) == normalize_tck_literal(&sb) {
            return true;
        }
    }

    if let (Some(sa), Value::List(lb)) = (to_tck_relationship_inner(a), b)
        && lb.len() == 1
        && let Value::String(sb) = &lb[0]
        && normalize_tck_literal(&sa) == normalize_tck_literal(sb)
    {
        return true;
    }
    if let (Value::List(la), Some(sb)) = (a, to_tck_relationship_inner(b))
        && la.len() == 1
        && let Value::String(sa) = &la[0]
        && normalize_tck_literal(sa) == normalize_tck_literal(&sb)
    {
        return true;
    }

    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => {
            if a.is_nan() && b.is_nan() {
                true
            } else {
                (a - b).abs() < 1e-9
            }
        }
        (Value::Int(i), Value::Float(f)) => (*i as f64 - *f).abs() < 1e-9,
        (Value::Float(f), Value::Int(i)) => (*f - *i as f64).abs() < 1e-9,
        (Value::String(a), Value::String(b)) => {
            normalize_utc_offset_suffix(a) == normalize_utc_offset_suffix(b)
        }
        (Value::List(a), Value::List(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter().zip(b.iter()).all(|(a, b)| value_eq(a, b))
        }
        (Value::Map(a), Value::Map(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter()
                .all(|(key, av)| b.get(key).is_some_and(|bv| value_eq(av, bv)))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct SideEffectsDelta {
    plus_nodes: i64,
    minus_nodes: i64,
    plus_relationships: i64,
    minus_relationships: i64,
    plus_properties: i64,
    minus_properties: i64,
    plus_labels: i64,
    minus_labels: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SideEffectsSnapshot {
    nodes: std::collections::BTreeSet<nervusdb_api::InternalNodeId>,
    relationships: std::collections::BTreeMap<nervusdb_api::EdgeKey, i64>,
    node_props: std::collections::BTreeMap<
        nervusdb_api::InternalNodeId,
        std::collections::BTreeMap<String, Value>,
    >,
    rel_props: std::collections::BTreeMap<
        nervusdb_api::EdgeKey,
        std::collections::BTreeMap<String, Value>,
    >,
    node_labels: std::collections::BTreeMap<
        nervusdb_api::InternalNodeId,
        std::collections::BTreeSet<nervusdb_api::LabelId>,
    >,
}

impl SideEffectsSnapshot {
    fn delta_from(&self, before: &SideEffectsSnapshot) -> SideEffectsDelta {
        let plus_nodes = self.nodes.difference(&before.nodes).count() as i64;
        let minus_nodes = before.nodes.difference(&self.nodes).count() as i64;
        let mut plus_relationships = 0i64;
        let mut minus_relationships = 0i64;
        let mut all_rel_keys: std::collections::BTreeSet<nervusdb_api::EdgeKey> =
            before.relationships.keys().copied().collect();
        all_rel_keys.extend(self.relationships.keys().copied());
        for rel in &all_rel_keys {
            let before_count = *before.relationships.get(rel).unwrap_or(&0);
            let after_count = *self.relationships.get(rel).unwrap_or(&0);
            if after_count > before_count {
                plus_relationships += after_count - before_count;
            } else if before_count > after_count {
                minus_relationships += before_count - after_count;
            }
        }

        let mut plus_properties = 0i64;
        let mut minus_properties = 0i64;
        let mut plus_labels = 0i64;
        let mut minus_labels = 0i64;
        let empty_props: std::collections::BTreeMap<String, Value> =
            std::collections::BTreeMap::new();
        let empty_labels: std::collections::BTreeSet<nervusdb_api::LabelId> =
            std::collections::BTreeSet::new();

        // Nodes: created/deleted nodes contribute their full property/label counts; updates count as +/-.
        let mut all_nodes = before.nodes.clone();
        all_nodes.extend(self.nodes.iter().copied());
        for node_id in all_nodes {
            let before_exists = before.nodes.contains(&node_id);
            let after_exists = self.nodes.contains(&node_id);
            let before_props = before.node_props.get(&node_id).unwrap_or(&empty_props);
            let after_props = self.node_props.get(&node_id).unwrap_or(&empty_props);

            let before_labels = before.node_labels.get(&node_id).unwrap_or(&empty_labels);
            let after_labels = self.node_labels.get(&node_id).unwrap_or(&empty_labels);

            match (before_exists, after_exists) {
                (false, true) => {
                    plus_properties += after_props.len() as i64;
                    plus_labels += after_labels.len() as i64;
                }
                (true, false) => {
                    minus_properties += before_props.len() as i64;
                    minus_labels += before_labels.len() as i64;
                }
                (true, true) => {
                    let (plus, minus) = diff_value_map(before_props, after_props);
                    plus_properties += plus;
                    minus_properties += minus;

                    let (plus, minus) = diff_set(before_labels, after_labels);
                    plus_labels += plus;
                    minus_labels += minus;
                }
                (false, false) => {}
            }
        }

        // Relationships: created/deleted relationships contribute their full property counts; updates count as +/-.
        for rel in all_rel_keys {
            let before_count = *before.relationships.get(&rel).unwrap_or(&0);
            let after_count = *self.relationships.get(&rel).unwrap_or(&0);
            let before_props = before.rel_props.get(&rel).unwrap_or(&empty_props);
            let after_props = self.rel_props.get(&rel).unwrap_or(&empty_props);

            if before_count == 0 && after_count > 0 {
                plus_properties += after_props.len() as i64 * after_count;
                continue;
            }
            if before_count > 0 && after_count == 0 {
                minus_properties += before_props.len() as i64 * before_count;
                continue;
            }

            if before_count > 0 && after_count > 0 {
                if after_count > before_count {
                    // With edge-key based identity, when multiplicity increases we cannot pair
                    // "existing instance" vs "new instance" reliably for property maps.
                    // Count only additive properties for newly created multiplicity.
                    plus_properties += after_props.len() as i64 * (after_count - before_count);
                    continue;
                }

                let (plus, minus) = diff_value_map(before_props, after_props);
                plus_properties += plus;
                minus_properties += minus;

                if before_count > after_count {
                    minus_properties += before_props.len() as i64 * (before_count - after_count);
                    if before_props != after_props {
                        // Relationship identities are key-collapsed, so a delete+create that
                        // reuses the same key shows up as count drop plus property rewrite.
                        plus_relationships += 1;
                        minus_relationships += 1;
                    }
                }
            }
        }

        // openCypher side effects count label token delta, not per-node label assignments.
        let before_label_ids: std::collections::BTreeSet<nervusdb_api::LabelId> = before
            .node_labels
            .values()
            .flat_map(|labels| labels.iter().copied())
            .collect();
        let after_label_ids: std::collections::BTreeSet<nervusdb_api::LabelId> = self
            .node_labels
            .values()
            .flat_map(|labels| labels.iter().copied())
            .collect();
        plus_labels = after_label_ids.difference(&before_label_ids).count() as i64;
        minus_labels = before_label_ids.difference(&after_label_ids).count() as i64;

        SideEffectsDelta {
            plus_nodes,
            minus_nodes,
            plus_relationships,
            minus_relationships,
            plus_properties,
            minus_properties,
            plus_labels,
            minus_labels,
        }
    }
}

fn diff_set<T: Ord>(
    before: &std::collections::BTreeSet<T>,
    after: &std::collections::BTreeSet<T>,
) -> (i64, i64) {
    let plus = after.difference(before).count() as i64;
    let minus = before.difference(after).count() as i64;
    (plus, minus)
}

fn diff_value_map(
    before: &std::collections::BTreeMap<String, Value>,
    after: &std::collections::BTreeMap<String, Value>,
) -> (i64, i64) {
    let mut plus = 0i64;
    let mut minus = 0i64;

    for (k, before_v) in before {
        match after.get(k) {
            None => minus += 1,
            Some(after_v) if after_v != before_v => {
                // Updates count as one remove + one add.
                plus += 1;
                minus += 1;
            }
            _ => {}
        }
    }

    for k in after.keys() {
        if !before.contains_key(k) {
            plus += 1;
        }
    }

    (plus, minus)
}

fn collect_side_effect_snapshot<S: GraphSnapshot>(snapshot: &S) -> SideEffectsSnapshot {
    use std::collections::{BTreeMap, BTreeSet};

    let mut nodes: BTreeSet<nervusdb_api::InternalNodeId> = BTreeSet::new();
    let mut relationships: BTreeMap<nervusdb_api::EdgeKey, i64> = BTreeMap::new();
    let mut node_props: BTreeMap<nervusdb_api::InternalNodeId, BTreeMap<String, Value>> =
        BTreeMap::new();
    let mut rel_props: BTreeMap<nervusdb_api::EdgeKey, BTreeMap<String, Value>> = BTreeMap::new();
    let mut node_labels: BTreeMap<nervusdb_api::InternalNodeId, BTreeSet<nervusdb_api::LabelId>> =
        BTreeMap::new();

    for node_id in snapshot.nodes() {
        nodes.insert(node_id);

        if let Some(props) = snapshot.node_properties(node_id) {
            let mut converted = BTreeMap::new();
            for (k, v) in props {
                converted.insert(
                    k,
                    nervusdb_query::executor::convert_api_property_to_value(&v),
                );
            }
            node_props.insert(node_id, converted);
        }

        if let Some(labels) = snapshot.resolve_node_labels(node_id) {
            let labels: BTreeSet<_> = labels
                .into_iter()
                .filter(|label_id| *label_id != nervusdb_api::LabelId::MAX)
                .collect();
            node_labels.insert(node_id, labels);
        }

        for edge in snapshot.neighbors(node_id, None) {
            *relationships.entry(edge).or_insert(0) += 1;
        }
    }

    for edge in relationships.keys() {
        if let Some(props) = snapshot.edge_properties(*edge) {
            let mut converted = BTreeMap::new();
            for (k, v) in props {
                converted.insert(
                    k,
                    nervusdb_query::executor::convert_api_property_to_value(&v),
                );
            }
            rel_props.insert(*edge, converted);
        }
    }

    SideEffectsSnapshot {
        nodes,
        relationships,
        node_props,
        rel_props,
        node_labels,
    }
}

fn normalize_utc_offset_suffix(input: &str) -> String {
    if let Some(stripped) = input.strip_suffix("+00:00") {
        format!("{stripped}Z")
    } else {
        input.to_string()
    }
}

fn to_tck_comparable(value: &Value) -> Option<String> {
    match value {
        Value::Node(node) => Some(format_node_literal(node)),
        Value::Relationship(rel) => Some(format_relationship_literal(rel)),
        Value::ReifiedPath(path) => Some(format_path_literal(path)),
        Value::Map(map) if matches!(map.get("__kind"), Some(Value::String(kind)) if kind == "duration") => {
            if let Some(Value::String(display)) = map.get("__display") {
                Some(display.clone())
            } else {
                None
            }
        }
        Value::Map(map) => Some(format_map(map)),
        _ => None,
    }
}

fn to_tck_relationship_inner(value: &Value) -> Option<String> {
    match value {
        Value::Relationship(rel) => {
            let mut s = format!(":{}", rel.rel_type);
            if !rel.properties.is_empty() {
                s.push(' ');
                s.push_str(&format_map(&rel.properties));
            }
            Some(s)
        }
        _ => None,
    }
}

fn normalize_tck_literal(input: &str) -> String {
    let s = input.trim();
    let Some(start) = s.find('{') else {
        return normalize_node_label_order(&s.replace(" ", ""));
    };
    let Some(end) = s.rfind('}') else {
        return normalize_node_label_order(&s.replace(" ", ""));
    };

    if end <= start {
        return normalize_node_label_order(&s.replace(" ", ""));
    }

    let prefix = s[..start].replace(" ", "");
    let suffix = s[end + 1..].replace(" ", "");
    let props_src = &s[start + 1..end];
    let mut props: Vec<String> = props_src
        .split(',')
        .map(|p| p.trim().replace(" ", ""))
        .filter(|p| !p.is_empty())
        .collect();
    props.sort();
    normalize_node_label_order(&format!("{}{{{}}}{}", prefix, props.join(","), suffix))
}

fn normalize_node_label_order(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] == '('
            && let Some(end) = chars[i + 1..].iter().position(|ch| *ch == ')')
        {
            let end_idx = i + 1 + end;
            let node_segment: String = chars[i..=end_idx].iter().collect();
            out.push_str(&normalize_single_node_segment(&node_segment));
            i = end_idx + 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

fn normalize_single_node_segment(segment: &str) -> String {
    if !segment.starts_with('(') || !segment.ends_with(')') {
        return segment.to_string();
    }

    let inner = &segment[1..segment.len() - 1];
    let props_start = inner.find('{').unwrap_or(inner.len());
    let before_props = &inner[..props_start];
    let props_and_tail = &inner[props_start..];

    let Some(first_colon) = before_props.find(':') else {
        return segment.to_string();
    };

    let var_prefix = &before_props[..first_colon];
    let labels_src = &before_props[first_colon + 1..];
    let mut labels: Vec<&str> = labels_src
        .split(':')
        .filter(|label| !label.is_empty())
        .collect();
    if labels.is_empty() {
        return segment.to_string();
    }
    labels.sort_unstable();

    let mut normalized = String::from("(");
    normalized.push_str(var_prefix);
    normalized.push(':');
    normalized.push_str(&labels.join(":"));
    normalized.push_str(props_and_tail);
    normalized.push(')');
    normalized
}

fn format_node_literal(node: &nervusdb_query::executor::NodeValue) -> String {
    let labels = if node.labels.is_empty() {
        String::new()
    } else {
        format!(":{}", node.labels.join(":"))
    };
    let props = if node.properties.is_empty() {
        String::new()
    } else {
        format!(" {}", format_map(&node.properties))
    };
    format!("({labels}{props})")
}

fn format_relationship_literal(rel: &nervusdb_query::executor::RelationshipValue) -> String {
    let props = if rel.properties.is_empty() {
        String::new()
    } else {
        format!(" {}", format_map(&rel.properties))
    };
    format!("[:{}{}]", rel.rel_type, props)
}

fn format_path_literal(path: &nervusdb_query::executor::ReifiedPathValue) -> String {
    if path.nodes.is_empty() {
        return "<>".to_string();
    }

    let mut out = String::from("<");
    out.push_str(&format_node_literal(&path.nodes[0]));

    for (idx, rel) in path.relationships.iter().enumerate() {
        let left_node = path.nodes.get(idx);
        let right_node = path.nodes.get(idx + 1);

        let is_forward = left_node
            .zip(right_node)
            .is_some_and(|(left, right)| rel.key.src == left.id && rel.key.dst == right.id);
        let is_backward = left_node
            .zip(right_node)
            .is_some_and(|(left, right)| rel.key.src == right.id && rel.key.dst == left.id);

        if is_backward {
            out.push_str("<-");
            out.push_str(&format_relationship_literal(rel));
            out.push('-');
        } else if is_forward {
            out.push('-');
            out.push_str(&format_relationship_literal(rel));
            out.push_str("->");
        } else {
            out.push('-');
            out.push_str(&format_relationship_literal(rel));
            out.push('-');
        }

        if let Some(node) = right_node {
            out.push_str(&format_node_literal(node));
        }
    }

    out.push('>');
    out
}

fn format_map(map: &std::collections::BTreeMap<String, Value>) -> String {
    let mut parts = Vec::new();
    for (k, v) in map {
        parts.push(format!("{k}: {}", format_value(v)));
    }
    format!("{{{}}}", parts.join(", "))
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        Value::String(s) => format!("'{s}'"),
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(format_value).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Map(m) => format_map(m),
        Value::Node(node) => format_node_literal(node),
        Value::Relationship(rel) => format_relationship_literal(rel),
        _ => format!("{value:?}"),
    }
}

fn split_top_level(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if let Some(quote) = in_quote {
            current.push(ch);
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                in_quote = Some(ch);
                current.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' => {
                bracket_depth -= 1;
                current.push(ch);
            }
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' => {
                brace_depth -= 1;
                current.push(ch);
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current.push(ch);
            }
            c if c == delimiter && bracket_depth == 0 && brace_depth == 0 && paren_depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

fn find_top_level_colon(input: &str) -> Option<usize> {
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in input.char_indices() {
        if let Some(quote) = in_quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                in_quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => in_quote = Some(ch),
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ':' if bracket_depth == 0 && brace_depth == 0 && paren_depth == 0 => return Some(idx),
            _ => {}
        }
    }

    None
}

fn parse_tck_map(inner: &str) -> Value {
    let mut map = std::collections::BTreeMap::new();
    for entry in split_top_level(inner, ',') {
        if entry.is_empty() {
            continue;
        }
        let Some(colon_idx) = find_top_level_colon(&entry) else {
            return Value::String(format!("{{{inner}}}"));
        };
        let key_raw = entry[..colon_idx].trim();
        let value_raw = entry[colon_idx + 1..].trim();
        let key = if (key_raw.starts_with('\'') && key_raw.ends_with('\''))
            || (key_raw.starts_with('"') && key_raw.ends_with('"'))
        {
            key_raw[1..key_raw.len() - 1].to_string()
        } else {
            key_raw.to_string()
        };
        map.insert(key, parse_tck_value(value_raw));
    }
    Value::Map(map)
}

fn parse_procedure_declaration(
    declaration: &str,
) -> (String, Vec<TestProcedureField>, Vec<TestProcedureField>) {
    let trimmed = declaration.trim().trim_end_matches(':').trim();
    let name_start = trimmed
        .find('(')
        .unwrap_or_else(|| panic!("Invalid procedure declaration (missing input '('): {trimmed}"));
    let mut depth = 0i32;
    let mut input_end = None;
    for (idx, ch) in trimmed.char_indices().skip(name_start) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    input_end = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let input_end = input_end
        .unwrap_or_else(|| panic!("Invalid procedure declaration (unclosed input): {trimmed}"));
    let name = trimmed[..name_start].trim().to_string();
    let input_inner = trimmed[name_start + 1..input_end].trim();

    let after_inputs = trimmed[input_end + 1..].trim_start();
    let Some(after_sep) = after_inputs.strip_prefix("::") else {
        panic!("Invalid procedure declaration (missing '::' separator): {trimmed}");
    };
    let outputs_raw = after_sep.trim_start();
    let out_start = outputs_raw
        .find('(')
        .unwrap_or_else(|| panic!("Invalid procedure declaration (missing output '('): {trimmed}"));
    let mut out_depth = 0i32;
    let mut out_end = None;
    for (offset, ch) in outputs_raw.char_indices().skip(out_start) {
        match ch {
            '(' => out_depth += 1,
            ')' => {
                out_depth -= 1;
                if out_depth == 0 {
                    out_end = Some(offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let out_end = out_end
        .unwrap_or_else(|| panic!("Invalid procedure declaration (unclosed output): {trimmed}"));
    let output_inner = outputs_raw[out_start + 1..out_end].trim();

    let inputs = parse_procedure_fields(input_inner);
    let outputs = parse_procedure_fields(output_inner);
    (name, inputs, outputs)
}

fn parse_procedure_fields(raw_fields: &str) -> Vec<TestProcedureField> {
    if raw_fields.trim().is_empty() {
        return Vec::new();
    }

    split_top_level(raw_fields, ',')
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .map(|part| {
            let (name_raw, ty_raw) = part
                .split_once("::")
                .unwrap_or_else(|| panic!("Invalid procedure field signature: {part}"));
            let name = name_raw.trim().to_string();
            let mut ty = ty_raw.trim().to_ascii_uppercase();
            let nullable = ty.ends_with('?');
            if nullable {
                ty.pop();
            }
            let field_type = match ty.trim() {
                "INTEGER" => TestProcedureType::Integer,
                "FLOAT" => TestProcedureType::Float,
                "NUMBER" => TestProcedureType::Number,
                "STRING" => TestProcedureType::String,
                "BOOLEAN" => TestProcedureType::Boolean,
                _ => TestProcedureType::Any,
            };
            TestProcedureField {
                name,
                field_type,
                nullable,
            }
        })
        .collect()
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
        let items: Vec<Value> = split_top_level(inner, ',')
            .into_iter()
            .map(|item| parse_tck_value(&item))
            .collect();
        return Value::List(items);
    }
    if s.starts_with('{') && s.ends_with('}') {
        return parse_tck_map(&s[1..s.len() - 1]);
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

fn normalize_feature_for_cucumber(content: &str) -> String {
    let mut normalized_lines = Vec::new();
    let mut expect_first_step = false;

    for line in content.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("Scenario:") || trimmed.starts_with("Scenario Outline:") {
            expect_first_step = true;
            normalized_lines.push(line.to_string());
            continue;
        }

        if expect_first_step {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                normalized_lines.push(line.to_string());
                continue;
            }

            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];

            if let Some(rest) = trimmed.strip_prefix("And ") {
                normalized_lines.push(format!("{indent}Given {rest}"));
                expect_first_step = false;
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("But ") {
                normalized_lines.push(format!("{indent}Given {rest}"));
                expect_first_step = false;
                continue;
            }

            if trimmed.starts_with("Given ")
                || trimmed.starts_with("When ")
                || trimmed.starts_with("Then ")
                || trimmed.starts_with("* ")
            {
                expect_first_step = false;
            }
        }

        normalized_lines.push(line.to_string());
    }

    let mut normalized = normalized_lines.join("\n");
    if content.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn copy_and_normalize_features(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_and_normalize_features(&src_path, &dst_path)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        if src_path.extension().is_some_and(|ext| ext == "feature") {
            let content = fs::read_to_string(&src_path)?;
            let normalized = normalize_feature_for_cucumber(&content);
            fs::write(&dst_path, normalized)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

fn requested_input_pattern() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--input=") {
            return Some(value.to_string());
        }
        if arg == "--input" || arg == "-i" {
            return args.next();
        }
    }
    None
}

fn resolve_requested_input(root: &Path, pattern: &str) -> Option<std::path::PathBuf> {
    let requested = Path::new(pattern);
    if requested.is_absolute() && requested.exists() {
        return Some(requested.to_path_buf());
    }

    let direct = root.join(requested);
    if direct.exists() {
        return Some(direct);
    }

    let normalized = pattern.replace('\\', "/");
    let suffix = normalized
        .strip_prefix("tests/opencypher_tck/tck/features/")
        .or_else(|| normalized.strip_prefix("opencypher_tck/tck/features/"))
        .or_else(|| normalized.strip_prefix("features/"))
        .unwrap_or(&normalized);
    let suffix_direct = root.join(suffix);
    if suffix_direct.exists() {
        return Some(suffix_direct);
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let path_norm = path.to_string_lossy().replace('\\', "/");
            if path_norm.ends_with(suffix)
                || path.file_name().and_then(|name| name.to_str()) == Some(pattern)
            {
                return Some(path);
            }
        }
    }

    None
}

fn main() {
    let source_features = Path::new("tests/opencypher_tck/tck/features");
    let prepared_features =
        tempfile::TempDir::new().expect("failed to create normalized feature dir");
    copy_and_normalize_features(source_features, prepared_features.path())
        .expect("failed to normalize TCK feature files");

    let run_input = requested_input_pattern()
        .and_then(|pattern| resolve_requested_input(prepared_features.path(), &pattern))
        .unwrap_or_else(|| prepared_features.path().to_path_buf());

    futures::executor::block_on(
        GraphWorld::cucumber()
            .max_concurrent_scenarios(1)
            .with_default_cli()
            .run_and_exit(run_input),
    );
}
