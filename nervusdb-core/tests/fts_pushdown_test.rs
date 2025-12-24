#![cfg(all(feature = "fts", not(target_arch = "wasm32")))]

use nervusdb_core::query::parser::Parser;
use nervusdb_core::query::planner::{PhysicalPlan, QueryPlanner};
use nervusdb_core::{Database, Fact, Options};
use std::collections::HashMap;

fn contains_fts_candidate_scan(plan: &PhysicalPlan) -> bool {
    match plan {
        PhysicalPlan::FtsCandidateScan(_) => true,
        PhysicalPlan::SingleRow(_)
        | PhysicalPlan::Scan(_)
        | PhysicalPlan::VectorTopKScan(_)
        | PhysicalPlan::Create(_) => false,
        PhysicalPlan::Filter(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Project(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Limit(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Skip(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Sort(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Distinct(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Aggregate(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Expand(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::ExpandVarLength(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Unwind(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Set(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::Delete(node) => contains_fts_candidate_scan(node.input.as_ref()),
        PhysicalPlan::NestedLoopJoin(node) => {
            contains_fts_candidate_scan(node.left.as_ref())
                || contains_fts_candidate_scan(node.right.as_ref())
        }
        PhysicalPlan::LeftOuterJoin(node) => {
            contains_fts_candidate_scan(node.left.as_ref())
                || contains_fts_candidate_scan(node.right.as_ref())
        }
    }
}

#[test]
fn planner_pushes_down_txt_score_gt_zero_on_scan() {
    let query = Parser::parse("MATCH (n:Doc) WHERE txt_score(n.content, $q) > 0 RETURN n").unwrap();
    let plan = QueryPlanner::new().plan(query).unwrap();
    assert!(contains_fts_candidate_scan(&plan));
}

#[test]
fn planner_does_not_pushdown_txt_score_ge_zero() {
    let query =
        Parser::parse("MATCH (n:Doc) WHERE txt_score(n.content, $q) >= 0 RETURN n").unwrap();
    let plan = QueryPlanner::new().plan(query).unwrap();
    assert!(!contains_fts_candidate_scan(&plan));
}

#[test]
fn planner_does_not_pushdown_txt_score_parameter_threshold() {
    let query =
        Parser::parse("MATCH (n:Doc) WHERE txt_score(n.content, $q) > $t RETURN n").unwrap();
    let plan = QueryPlanner::new().plan(query).unwrap();
    assert!(!contains_fts_candidate_scan(&plan));
}

#[test]
fn candidate_scan_respects_match_labels() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("db");

    let mut db = Database::open(Options::new(&base_path)).unwrap();
    db.configure_fts_index("all_string_props").unwrap();

    let doc = db.add_fact(Fact::new("doc", "type", "Doc")).unwrap();
    db.set_node_property(doc.subject_id, r#"{"content":"hello world"}"#)
        .unwrap();

    let other = db.add_fact(Fact::new("other", "type", "Other")).unwrap();
    db.set_node_property(other.subject_id, r#"{"content":"hello world"}"#)
        .unwrap();

    db.flush_indexes().unwrap();

    let mut params = HashMap::new();
    params.insert("q".to_string(), serde_json::json!("hello"));
    let results = db
        .execute_query_with_params(
            "MATCH (n:Doc) WHERE txt_score(n.content, $q) > 0 RETURN id(n) AS id",
            Some(params),
        )
        .unwrap();

    assert_eq!(results.len(), 1);
    let Some(nervusdb_core::query::executor::Value::Float(node_id)) = results[0].get("id") else {
        panic!("expected id(n) to return a float");
    };
    assert_eq!(*node_id as u64, doc.subject_id);
}
