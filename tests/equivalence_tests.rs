//! v1.0.0 的**行为等价性**护栏。
//!
//! ## 这个文件为什么存在
//!
//! v1.0.0 的目标是「打磨」——不引入新行为，只让既有行为更正确、更可依赖。
//! 这类改动最容易悄悄改变语义：一个"顺手"的边界调整、一次"更合理"的默认值，
//! 都可能让用户的既有查询返回不同结果。
//!
//! 因此这里把 v1.0.0 明确承诺**不变**的语义逐条钉死。任何一条失败都意味着
//! 改动越界了，应当重新审视而不是改测试。
//!
//! 这些断言刻意写成「用户可观察的行为」，而不是内部实现细节：内部的页布局、
//! 缓存策略、索引结构都可以变，用户的查询结果与 API 语义不行。

use graphlite::{GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

fn props(i: i64) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    m.insert("i".to_string(), Value::from(i));
    m
}

/// 核心 CRUD 与查询语义在 v1.0.0 必须与之前一致。
#[test]
fn test_core_semantics_unchanged() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("core.db"))?;

    let mut ids = Vec::new();
    db.with_transaction(|tx| {
        for i in 1..=20i64 {
            let mut m = HashMap::new();
            m.insert("i".to_string(), Value::from(i));
            m.insert("name".to_string(), Value::from(format!("n{}", i)));
            ids.push(tx.add_node(HashSet::from(["N".to_string()]), m)?);
        }
        for i in 0..19 {
            tx.add_edge(ids[i], ids[i + 1], "NEXT", HashMap::new(), 1.0)?;
        }
        Ok(())
    })?;

    // 计数
    assert_eq!(db.node_count(), 20);
    assert_eq!(db.edge_count(), 19);

    // 点查
    let n = db.try_get_node(1)?.expect("node 1");
    assert_eq!(n.get_prop("name").and_then(|v| v.as_str()), Some("n1"));

    // 过滤 + 排序 + 分页：这是最容易被"顺手优化"改变的一组语义
    let r = db.run_cypher("MATCH (a:N) WHERE a.i > 15 RETURN a.i AS i ORDER BY i DESC")?;
    let got: Vec<i64> = r
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().unwrap_or(-1))
        .collect();
    assert_eq!(
        got,
        vec![20, 19, 18, 17, 16],
        "filter+sort semantics must hold"
    );

    // SKIP / LIMIT
    let r = db.run_cypher("MATCH (a:N) RETURN a.i AS i ORDER BY i ASC SKIP 2 LIMIT 3")?;
    let got: Vec<i64> = r
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().unwrap_or(-1))
        .collect();
    assert_eq!(got, vec![3, 4, 5], "skip/limit semantics must hold");

    // 聚合
    let r = db.run_cypher("MATCH (a:N) RETURN count(*) AS c, sum(a.i) AS s, avg(a.i) AS av")?;
    assert_eq!(r.rows[0].values[0].as_i64(), Some(20), "count(*)");
    assert_eq!(r.rows[0].values[1].as_i64(), Some(210), "sum");
    assert_eq!(r.rows[0].values[2].as_f64(), Some(10.5), "avg");

    // 变长路径
    let r = db.run_cypher("MATCH (a:N)-[:NEXT*1..3]->(b) RETURN count(*) AS c")?;
    assert!(r.rows[0].values[0].as_i64().unwrap_or(0) > 0);

    Ok(())
}

/// 事务语义（回滚零污染、单次 fsync）在 v1.0.0 必须一致。
#[test]
fn test_transaction_semantics_unchanged() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join("tx.db"))?;

    // 回滚不留痕迹
    let before = db.node_count();
    let res = db.with_transaction(|tx| {
        tx.add_node(HashSet::from(["X".to_string()]), props(1))?;
        Err::<(), GraphError>(GraphError::General("deliberate".into()))
    });
    assert!(res.is_err());
    assert_eq!(db.node_count(), before, "rollback must leave no nodes");

    // 批量提交只 fsync 一次
    let f0 = db.buffer_stats().wal_fsync_count;
    db.with_transaction(|tx| {
        for i in 0..500 {
            tx.add_node(HashSet::from(["Y".to_string()]), props(i))?;
        }
        Ok(())
    })?;
    let f1 = db.buffer_stats().wal_fsync_count;
    assert_eq!(f1 - f0, 1, "a batch commit must cost exactly one fsync");

    Ok(())
}

/// 公开 API 的既有签名与语义必须保持（编译期即验证，这里补充行为断言）。
#[test]
fn test_public_api_contracts_unchanged() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("api.db");

    let db = GraphLite::open(&db_path)?;
    assert!(!db.is_read_only());

    let id = db.add_node(HashSet::from(["N".to_string()]), props(1))?;
    db.add_edge(id, id, "SELF", HashMap::new(), 1.0)?;
    db.checkpoint()?;

    // lossy 与 error-preserving 两套读取器的语义分工不得改变
    assert!(
        db.get_node(id).is_some(),
        "get_node returns Some for a live node"
    );
    assert!(db.try_get_node(id)?.is_some(), "try_get_node agrees");
    assert!(db.get_node(999_999).is_none(), "get_node is lossy → None");
    assert!(
        db.try_get_node(999_999)?.is_none(),
        "try_get_node must use None only for genuine absence"
    );

    // 只读句柄的语义
    drop(db);
    let ro = GraphLite::open_read_only(&db_path)?;
    assert!(ro.is_read_only());
    assert_eq!(ro.node_count(), 1);

    // 目录导出与回灌（迁移路径）必须仍然可用且幂等
    let mut dump = Vec::new();
    ro.dump_cypher(&mut dump)?;
    let text = String::from_utf8_lossy(&dump).to_string();
    assert!(
        text.contains("CREATE"),
        "dump must contain CREATE statements"
    );

    Ok(())
}

/// 磁盘格式版本与最小库尺寸在 v1.0.0 不得变化。
#[test]
fn test_format_contract_unchanged() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("fmt.db");

    {
        let db = GraphLite::open(&db_path)?;
        db.add_node(HashSet::from(["P".to_string()]), props(1))?;
        db.checkpoint()?;
    }

    // 版本落盘为 FORMAT.md 承诺的 4
    let raw = std::fs::read(&db_path)?;
    assert_eq!(&raw[0..4], b"GLDB", "magic must stay GLDB");
    let version = u32::from_le_bytes(raw[4..8].try_into().unwrap_or([0; 4]));
    assert_eq!(
        version, 4,
        "the frozen format version must not drift without a documented bump"
    );

    // 最小库不超过 16 KiB（4 页）——这是 FORMAT.md 明确承诺的
    let size = std::fs::metadata(&db_path)?.len();
    assert!(
        size <= 16 * 1024,
        "the minimal database must stay within 16 KiB, got {} bytes",
        size
    );

    Ok(())
}
