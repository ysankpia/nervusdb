#![cfg(all(feature = "fts", not(target_arch = "wasm32")))]

use nervusdb_core::{Fact, Options};
use std::collections::HashMap;

#[test]
fn txt_score_stale_sidecar_triggers_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("db");

    let mut db = nervusdb_core::Database::open(Options::new(&base_path)).unwrap();
    db.configure_fts_index("all_string_props").unwrap();

    let doc = db.add_fact(Fact::new("doc", "type", "Doc")).unwrap();
    db.set_node_property(doc.subject_id, r#"{"content":"hello world"}"#)
        .unwrap();
    db.flush_indexes().unwrap();

    let mut params = HashMap::new();
    params.insert("q".to_string(), serde_json::json!("hello"));
    let results = db
        .execute_query_with_params(
            "MATCH (n:Doc) RETURN txt_score(n.content, $q) AS s",
            Some(params),
        )
        .unwrap();
    assert_eq!(results.len(), 1);
    let Some(nervusdb_core::query::executor::Value::Float(score)) = results[0].get("s") else {
        panic!("Expected txt_score to return a float");
    };
    assert!(*score > 0.0);

    // Make the on-disk sidecar stale by writing new content without flushing the index.
    db.set_node_property(doc.subject_id, r#"{"content":"goodbye"}"#)
        .unwrap();
    drop(db);

    // Reopen should detect stale sidecar via meta counters and rebuild.
    let mut db = nervusdb_core::Database::open(Options::new(&base_path)).unwrap();

    let mut params = HashMap::new();
    params.insert("q".to_string(), serde_json::json!("goodbye"));
    let results = db
        .execute_query_with_params(
            "MATCH (n:Doc) RETURN txt_score(n.content, $q) AS s",
            Some(params),
        )
        .unwrap();
    assert_eq!(results.len(), 1);
    let Some(nervusdb_core::query::executor::Value::Float(score)) = results[0].get("s") else {
        panic!("Expected txt_score to return a float");
    };
    assert!(*score > 0.0);

    // Old content should no longer match after rebuild.
    let mut params = HashMap::new();
    params.insert("q".to_string(), serde_json::json!("hello"));
    let results = db
        .execute_query_with_params(
            "MATCH (n:Doc) RETURN txt_score(n.content, $q) AS s",
            Some(params),
        )
        .unwrap();
    assert_eq!(results.len(), 1);
    let Some(nervusdb_core::query::executor::Value::Float(score)) = results[0].get("s") else {
        panic!("Expected txt_score to return a float");
    };
    assert_eq!(*score, 0.0);
}
