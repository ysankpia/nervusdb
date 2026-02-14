use nervusdb_query::prepare;

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
fn t332_variable_type_conflict_node_then_relationship() {
    assert_compile_error_contains("MATCH (r) MATCH ()-[r]-() RETURN r", "VariableTypeConflict");
}

#[test]
fn t332_variable_type_conflict_scalar_then_relationship() {
    assert_compile_error_contains(
        "WITH 1 AS r MATCH ()-[r]-() RETURN r",
        "VariableTypeConflict",
    );
}

#[test]
fn t332_variable_already_bound_path_variable() {
    assert_compile_error_contains(
        "WITH 1 AS p MATCH p = ()-[]-() RETURN p",
        "VariableAlreadyBound",
    );
}

#[test]
fn t332_variable_type_conflict_duplicate_in_same_pattern() {
    assert_compile_error_contains("MATCH (r)-[r]-() RETURN r", "VariableTypeConflict");
}

#[test]
fn t332_allows_reusing_node_variable_across_match_clauses() {
    let query = "MATCH (a)-[]-() WITH a MATCH (a)-[]-() RETURN a";
    prepare(query).expect("reusing node variables across MATCH clauses should be valid");
}

#[test]
fn t332_prepare_unicode_input_does_not_panic() {
    let outcome = std::panic::catch_unwind(|| prepare("!¡ 🃁"));
    assert!(
        outcome.is_ok(),
        "prepare() should not panic on unicode input"
    );
}

#[test]
fn t332_where_expression_rejects_undefined_variable() {
    assert_compile_error_contains(
        "MATCH (s) WHERE s.name = undefinedVariable AND s.age = 10 RETURN s",
        "UndefinedVariable",
    );
}
