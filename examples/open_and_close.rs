// 样例 1：打开与关闭，以及「一个写者 + 任意读者」这条边界
use nervusdb::{GraphError, NervusDb, NervusDbOptions};

fn main() -> Result<(), GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("t.db");

    // 默认 4 MB 缓冲池
    let db = NervusDb::open(&p)?;

    // 写句柄存活时，只读打开会失败——写者与读者互斥。
    // 这不是限制，是防「两个写者各自以为成功、第二个写入静默丢失」。
    let err = NervusDb::open_read_only(&p)
        .err()
        .expect("写句柄存活时只读打开必须失败");
    assert!(matches!(err, GraphError::DatabaseLocked(_)), "got {err:?}");

    // 第二个写句柄同样被拒绝
    assert!(NervusDb::open(&p).is_err());

    drop(db);

    // 释放之后，可以重新打开。任一时刻只有一个句柄。
    let db = NervusDb::open_with_options(
        &p,
        NervusDbOptions {
            buffer_pool_frames: 256,
            wal_auto_checkpoint_bytes: 0,
            read_only: false,
            max_transaction_actions: 4_000_000,
            // `..Default::default()` 而不是逐字段列全：新增选项时这个示例不会
            // 因为缺一个字段而编译失败。
            ..Default::default()
        },
    )?;
    drop(db);

    let db = NervusDb::open_with_pool_mb(&p, 16)?;
    drop(db);

    // 没有写句柄时，只读打开成功
    let ro = NervusDb::open_read_only(&p)?;
    assert!(ro.is_read_only());
    Ok(())
}
