// 样例 3：批量写入 + fsync 契约 + 队列上限
use nervusdb::{NervusDb, Value};
use std::collections::{HashMap, HashSet};

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let db = NervusDb::open(dir.path().join("t.db"))?;

    let before = db.buffer_stats().wal_fsync_count;
    db.with_transaction(|tx| {
        for i in 0..1_000i64 {
            tx.add_node(
                HashSet::from(["Bulk".to_string()]),
                HashMap::from([("idx".to_string(), Value::from(i))]),
            )?;
        }
        Ok(())
    })?;
    assert_eq!(db.buffer_stats().wal_fsync_count - before, 1);
    Ok(())
}
