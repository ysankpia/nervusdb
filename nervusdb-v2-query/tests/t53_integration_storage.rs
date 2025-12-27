use nervusdb_v2_api::GraphStore;
use nervusdb_v2_query::{Params, Result, Value, prepare};
use nervusdb_v2_storage::engine::GraphEngine;
use tempfile::tempdir;

#[test]
fn t53_end_to_end_v2_storage_plus_query() {
    let dir = tempdir().unwrap();
    let ndb = dir.path().join("graph.ndb");
    let wal = dir.path().join("graph.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();
    let (a, b) = {
        let mut tx = engine.begin_write();
        let a = tx.create_node(10, 1).unwrap();
        let b = tx.create_node(20, 1).unwrap();
        tx.create_edge(a, 7, b);
        tx.commit().unwrap();
        (a, b)
    };

    let snap = engine.snapshot();
    let q = prepare("MATCH (n)-[:7]->(m) RETURN n, m LIMIT 10").unwrap();
    let rows: Vec<_> = q
        .execute_streaming(&snap, &Params::new())
        .collect::<Result<_>>()
        .unwrap();

    assert_eq!(rows.len(), 1);
    let cols = rows[0].columns();
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0].0, "n");
    assert_eq!(cols[1].0, "m");
    assert_eq!(cols[0].1, Value::NodeId(a));
    assert_eq!(cols[1].1, Value::NodeId(b));
}
