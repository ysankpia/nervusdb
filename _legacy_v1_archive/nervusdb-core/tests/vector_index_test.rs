#![cfg(all(feature = "vector", not(target_arch = "wasm32")))]

use nervusdb_core::{Fact, Options};

fn embedding_json(v: &[f32]) -> String {
    let nums: Vec<_> = v
        .iter()
        .map(|f| serde_json::Value::from(*f as f64))
        .collect();
    serde_json::json!({ "embedding": nums }).to_string()
}

fn unit(dim: usize, idx: usize) -> Vec<f32> {
    let mut v = vec![0.0; dim];
    if idx < dim {
        v[idx] = 1.0;
    }
    v
}

#[test]
fn vector_index_search_persist_and_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("db");

    let mut db = nervusdb_core::Database::open(Options::new(&base_path)).unwrap();
    let dim = 64;
    db.configure_vector_index(dim, "embedding", "cosine")
        .unwrap();

    let n1 = db.add_fact(Fact::new("n1", "type", "Doc")).unwrap();
    let n2 = db.add_fact(Fact::new("n2", "type", "Doc")).unwrap();

    db.set_node_property(n1.subject_id, &embedding_json(&unit(dim, 0)))
        .unwrap();
    db.set_node_property(n2.subject_id, &embedding_json(&unit(dim, 1)))
        .unwrap();

    let hits = db.vector_search(&unit(dim, 0), 2).unwrap();
    assert_eq!(hits.first().map(|(id, _)| *id), Some(n1.subject_id));

    db.flush_indexes().unwrap();
    drop(db);

    // Reopen should load sidecar (or rebuild) and keep behavior.
    let db = nervusdb_core::Database::open(Options::new(&base_path)).unwrap();
    let hits = db.vector_search(&unit(dim, 0), 2).unwrap();
    assert_eq!(hits.first().map(|(id, _)| *id), Some(n1.subject_id));
    drop(db);

    // Remove sidecar to force rebuild.
    let sidecar = base_path.with_extension("redb.usearch");
    let sidecar_meta = base_path.with_extension("redb.usearch.meta.json");
    let _ = std::fs::remove_file(sidecar);
    let _ = std::fs::remove_file(sidecar_meta);

    let db = nervusdb_core::Database::open(Options::new(&base_path)).unwrap();
    let hits = db.vector_search(&unit(dim, 1), 2).unwrap();
    assert_eq!(hits.first().map(|(id, _)| *id), Some(n2.subject_id));
}

#[test]
fn vector_index_transaction_abort_and_commit() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("db");

    let mut db = nervusdb_core::Database::open(Options::new(&base_path)).unwrap();
    let dim = 64;
    db.configure_vector_index(dim, "embedding", "cosine")
        .unwrap();

    let n1 = db.add_fact(Fact::new("n1", "type", "Doc")).unwrap();
    let n2 = db.add_fact(Fact::new("n2", "type", "Doc")).unwrap();
    db.set_node_property(n1.subject_id, &embedding_json(&unit(dim, 0)))
        .unwrap();
    db.set_node_property(n2.subject_id, &embedding_json(&unit(dim, 2)))
        .unwrap();

    db.begin_transaction().unwrap();
    db.set_node_property(n1.subject_id, &embedding_json(&unit(dim, 2)))
        .unwrap();
    db.abort_transaction().unwrap();

    let hits = db.vector_search(&unit(dim, 2), 2).unwrap();
    let score_n1 = hits
        .iter()
        .find(|(id, _)| *id == n1.subject_id)
        .map(|(_, s)| *s)
        .unwrap_or(0.0);
    let score_n2 = hits
        .iter()
        .find(|(id, _)| *id == n2.subject_id)
        .map(|(_, s)| *s)
        .unwrap_or(0.0);
    assert!(score_n2 > 0.9);
    assert!(score_n1 < 0.5);

    db.begin_transaction().unwrap();
    db.set_node_property(n1.subject_id, &embedding_json(&unit(dim, 2)))
        .unwrap();
    db.commit_transaction().unwrap();

    let hits = db.vector_search(&unit(dim, 2), 2).unwrap();
    let score_n1 = hits
        .iter()
        .find(|(id, _)| *id == n1.subject_id)
        .map(|(_, s)| *s)
        .unwrap_or(0.0);
    assert!(score_n1 > 0.9);
}
