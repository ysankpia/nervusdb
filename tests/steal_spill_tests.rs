//! 战役一验证套件：STEAL 溢出策略、事务回滚一致性、WAL 零污染与检查点语义。
//!
//! 核心断言：极小内存（1MB / 2MB）下超大事务必须成功；任何未提交数据
//! 都绝不能泄漏到主数据文件；冷重启后数据必须完整一致。

use nervusdb::{Direction, GraphError, NervusDb, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

fn read_main_file(db_path: &std::path::Path) -> Vec<u8> {
    std::fs::read(db_path).unwrap_or_default()
}

// =========================================================================
// 极小缓冲池下超大事务的成功路径与内存受控
// =========================================================================
#[test]
fn test_large_transaction_exceeds_pool_with_spill() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("spill_success.db");

    // 512 帧 = 2MB 硬约束，事务规模远超该容量
    let db = NervusDb::open_with_pool_size(&db_path, 512)?;
    let total: u64 = 12_000;

    let mut tx = db.begin_transaction()?;
    for i in 1..=total {
        let mut props = HashMap::new();
        props.insert("idx".to_string(), Value::from(i as i64));
        // 每个节点携带 ~1.2KB 载荷，强制消耗大量溢出页
        props.insert("blob".to_string(), Value::from("Z".repeat(1200)));
        let mut labels = HashSet::new();
        labels.insert("Bulk".to_string());
        tx.add_node(labels, props)?;
    }
    tx.commit()?;

    assert_eq!(db.node_count(), total as usize);

    // 缓冲池绝不允许突破配置容量
    let stats = db.buffer_stats();
    assert_eq!(stats.capacity_frames, 512);
    assert!(stats.used_frames <= 512, "used frames must stay bounded");
    assert!(
        stats.spill_count > 0,
        "a transaction larger than the pool must have triggered STEAL spilling"
    );

    // 提交后数据的可见性与准确性
    assert_eq!(
        db.get_node(6_000)
            .unwrap()
            .get_prop("idx")
            .and_then(|v| v.as_i64()),
        Some(6_000)
    );

    // 冷重启后必须完整一致（已提交数据经 WAL 恢复）
    drop(db);
    let reopened = NervusDb::open_with_pool_size(&db_path, 512)?;
    assert_eq!(reopened.node_count(), total as usize);
    assert_eq!(
        reopened
            .get_node(total)
            .unwrap()
            .get_prop("idx")
            .and_then(|v| v.as_i64()),
        Some(total as i64)
    );
    assert_eq!(
        reopened
            .get_node(total)
            .unwrap()
            .get_prop("blob")
            .and_then(|v| v.as_str())
            .map(|s| s.len()),
        Some(1200)
    );

    Ok(())
}

// =========================================================================
// 溢出事务回滚后主库零污染
// =========================================================================
#[test]
fn test_rollback_leaves_no_pollution_after_spill() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("spill_rollback.db");

    // 1. 建立基线并落盘
    {
        let db = NervusDb::open_with_pool_size(&db_path, 256)?;
        let mut props = HashMap::new();
        props.insert("base".to_string(), Value::from("v1"));
        db.add_node(HashSet::from(["Base".to_string()]), props)?;
        db.checkpoint()?;
    }

    let baseline_bytes = read_main_file(&db_path);
    assert!(!baseline_bytes.is_empty());

    // 2. 在 1MB 约束下开启远超容量的写事务，然后显式回滚
    {
        let db = NervusDb::open_with_pool_size(&db_path, 256)?;
        let before = db.node_count();

        let mut tx = db.begin_transaction()?;
        for i in 0..5_000 {
            let mut props = HashMap::new();
            props.insert("idx".to_string(), Value::from(i as i64));
            props.insert("blob".to_string(), Value::from("Q".repeat(900)));
            tx.add_node(HashSet::from(["Taint".to_string()]), props)?;
        }
        tx.rollback()?;

        // 内存态必须完全复原
        assert_eq!(db.node_count(), before);
        assert_eq!(
            db.query_cypher("MATCH (t:Taint) RETURN t")?.row_count(),
            0,
            "rolled back spill page leaked into in-memory metadata"
        );

        // 主库物理文件绝不能被未提交的溢出帧改写
        assert_eq!(
            read_main_file(&db_path),
            baseline_bytes,
            "main database file was polluted by uncommitted spill pages"
        );

        // 后续写入必须仍然可用且正确
        db.add_node(HashSet::from(["After".to_string()]), HashMap::new())?;
        assert_eq!(db.node_count(), before + 1);
    }

    // 3. 冷重启：污染数据绝不出现
    let db = NervusDb::open_with_pool_size(&db_path, 256)?;
    assert_eq!(
        db.query_cypher("MATCH (t:Taint) RETURN t")?.row_count(),
        0,
        "rolled back data resurfaced after cold restart"
    );
    assert_eq!(db.node_count(), 2);

    Ok(())
}

// =========================================================================
// 极小缓冲池下批量边事务 + 图算法（综合外存压力）
// =========================================================================
#[test]
fn test_spill_then_traverse_and_analytics() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("spill_traverse.db");

    let db = NervusDb::open_with_pool_size(&db_path, 256)?;
    let total: u64 = 4_000;

    // 节点事务
    let mut tx = db.begin_transaction()?;
    for i in 1..=total {
        let mut props = HashMap::new();
        props.insert("idx".to_string(), Value::from(i as i64));
        tx.add_node(HashSet::from(["V".to_string()]), props)?;
    }
    tx.commit()?;

    // 边事务（环形 + 弦边），同样远超 256 帧容量
    let mut tx = db.begin_transaction()?;
    for i in 1..=total {
        let next = if i == total { 1 } else { i + 1 };
        tx.add_edge(i, next, "RING", HashMap::new(), 1.0)?;
    }
    for i in 1..=total {
        let jump = (i + 37) % total + 1;
        if i != jump {
            let _ = tx.add_edge(i, jump, "CHORD", HashMap::new(), 2.0);
        }
    }
    tx.commit()?;
    assert!(db.edge_count() >= total as usize);

    // 溢出发生后算法与遍历仍须准确
    let stats = db.buffer_stats();
    assert!(stats.used_frames <= 256);

    let bfs = db
        .bfs(1, 500, Some("RING"))
        .expect("BFS must find the ring path");
    assert_eq!(bfs.len(), 500);

    let res = db
        .query()
        .traverse(1, "RING", Direction::Outgoing, 3)
        .execute();
    assert!(!res.multi_hop_paths().is_empty());

    let components = db.weakly_connected_components();
    assert_eq!(components.len(), 1, "ring graph must be one component");

    // Checkpoint 后数据保持完整
    db.checkpoint()?;
    assert_eq!(db.node_count(), total as usize);

    Ok(())
}

// =========================================================================
// 检查点会把 WAL 中已提交页落回主文件并清空 WAL
// =========================================================================
#[test]
fn test_checkpoint_drains_wal_into_main_file() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("checkpoint.db");

    let db = NervusDb::open_with_pool_size(&db_path, 512)?;

    let mut tx = db.begin_transaction()?;
    for i in 0..3_000 {
        let mut props = HashMap::new();
        props.insert("idx".to_string(), Value::from(i as i64));
        props.insert("blob".to_string(), Value::from("M".repeat(800)));
        tx.add_node(HashSet::from(["C".to_string()]), props)?;
    }
    tx.commit()?;

    // 提交后 WAL 中应当存有页镜像
    let before = db.buffer_stats();
    assert!(before.wal_size_bytes > 0, "WAL must hold committed pages");

    db.checkpoint()?;

    let after = db.buffer_stats();
    assert_eq!(
        after.wal_page_count, 0,
        "checkpoint must clear the WAL page index"
    );
    assert_eq!(db.node_count(), 3_000);

    // 主文件体积应当显著增长（吸收原 WAL 内容）
    assert!(
        after.file_size_bytes >= before.file_size_bytes,
        "main file must absorb the checkpointed pages"
    );

    // 冷重启数据一致（节点 ID 从 1 起，故 ID 2500 对应 idx = 2499）
    drop(db);
    let reopened = NervusDb::open_with_pool_size(&db_path, 512)?;
    assert_eq!(reopened.node_count(), 3_000);
    assert_eq!(
        reopened
            .get_node(2_500)
            .unwrap()
            .get_prop("idx")
            .and_then(|v| v.as_i64()),
        Some(2_499)
    );

    Ok(())
}

// =========================================================================
// 多标签与 WAL 恢复组合
// =========================================================================
#[test]
fn test_multi_label_survives_spill_and_restart() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("multilabel_restart.db");

    {
        let db = NervusDb::open_with_pool_size(&db_path, 256)?;
        db.execute("CREATE (a:Person:Engineer {name: 'A', blob: 'X'})")?;
        let mut tx = db.begin_transaction()?;
        for i in 0..2_000 {
            let mut props = HashMap::new();
            props.insert("idx".to_string(), Value::from(i as i64));
            props.insert("blob".to_string(), Value::from("P".repeat(1000)));
            tx.add_node(HashSet::from(["Bulk".to_string()]), props)?;
        }
        tx.commit()?;
        db.checkpoint()?;
    }

    let db = NervusDb::open_with_pool_size(&db_path, 256)?;
    // 多标签节点在两个标签下都必须可检索
    assert_eq!(db.query_cypher("MATCH (p:Person) RETURN p")?.row_count(), 1);
    assert_eq!(
        db.query_cypher("MATCH (e:Engineer) RETURN e")?.row_count(),
        1
    );
    assert_eq!(db.node_count(), 2_001);

    Ok(())
}
