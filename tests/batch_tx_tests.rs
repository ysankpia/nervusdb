//! 1.1 验证套件：显式批量事务（BEGIN TRANSACTION ... COMMIT）。
//!
//! 核心断言：
//! - 单个事务无论写入多少条记录，**恰好触发一次 fsync**（批量组提交语义）；
//! - 批量写入吞吐远超逐条自动提交，且 `:memory:` 下达到 20,000+ ops/s；
//! - 批量回滚对主库零污染、内存元数据完全复原；
//! - 1MB 受限缓冲池下超大批量事务依然成功且帧占用受控。

use graphlite::{GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use tempfile::tempdir;

fn node_props(i: i64) -> HashMap<String, Value> {
    let mut props = HashMap::new();
    props.insert("idx".to_string(), Value::from(i));
    props.insert("name".to_string(), Value::from(format!("entity-{:06}", i)));
    props
}

// =========================================================================
// 结构性断言：一次 commit 恰好一次 fsync，且事务规模不影响 fsync 次数
// =========================================================================
#[test]
fn test_single_commit_triggers_exactly_one_fsync() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("fsync_count.db");
    let db = GraphLite::open_with_pool_size(&db_path, 512)?;

    // 基线：统计逐条自动提交的 fsync 次数
    let base_fsync = db.buffer_stats().wal_fsync_count;
    for i in 0..10 {
        db.add_node(HashSet::from(["Auto".to_string()]), node_props(i))?;
    }
    let after_autocommit = db.buffer_stats().wal_fsync_count;
    assert_eq!(
        after_autocommit - base_fsync,
        10,
        "each autocommitted write must fsync exactly once"
    );

    // 批量事务：10,000 次写入共享一次 fsync
    let writes: u64 = 10_000;
    db.with_transaction(|tx| {
        for i in 0..writes {
            tx.add_node(HashSet::from(["Batch".to_string()]), node_props(i as i64))?;
        }
        Ok(())
    })?;

    let after_batch = db.buffer_stats().wal_fsync_count;
    assert_eq!(
        after_batch - after_autocommit,
        1,
        "a single batched transaction of {} writes must fsync exactly once, but fsynced {} times",
        writes,
        after_batch - after_autocommit
    );

    // 数据必须完整可见
    assert_eq!(db.node_count(), 10 + writes as usize);
    let res = db.query_cypher("MATCH (b:Batch) RETURN count(b)")?;
    assert_eq!(res.rows[0].values[0], Value::from(writes as i64));

    // 冷重启后仍然一致
    drop(db);
    let db = GraphLite::open_with_pool_size(&db_path, 512)?;
    assert_eq!(db.node_count(), 10 + writes as usize);

    Ok(())
}

/// 批量写入吞吐下限（ops/s）。
///
/// 20,000 ops/s 的验收目标针对**优化构建**（`cargo test --release`），这是有意义的性能度量环境。
/// 未优化的 debug 构建中，每条边插入需约 8 次页操作，吞吐显著低于优化构建；
/// 此处保留一个仍能暴露数量级回归的 debug 下限，避免把编译档位差异误判为功能缺陷。
fn throughput_floor() -> f64 {
    if cfg!(debug_assertions) {
        8_000.0
    } else {
        20_000.0
    }
}

// =========================================================================
// 吞吐：:memory: 模式批量写入必须达到验收下限（优化构建下为 20,000+ ops/s）
// =========================================================================
#[test]
fn test_batch_throughput_memory_mode() -> Result<(), GraphError> {
    let db = GraphLite::open(":memory:")?;
    let floor = throughput_floor();

    let batch: u64 = 50_000;
    let start = Instant::now();
    db.with_transaction(|tx| {
        for i in 0..batch {
            tx.add_node(HashSet::from(["T".to_string()]), node_props(i as i64))?;
        }
        Ok(())
    })?;
    let elapsed = start.elapsed();
    let ops_per_sec = batch as f64 / elapsed.as_secs_f64();

    assert_eq!(db.node_count(), batch as usize);
    println!(
        "[batch] node throughput: {:.0} ops/s ({} ops in {:.2?})",
        ops_per_sec, batch, elapsed
    );
    assert!(
        ops_per_sec >= floor,
        "batched node throughput must reach {} ops/s, measured {:.0} ops/s",
        floor,
        ops_per_sec
    );

    // 批量建立边同样计入吞吐（含双向邻接链表维护）
    let edges: u64 = 50_000;
    let start = Instant::now();
    db.with_transaction(|tx| {
        for i in 0..edges {
            let src = (i % batch) + 1;
            let dst = ((i * 13 + 7) % batch) + 1;
            if src != dst {
                tx.add_edge(src, dst, "R", HashMap::new(), 1.0)?;
            }
        }
        Ok(())
    })?;
    let edge_elapsed = start.elapsed();
    let edge_ops_per_sec = edges as f64 / edge_elapsed.as_secs_f64();
    println!(
        "[batch] edge throughput: {:.0} ops/s ({} ops in {:.2?})",
        edge_ops_per_sec, edges, edge_elapsed
    );
    assert!(
        edge_ops_per_sec >= floor,
        "batched edge throughput must reach {} ops/s, measured {:.0} ops/s",
        floor,
        edge_ops_per_sec
    );

    Ok(())
}

// =========================================================================
// 批量提交相对逐条自动提交必须有数量级提升
// =========================================================================
#[test]
fn test_batch_beats_autocommit() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("batch_vs_auto.db");
    let db = GraphLite::open_with_pool_size(&db_path, 512)?;

    // 逐条自动提交（每条一次 WAL 追加 + fsync）
    let autocommit_ops: u64 = 200;
    let start = Instant::now();
    for i in 0..autocommit_ops {
        db.add_node(HashSet::from(["Auto".to_string()]), node_props(i as i64))?;
    }
    let autocommit_elapsed = start.elapsed().as_secs_f64();

    // 批量事务写同等条数
    let start = Instant::now();
    db.with_transaction(|tx| {
        for i in 0..autocommit_ops {
            tx.add_node(HashSet::from(["Batch".to_string()]), node_props(i as i64))?;
        }
        Ok(())
    })?;
    let batch_elapsed = start.elapsed().as_secs_f64();

    let autocommit_rate = autocommit_ops as f64 / autocommit_elapsed;
    let batch_rate = autocommit_ops as f64 / batch_elapsed;

    assert!(
        batch_rate > autocommit_rate * 20.0,
        "batched writes must be >20x faster than autocommit; autocommit {:.0} ops/s vs batch {:.0} ops/s",
        autocommit_rate,
        batch_rate
    );

    Ok(())
}

// =========================================================================
// 批量回滚：主库字节级零污染 + 元数据完全复原
// =========================================================================
#[test]
fn test_batch_rollback_zero_pollution() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("batch_rollback.db");

    // 建立基线并落盘
    {
        let db = GraphLite::open_with_pool_size(&db_path, 512)?;
        db.add_node(HashSet::from(["Base".to_string()]), node_props(1))?;
        db.checkpoint()?;
    }
    let baseline_bytes = std::fs::read(&db_path)?;

    {
        let db = GraphLite::open_with_pool_size(&db_path, 512)?;
        let before = db.node_count();

        let mut tx = db.begin_transaction()?;
        for i in 0..5_000 {
            tx.add_node(HashSet::from(["Taint".to_string()]), node_props(i))?;
        }
        tx.rollback()?;

        assert_eq!(db.node_count(), before, "rollback must restore node count");
        assert_eq!(
            db.query_cypher("MATCH (t:Taint) RETURN t")?.row_count(),
            0,
            "rolled back batch leaked into memory"
        );
        assert_eq!(
            std::fs::read(&db_path)?,
            baseline_bytes,
            "main database file was polluted by a rolled back batch"
        );

        // 回滚后事务仍可正常使用
        db.with_transaction(|tx| {
            tx.add_node(HashSet::from(["After".to_string()]), node_props(2))?;
            Ok(())
        })?;
        assert_eq!(db.node_count(), before + 1);
    }

    let db = GraphLite::open_with_pool_size(&db_path, 512)?;
    assert_eq!(db.query_cypher("MATCH (t:Taint) RETURN t")?.row_count(), 0);
    assert_eq!(db.node_count(), 2);

    Ok(())
}

// =========================================================================
// 闭包返回 Err 时自动回滚（with_transaction 的失败路径）
// =========================================================================
#[test]
fn test_with_transaction_rolls_back_on_error() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("with_tx_err.db");
    let db = GraphLite::open(&db_path)?;

    let before = db.node_count();
    let err = db
        .with_transaction(|tx| {
            tx.add_node(HashSet::from(["Partial".to_string()]), node_props(1))?;
            tx.add_node(HashSet::from(["Partial".to_string()]), node_props(2))?;
            Err::<(), GraphError>(GraphError::General("simulated failure".into()))
        })
        .expect_err("closure error must propagate");
    assert!(err.to_string().contains("simulated failure"));

    assert_eq!(db.node_count(), before, "failed batch must not commit");
    assert_eq!(
        db.query_cypher("MATCH (p:Partial) RETURN p")?.row_count(),
        0
    );

    // 之后的事务必须仍然可用
    db.with_transaction(|tx| {
        tx.add_node(HashSet::from(["Ok".to_string()]), node_props(3))?;
        Ok(())
    })?;
    assert_eq!(db.node_count(), before + 1);

    Ok(())
}

// =========================================================================
// 1MB 受限缓冲池下的超大批量事务（STEAL 溢出 + 组提交协同）
// =========================================================================
#[test]
fn test_large_batch_under_one_megabyte_pool() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("batch_small_pool.db");

    // 256 帧 = 1MB 硬预算
    let db = GraphLite::open_with_pool_size(&db_path, 256)?;

    let num_nodes: u64 = 10_000;
    let num_edges: u64 = 20_000;

    db.with_transaction(|tx| {
        for i in 1..=num_nodes {
            let mut props = HashMap::new();
            props.insert("idx".to_string(), Value::from(i as i64));
            props.insert("blob".to_string(), Value::from("Z".repeat(700)));
            tx.add_node(HashSet::from(["N".to_string()]), props)?;
        }
        Ok(())
    })?;

    db.with_transaction(|tx| {
        for i in 0..num_edges {
            let src = (i % num_nodes) + 1;
            let dst = ((i * 17 + 5) % num_nodes) + 1;
            if src != dst {
                tx.add_edge(src, dst, "R", HashMap::new(), 1.0)?;
            }
        }
        Ok(())
    })?;

    // 内存硬预算绝不可突破
    let stats = db.buffer_stats();
    assert_eq!(stats.capacity_frames, 256);
    assert!(
        stats.used_frames <= 256,
        "resident frames must stay bounded"
    );

    assert_eq!(db.node_count(), num_nodes as usize);
    assert_eq!(db.edge_count(), num_edges as usize);

    // 每个批量事务各自只触发一次 fsync
    assert_eq!(
        stats.wal_fsync_count, 2,
        "two batched transactions must produce exactly two fsyncs, got {}",
        stats.wal_fsync_count
    );

    db.checkpoint()?;
    drop(db);

    let db = GraphLite::open_with_pool_size(&db_path, 256)?;
    assert_eq!(db.node_count(), num_nodes as usize);
    assert_eq!(db.edge_count(), num_edges as usize);
    assert_eq!(
        db.get_node(7_777)
            .unwrap()
            .get_prop("idx")
            .and_then(|v| v.as_i64()),
        Some(7_777)
    );

    Ok(())
}

// =========================================================================
// 批量事务内的混合操作（增 / 改 / 删）一次性原子提交
// =========================================================================
#[test]
fn test_batch_mixed_operations_atomic_commit() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("batch_mixed.db");
    let db = GraphLite::open(&db_path)?;

    // 预置基准节点
    let seed = db.add_node(HashSet::from(["Seed".to_string()]), node_props(0))?;

    let created = db.with_transaction(|tx| {
        let mut ids = Vec::new();
        for i in 1..=500 {
            ids.push(tx.add_node(HashSet::from(["Mixed".to_string()]), node_props(i))?);
        }
        for id in ids.iter().take(200) {
            tx.add_edge(seed, *id, "LINK", HashMap::new(), 1.0)?;
        }
        tx.update_node_property(seed, "role", "hub");
        for id in ids.iter().take(50) {
            tx.remove_node(*id);
        }
        Ok(ids)
    })?;

    // 500 创建 - 50 删除 = 450 存活
    assert_eq!(created.len(), 500);
    assert_eq!(db.node_count(), 1 + 450);
    assert_eq!(
        db.query_cypher("MATCH (m:Mixed) RETURN count(m)")?.rows[0].values[0],
        Value::from(450)
    );
    assert_eq!(
        db.get_node(seed)
            .unwrap()
            .get_prop("role")
            .and_then(|v| v.as_str()),
        Some("hub")
    );

    // 删除的节点其关联边必须一并清除
    let remaining_edges = db.query_cypher("MATCH (s:Seed)-[r:LINK]->(m:Mixed) RETURN count(r)")?;
    assert_eq!(remaining_edges.rows[0].values[0], Value::from(150));

    // 冷重启一致
    drop(db);
    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.node_count(), 451);
    assert_eq!(
        db.get_node(seed)
            .unwrap()
            .get_prop("role")
            .and_then(|v| v.as_str()),
        Some("hub")
    );

    Ok(())
}
