// 样例 7：自洽读（快照）
use nervusdb::NervusDb;

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let db = NervusDb::open(dir.path().join("t.db"))?;
    db.run_cypher("CREATE (a:N)-[:R]->(b:N)")?;

    let snapshot = db.read_snapshot();
    if let Some(node) = snapshot.get_node(1)? {
        for eid in &node.outgoing {
            assert!(snapshot.get_edge(*eid)?.is_some());
        }
    }
    drop(snapshot);
    Ok(())
}
