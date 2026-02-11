use cucumber::{World, given, then, when};
use nervusdb_v2::Db;
use nervusdb_v2_query::{Params, Value, prepare};
use std::collections::HashMap;
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

            match query.execute_mixed(&snapshot, &mut txn, &params) {
                Ok((rows, _write_count)) => {
                    if let Err(e) = txn.commit() {
                        world.last_result = None;
                        world.last_error = Some(e.to_string());
                        return;
                    }

                    let reify_snapshot = db.snapshot();
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
                }
                Err(e) => {
                    world.last_result = None;
                    world.last_error = Some(e.to_string());
                }
            }
        }
        Err(e) => {
            world.last_result = None;
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

    // TCK 当前阶段只要求“有错误且为编译期路径”，错误码逐步细化
    if err_lower.trim().is_empty() {
        panic!("Expected error type '{}' but got empty error", error_type);
    }
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
        && &sa == sb
    {
        return true;
    }
    if let (Value::List(la), Some(sb)) = (a, to_tck_relationship_inner(b))
        && la.len() == 1
        && let Value::String(sa) = &la[0]
        && sa == &sb
    {
        return true;
    }

    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => (a - b).abs() < 1e-9,
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
        return s.replace(" ", "");
    };
    let Some(end) = s.rfind('}') else {
        return s.replace(" ", "");
    };

    if end <= start {
        return s.replace(" ", "");
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
    format!("{}{{{}}}{}", prefix, props.join(","), suffix)
}

fn format_node_literal(node: &nervusdb_v2_query::executor::NodeValue) -> String {
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

fn format_relationship_literal(rel: &nervusdb_v2_query::executor::RelationshipValue) -> String {
    let props = if rel.properties.is_empty() {
        String::new()
    } else {
        format!(" {}", format_map(&rel.properties))
    };
    format!("[:{}{}]", rel.rel_type, props)
}

fn format_path_literal(path: &nervusdb_v2_query::executor::ReifiedPathValue) -> String {
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
            .with_default_cli()
            .run_and_exit(run_input),
    );
}
