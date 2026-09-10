//! 边织网局部性与批量事务专项验证。
//!
//! 覆盖：
//! - 批量织网与逐条插入产出**完全相同的图结构**（出边/入边集合与边实体）；
//! - 自环、重复边、跨批次延续等边界场景的链完整性；
//! - 受限内存下假溢出（STEAL）与缓存抖动的消减；
//! - 图算法（PageRank / WCC / K-Hop）在批量写入后的精度一致性；
//! - 批量织网失败时的原子回滚与主库零污染；
//! - 多级页目录页在受限池下的常驻与深度寻址正确性。

use graphlite::{Direction, GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

/// 节点出边集合（公开 API 观测面）
fn outgoing_set(db: &GraphLite, node_id: u64) -> HashSet<u64> {
    db.get_node(node_id)
        .map(|n| n.outgoing.iter().copied().collect())
        .unwrap_or_default()
}

/// 节点入边集合（公开 API 观测面）
fn incoming_set(db: &GraphLite, node_id: u64) -> HashSet<u64> {
    db.get_node(node_id)
        .map(|n| n.incoming.iter().copied().collect())
        .unwrap_or_default()
}

// =========================================================================
// 1. 批量织网必须与逐条插入产出完全相同的图结构
// =========================================================================
#[test]
fn test_batch_weave_matches_per_edge_insert() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let batch_path = dir.path().join("weave_batch.db");
    let single_path = dir.path().join("weave_single.db");

    // 同一批边请求（含自环、重复边、扇入扇出）
    let requests: Vec<(u64, u64)> = {
        let mut v = Vec::new();
        for i in 0..500u64 {
            v.push((1, i % 7 + 2)); // 单源扇出
            v.push((i % 7 + 2, 1)); // 单目标扇入
            v.push((i % 50 + 2, (i * 3) % 50 + 2));
        }
        v.push((9, 9)); // 自环
        v.push((10, 11));
        v.push((10, 11)); // 重复边
        v
    };

    // A. 批量路径（单个事务内连续 AddEdge，走两阶段批量织网）
    {
        let db = GraphLite::open(&batch_path)?;
        db.with_transaction(|tx| {
            for _ in 1..=60u64 {
                tx.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
            }
            Ok(())
        })?;
        db.with_transaction(|tx| {
            for &(s, d) in &requests {
                tx.add_edge(s, d, "R", HashMap::new(), 1.0)?;
            }
            Ok(())
        })?;
        db.checkpoint()?;
    }

    // B. 逐条路径（每条边独立事务，强制走单条插入分支）
    {
        let db = GraphLite::open(&single_path)?;
        db.with_transaction(|tx| {
            for _ in 1..=60u64 {
                tx.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
            }
            Ok(())
        })?;
        for &(s, d) in &requests {
            let mut tx = db.begin_transaction()?;
            tx.add_edge(s, d, "R", HashMap::new(), 1.0)?;
            tx.commit()?;
        }
        db.checkpoint()?;
    }

    let batch_db = GraphLite::open(&batch_path)?;
    let single_db = GraphLite::open(&single_path)?;

    assert_eq!(batch_db.edge_count(), single_db.edge_count());
    assert_eq!(batch_db.node_count(), single_db.node_count());

    // 逐节点比对出边与入边集合
    for nid in 1..=60u64 {
        assert_eq!(
            outgoing_set(&batch_db, nid),
            outgoing_set(&single_db, nid),
            "outgoing set mismatch at node {}",
            nid
        );
        assert_eq!(
            incoming_set(&batch_db, nid),
            incoming_set(&single_db, nid),
            "incoming set mismatch at node {}",
            nid
        );
    }

    // 逐边比对端点与类型
    for eid in 1..=batch_db.edge_count() as u64 {
        let a = batch_db.get_edge(eid).expect("batch edge must exist");
        let b = single_db.get_edge(eid).expect("single edge must exist");
        assert_eq!(a.src_id, b.src_id);
        assert_eq!(a.dst_id, b.dst_id);
        assert_eq!(a.edge_type, b.edge_type);
    }

    Ok(())
}

// =========================================================================
// 2. 自环不得破坏出边链表头
// =========================================================================
#[test]
fn test_self_loop_preserves_outgoing_chain() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("self_loop.db");
    let db = GraphLite::open(&db_path)?;

    let hub = db.add_node(HashSet::from(["Hub".to_string()]), HashMap::new())?;
    let other = db.add_node(HashSet::from(["Hub".to_string()]), HashMap::new())?;

    // 批次内混合：普通边 + 自环 + 再普通边
    db.with_transaction(|tx| {
        tx.add_edge(hub, other, "R", HashMap::new(), 1.0)?;
        tx.add_edge(hub, hub, "R", HashMap::new(), 1.0)?;
        tx.add_edge(hub, other, "R", HashMap::new(), 1.0)?;
        Ok(())
    })?;

    let hub_node = db.get_node(hub).expect("hub must exist");
    assert_eq!(
        hub_node.outgoing.len(),
        3,
        "self-loop must not drop any outgoing edge"
    );
    assert!(
        !hub_node.incoming.is_empty(),
        "self-loop must also register as an incoming edge"
    );

    // 自环仍可被方向性查询命中
    let res = db.query_cypher("MATCH (a:Hub)-[:R]->(a:Hub) RETURN a")?;
    assert_eq!(res.row_count(), 1);

    Ok(())
}

// =========================================================================
// 3. 跨批次延续：后续批次必须正确挂接到既有链头
// =========================================================================
#[test]
fn test_batch_append_across_transactions() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("append.db");
    let db = GraphLite::open(&db_path)?;

    let src = db.add_node(HashSet::from(["S".to_string()]), HashMap::new())?;
    let mut dsts = Vec::new();
    for _ in 0..200 {
        dsts.push(db.add_node(HashSet::from(["D".to_string()]), HashMap::new())?);
    }

    // 三个连续批次，每次 70 条边（超过批量织网阈值）
    for round in 0..3usize {
        db.with_transaction(|tx| {
            for i in 0..70usize {
                let d = dsts[(round * 70 + i) % dsts.len()];
                tx.add_edge(src, d, "R", HashMap::new(), 1.0)?;
            }
            Ok(())
        })?;
    }

    let node = db.get_node(src).expect("src must exist");
    assert_eq!(
        node.outgoing.len(),
        210,
        "all edges across batches must stay reachable from the source"
    );

    db.checkpoint()?;
    let reopened = GraphLite::open(&db_path)?;
    assert_eq!(
        reopened.get_node(src).unwrap().outgoing.len(),
        210,
        "chain must survive reopen"
    );
    assert_eq!(reopened.edge_count(), 210);

    Ok(())
}

// =========================================================================
// 4. 受限内存下：假溢出（STEAL）必须被批量织网消除
// =========================================================================
#[test]
fn test_batch_weave_eliminates_false_spill() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("no_false_spill.db");

    // 极小池：256 帧 = 1MB；节点热集远超池容量
    let db = GraphLite::open_with_pool_size(&db_path, 256)?;

    let nodes: u64 = 40_000;
    db.with_transaction(|tx| {
        for _ in 1..=nodes {
            tx.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
        }
        Ok(())
    })?;

    let before = db.buffer_stats();

    // 80k 条高度离散的边：每条 src/dst 跨越多张节点页
    let edges: u64 = 80_000;
    db.with_transaction(|tx| {
        for i in 0..edges {
            let src = ((i * 7919) % nodes) + 1;
            let dst = ((i * 104_729 + 13) % nodes) + 1;
            tx.add_edge(src, dst, "R", HashMap::new(), 1.0)?;
        }
        Ok(())
    })?;

    let after = db.buffer_stats();
    let spills = after.spill_count - before.spill_count;
    let miss_ratio = (after.cache_misses - before.cache_misses) as f64 / edges as f64;

    // 逐条头插在此剖面下约 2.0 次 spill/边；批量织网后应远低于 0.1 次/边。
    assert!(
        (spills as f64 / edges as f64) < 0.1,
        "false spill not eliminated: {:.3} spills/edge ({} total)",
        spills as f64 / edges as f64,
        spills
    );
    assert!(
        miss_ratio < 0.6,
        "cache misses too high: {:.3} misses/edge",
        miss_ratio
    );

    // 内存硬预算不可突破
    assert_eq!(after.capacity_frames, 256);
    assert!(after.used_frames <= 256);

    assert_eq!(db.edge_count(), edges as usize);

    Ok(())
}

// =========================================================================
// 5. 批量写入后的图算法精度必须与逐条插入一致
// =========================================================================
#[test]
fn test_analytics_identical_after_batch_weave() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let batch_path = dir.path().join("algo_batch.db");
    let single_path = dir.path().join("algo_single.db");

    let node_count = 120u64;
    let edge_list: Vec<(u64, u64, f64)> = (0..400u64)
        .map(|i| {
            let s = (i * 7919) % node_count + 1;
            let d = (i * 104_729 + 13) % node_count + 1;
            (s, d, 1.0 + (i % 5) as f64)
        })
        .collect();

    for (path, batched) in [(&batch_path, true), (&single_path, false)] {
        let db = GraphLite::open(path)?;
        db.with_transaction(|tx| {
            for n in 1..=node_count {
                let mut m = HashMap::new();
                m.insert("idx".to_string(), Value::from(n as i64));
                tx.add_node(HashSet::from(["A".to_string()]), m)?;
            }
            Ok(())
        })?;
        if batched {
            db.with_transaction(|tx| {
                for &(s, d, w) in &edge_list {
                    tx.add_edge(s, d, "R", HashMap::new(), w)?;
                }
                Ok(())
            })?;
        } else {
            for &(s, d, w) in &edge_list {
                let mut tx = db.begin_transaction()?;
                tx.add_edge(s, d, "R", HashMap::new(), w)?;
                tx.commit()?;
            }
        }
        db.checkpoint()?;
    }

    let batch_db = GraphLite::open(&batch_path)?;
    let single_db = GraphLite::open(&single_path)?;

    // PageRank 分数必须逐位一致
    let a = batch_db.pagerank_with(0.85, 200, 1e-12);
    let b = single_db.pagerank_with(0.85, 200, 1e-12);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.node_id, y.node_id);
        assert!(
            (x.score - y.score).abs() < 1e-15,
            "PageRank diverged at node {}: {} vs {}",
            x.node_id,
            x.score,
            y.score
        );
    }

    // 弱连通分量必须完全一致
    let ca = batch_db.weakly_connected_components();
    let cb = single_db.weakly_connected_components();
    assert_eq!(ca.len(), cb.len());
    for (x, y) in ca.iter().zip(cb.iter()) {
        assert_eq!(x, y, "WCC diverged");
    }

    // K-Hop 子图必须完全一致
    let ka = batch_db.k_hop_subgraph_with(1, 2, Direction::Both, Some("R"))?;
    let kb = single_db.k_hop_subgraph_with(1, 2, Direction::Both, Some("R"))?;
    assert_eq!(ka.nodes, kb.nodes, "K-Hop node set diverged");
    assert_eq!(
        ka.edges.iter().map(|e| e.id).collect::<Vec<_>>(),
        kb.edges.iter().map(|e| e.id).collect::<Vec<_>>(),
        "K-Hop edge set diverged"
    );

    Ok(())
}

// =========================================================================
// 6. 批量织网失败必须原子回滚，主库零污染
// =========================================================================
#[test]
fn test_batch_weave_rollback_zero_pollution() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("weave_rollback.db");

    {
        let db = GraphLite::open(&db_path)?;
        db.with_transaction(|tx| {
            for _ in 1..=130u64 {
                tx.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
            }
            Ok(())
        })?;
        db.checkpoint()?;
    }
    let baseline = std::fs::read(&db_path)?;

    {
        let db = GraphLite::open(&db_path)?;
        let before_edges = db.edge_count();

        // 批量织网中混入非法负权重边 -> 整批必须失败并干净回滚
        let mut tx = db.begin_transaction()?;
        for i in 0..100u64 {
            tx.add_edge(i % 130 + 1, (i * 3) % 130 + 1, "R", HashMap::new(), 1.0)?;
        }
        tx.add_edge(1, 2, "BAD", HashMap::new(), -5.0)?;
        let err = tx
            .commit()
            .expect_err("negative weight must fail the whole batch");
        assert!(err.to_string().contains("weight") || err.to_string().contains("InvalidWeight"));

        assert_eq!(db.edge_count(), before_edges, "rolled back edges leaked");
        assert_eq!(
            std::fs::read(&db_path)?,
            baseline,
            "main file polluted by rolled back batch weave"
        );
    }

    let db = GraphLite::open(&db_path)?;
    assert_eq!(db.edge_count(), 0);
    assert_eq!(db.node_count(), 130);

    Ok(())
}

// =========================================================================
// 7. 多级页目录页在受限池下的常驻与深度寻址正确性
// =========================================================================
#[test]
fn test_directory_pages_stay_resident() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("dir_resident.db");

    // 极小池 + 超过直接页槽位的节点规模，强制分配间接目录页
    let db = GraphLite::open_with_pool_size(&db_path, 256)?;
    let nodes: u64 = 20_000; // 20k/128 = 157 张节点页 > 32 张直接页 -> 需要目录页

    db.with_transaction(|tx| {
        for i in 1..=nodes {
            let mut m = HashMap::new();
            m.insert("idx".to_string(), Value::from(i as i64));
            tx.add_node(HashSet::from(["N".to_string()]), m)?;
        }
        Ok(())
    })?;

    assert_eq!(db.node_count(), nodes as usize);
    // 深度寻址（落在间接目录页覆盖区）必须正确
    assert_eq!(
        db.get_node(nodes)
            .unwrap()
            .get_prop("idx")
            .and_then(|v| v.as_i64()),
        Some(nodes as i64)
    );

    db.checkpoint()?;
    let reopened = GraphLite::open_with_pool_size(&db_path, 256)?;
    assert_eq!(reopened.node_count(), nodes as usize);
    assert_eq!(
        reopened
            .get_node(19_999)
            .unwrap()
            .get_prop("idx")
            .and_then(|v| v.as_i64()),
        Some(19_999)
    );

    Ok(())
}
