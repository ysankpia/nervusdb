#![cfg(all(feature = "vector", not(target_arch = "wasm32")))]

use nervusdb_core::query::parser::Parser;
use nervusdb_core::query::planner::{PhysicalPlan, QueryPlanner};
use nervusdb_core::{Database, Fact, Options};
use std::collections::HashMap;

fn contains_vector_topk_scan(plan: &PhysicalPlan) -> bool {
    match plan {
        PhysicalPlan::VectorTopKScan(_) => true,
        PhysicalPlan::Filter(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Project(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Limit(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Skip(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Sort(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Distinct(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Aggregate(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Expand(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::ExpandVarLength(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Unwind(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Set(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::Delete(node) => contains_vector_topk_scan(node.input.as_ref()),
        PhysicalPlan::NestedLoopJoin(node) => {
            contains_vector_topk_scan(node.left.as_ref())
                || contains_vector_topk_scan(node.right.as_ref())
        }
        PhysicalPlan::LeftOuterJoin(node) => {
            contains_vector_topk_scan(node.left.as_ref())
                || contains_vector_topk_scan(node.right.as_ref())
        }
        _ => false,
    }
}

fn embedding_json(v: &[f32]) -> String {
    let nums: Vec<_> = v
        .iter()
        .map(|f| serde_json::Value::from(*f as f64))
        .collect();
    serde_json::json!({ "embedding": nums }).to_string()
}

#[test]
fn planner_pushes_down_vec_similarity_sort_limit_on_global_scan() {
    let query =
        Parser::parse("MATCH (n) RETURN n ORDER BY vec_similarity(n.embedding, $q) DESC LIMIT 10")
            .unwrap();
    let plan = QueryPlanner::new().plan(query).unwrap();
    assert!(contains_vector_topk_scan(&plan));
}

#[test]
fn planner_does_not_pushdown_without_limit() {
    let query =
        Parser::parse("MATCH (n) RETURN n ORDER BY vec_similarity(n.embedding, $q) DESC").unwrap();
    let plan = QueryPlanner::new().plan(query).unwrap();
    assert!(!contains_vector_topk_scan(&plan));
}

#[test]
fn planner_does_not_pushdown_for_ascending_order() {
    let query =
        Parser::parse("MATCH (n) RETURN n ORDER BY vec_similarity(n.embedding, $q) ASC LIMIT 10")
            .unwrap();
    let plan = QueryPlanner::new().plan(query).unwrap();
    assert!(!contains_vector_topk_scan(&plan));
}

#[test]
fn planner_does_not_pushdown_for_labeled_scan() {
    let query = Parser::parse(
        "MATCH (n:Doc) RETURN n ORDER BY vec_similarity(n.embedding, $q) DESC LIMIT 10",
    )
    .unwrap();
    let plan = QueryPlanner::new().plan(query).unwrap();
    assert!(!contains_vector_topk_scan(&plan));
}

#[test]
fn vector_topk_pushdown_returns_results_without_index_configured() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("db");

    let mut db = Database::open(Options::new(&base_path)).unwrap();

    let n1 = db.add_fact(Fact::new("n1", "type", "Doc")).unwrap();
    let n2 = db.add_fact(Fact::new("n2", "type", "Doc")).unwrap();

    db.set_node_property(n1.subject_id, &embedding_json(&[1.0, 0.0, 0.0, 0.0]))
        .unwrap();
    db.set_node_property(n2.subject_id, &embedding_json(&[0.8, 0.6, 0.0, 0.0]))
        .unwrap();

    let mut params = HashMap::new();
    params.insert("q".to_string(), serde_json::json!([1.0, 0.0, 0.0, 0.0]));

    let results = db
        .execute_query_with_params(
            "MATCH (n) RETURN n ORDER BY vec_similarity(n.embedding, $q) DESC LIMIT 2",
            Some(params),
        )
        .unwrap();

    assert_eq!(results.len(), 2);
    let Some(nervusdb_core::query::executor::Value::Node(id0)) = results[0].get("n") else {
        panic!("expected n to be a Node id");
    };
    assert_eq!(*id0, n1.subject_id);
}

#[test]
fn vector_topk_pushdown_works_with_index_configured() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("db");

    let mut db = Database::open(Options::new(&base_path)).unwrap();
    db.configure_vector_index(4, "embedding", "cosine").unwrap();

    let n1 = db.add_fact(Fact::new("n1", "type", "Doc")).unwrap();
    let n2 = db.add_fact(Fact::new("n2", "type", "Doc")).unwrap();
    let n3 = db.add_fact(Fact::new("n3", "type", "Doc")).unwrap();

    db.set_node_property(n1.subject_id, &embedding_json(&[1.0, 0.0, 0.0, 0.0]))
        .unwrap();
    db.set_node_property(n2.subject_id, &embedding_json(&[0.8, 0.6, 0.0, 0.0]))
        .unwrap();
    db.set_node_property(n3.subject_id, &embedding_json(&[0.0, 1.0, 0.0, 0.0]))
        .unwrap();

    db.flush_indexes().unwrap();

    let mut params = HashMap::new();
    params.insert("q".to_string(), serde_json::json!([1.0, 0.0, 0.0, 0.0]));

    let results = db
        .execute_query_with_params(
            "MATCH (n) RETURN n ORDER BY vec_similarity(n.embedding, $q) DESC LIMIT 2",
            Some(params),
        )
        .unwrap();

    assert_eq!(results.len(), 2);
    let Some(nervusdb_core::query::executor::Value::Node(id0)) = results[0].get("n") else {
        panic!("expected n to be a Node id");
    };
    let Some(nervusdb_core::query::executor::Value::Node(id1)) = results[1].get("n") else {
        panic!("expected n to be a Node id");
    };
    assert_eq!(*id0, n1.subject_id);
    assert_eq!(*id1, n2.subject_id);
}

#[test]
fn vector_topk_pushdown_falls_back_on_dim_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("db");

    let mut db = Database::open(Options::new(&base_path)).unwrap();
    db.configure_vector_index(4, "embedding", "cosine").unwrap();

    let n1 = db.add_fact(Fact::new("n1", "type", "Doc")).unwrap();
    let n2 = db.add_fact(Fact::new("n2", "type", "Doc")).unwrap();

    db.set_node_property(n1.subject_id, &embedding_json(&[1.0, 0.0, 0.0, 0.0]))
        .unwrap();
    db.set_node_property(n2.subject_id, &embedding_json(&[0.0, 1.0, 0.0, 0.0]))
        .unwrap();

    db.flush_indexes().unwrap();

    let mut params = HashMap::new();
    params.insert("q".to_string(), serde_json::json!([1.0, 0.0, 0.0])); // wrong dim

    let results = db
        .execute_query_with_params(
            "MATCH (n) RETURN n ORDER BY vec_similarity(n.embedding, $q) DESC LIMIT 1",
            Some(params),
        )
        .unwrap();

    assert_eq!(results.len(), 1);
    let Some(nervusdb_core::query::executor::Value::Node(_)) = results[0].get("n") else {
        panic!("expected n to be a Node id");
    };
}
