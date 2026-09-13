// 样例 4：显式事务
use nervusdb::NervusDb;
use std::collections::HashMap;

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let db = NervusDb::open(dir.path().join("t.db"))?;

    let mut tx = db.begin_transaction()?;
    let a = tx.add_node(Default::default(), HashMap::new())?;
    let b = tx.add_node(Default::default(), HashMap::new())?;
    tx.add_edge(a, b, "LINK", HashMap::new(), 1.0)?;
    tx.commit()?;
    Ok(())
}
