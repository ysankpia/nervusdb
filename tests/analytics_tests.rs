//! 战役三验证套件：企业级图算法引擎（PageRank / 弱连通分量 / K-Hop 子图）。

use graphlite::{Direction, GraphError, GraphLite, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

fn open_temp(name: &str) -> Result<(tempfile::TempDir, GraphLite), GraphError> {
    let dir = tempdir()?;
    let db = GraphLite::open(dir.path().join(name))?;
    Ok((dir, db))
}

// =========================================================================
// PageRank：星型图上枢纽节点必须显著高于叶子节点，且分数归一
// =========================================================================
#[test]
fn test_pagerank_star_topology() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("pagerank_star.db")?;

    // 中心枢纽 hub 被 6 个叶子节点指向
    let hub = db.add_node(HashSet::new(), HashMap::new())?;
    let mut leaves = Vec::new();
    for _ in 0..6 {
        let leaf = db.add_node(HashSet::new(), HashMap::new())?;
        db.add_edge(leaf, hub, "POINTS_TO", HashMap::new(), 1.0)?;
        leaves.push(leaf);
    }

    let scores = db.pagerank_with(0.85, 100, 1e-10);
    assert_eq!(scores.len(), 7);

    // 分数总和必须归一为 1.0
    let total: f64 = scores.iter().map(|s| s.score).sum();
    assert!(
        (total - 1.0).abs() < 1e-6,
        "PageRank scores must sum to 1.0, got {}",
        total
    );

    // 枢纽必须排第一，且严格高于所有叶子
    assert_eq!(scores[0].node_id, hub, "hub must rank first");
    for leaf in &leaves {
        let leaf_score = scores
            .iter()
            .find(|s| s.node_id == *leaf)
            .expect("leaf must be scored");
        assert!(
            scores[0].score > leaf_score.score,
            "hub score {} must exceed leaf score {}",
            scores[0].score,
            leaf_score.score
        );
    }

    // 阻尼因子取 0.0 时所有节点均匀分布
    let uniform = db.pagerank_with(0.0, 50, 1e-12);
    for s in &uniform {
        assert!(
            (s.score - 1.0 / 7.0).abs() < 1e-6,
            "damping 0 must yield uniform distribution, got {}",
            s.score
        );
    }

    Ok(())
}

#[test]
fn test_pagerank_chain_and_isolated() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("pagerank_chain.db")?;

    // 链 A -> B -> C，加一个孤立节点 D
    let a = db.add_node(HashSet::new(), HashMap::new())?;
    let b = db.add_node(HashSet::new(), HashMap::new())?;
    let c = db.add_node(HashSet::new(), HashMap::new())?;
    let d = db.add_node(HashSet::new(), HashMap::new())?;
    let e = db.add_node(HashSet::new(), HashMap::new())?;

    db.add_edge(a, b, "NEXT", HashMap::new(), 1.0)?;
    db.add_edge(b, c, "NEXT", HashMap::new(), 1.0)?;
    db.add_edge(e, a, "NEXT", HashMap::new(), 1.0)?;

    let scores = db.pagerank_with(0.85, 200, 1e-12);
    assert_eq!(scores.len(), 5);

    let score_of = |id: u64| scores.iter().find(|s| s.node_id == id).unwrap().score;

    // 链式 DAG 中越靠后越重要（排名沿入边链路累积）
    assert!(score_of(c) > score_of(b), "C must outrank B");
    assert!(score_of(b) > score_of(a), "B must outrank A");

    // A 拥有一条入边 E->A，D 完全无入边；二者均无出边，故 A 必须严格高于 D
    assert!(
        score_of(a) > score_of(d),
        "A (has inbound edge) must outrank fully isolated D: {} vs {}",
        score_of(a),
        score_of(d)
    );

    let total: f64 = scores.iter().map(|s| s.score).sum();
    assert!(
        (total - 1.0).abs() < 1e-6,
        "PageRank scores must sum to 1.0, got {}",
        total
    );

    Ok(())
}

// =========================================================================
// 弱连通分量：多孤岛划分 + 无向语义
// =========================================================================
#[test]
fn test_weakly_connected_components() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("wcc.db")?;

    // 分量 1：1 -> 2 -> 3（有向链，视为无向连通）
    let n1 = db.add_node(HashSet::new(), HashMap::new())?;
    let n2 = db.add_node(HashSet::new(), HashMap::new())?;
    let n3 = db.add_node(HashSet::new(), HashMap::new())?;
    db.add_edge(n1, n2, "R", HashMap::new(), 1.0)?;
    db.add_edge(n2, n3, "R", HashMap::new(), 1.0)?;

    // 分量 2：4 <- 5（有向反向）
    let n4 = db.add_node(HashSet::new(), HashMap::new())?;
    let n5 = db.add_node(HashSet::new(), HashMap::new())?;
    db.add_edge(n5, n4, "R", HashMap::new(), 1.0)?;

    // 分量 3：孤立节点 6
    let n6 = db.add_node(HashSet::new(), HashMap::new())?;

    let components = db.weakly_connected_components();
    assert_eq!(components.len(), 3, "expected 3 components");

    // 按分量规模降序：{1,2,3} 在前
    assert_eq!(components[0], vec![n1, n2, n3]);
    assert_eq!(components[1], vec![n4, n5]);
    assert_eq!(components[2], vec![n6]);

    Ok(())
}

#[test]
fn test_wcc_single_component_and_empty() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("wcc_single.db")?;

    // 空图
    assert!(db.weakly_connected_components().is_empty());

    // 环 A -> B -> C -> A 必须归为单一分量
    let a = db.add_node(HashSet::new(), HashMap::new())?;
    let b = db.add_node(HashSet::new(), HashMap::new())?;
    let c = db.add_node(HashSet::new(), HashMap::new())?;
    db.add_edge(a, b, "R", HashMap::new(), 1.0)?;
    db.add_edge(b, c, "R", HashMap::new(), 1.0)?;
    db.add_edge(c, a, "R", HashMap::new(), 1.0)?;

    let components = db.weakly_connected_components();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0], vec![a, b, c]);

    Ok(())
}

// =========================================================================
// K-Hop 子图提取
// =========================================================================
#[test]
fn test_k_hop_subgraph() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("khop.db")?;

    // 双分支结构: hub -> l1, hub -> l2, l1 -> g1, l2 -> g2, g1 -> gg1, 另有孤岛 far
    let hub = db.add_node(HashSet::from(["Hub".to_string()]), HashMap::new())?;
    let l1 = db.add_node(HashSet::from(["Leaf".to_string()]), HashMap::new())?;
    let l2 = db.add_node(HashSet::from(["Leaf".to_string()]), HashMap::new())?;
    let g1 = db.add_node(HashSet::new(), HashMap::new())?;
    let g2 = db.add_node(HashSet::new(), HashMap::new())?;
    let gg1 = db.add_node(HashSet::new(), HashMap::new())?;
    let far = db.add_node(HashSet::new(), HashMap::new())?;

    db.add_edge(hub, l1, "LINK", HashMap::new(), 1.0)?;
    db.add_edge(hub, l2, "LINK", HashMap::new(), 1.0)?;
    db.add_edge(l1, g1, "LINK", HashMap::new(), 1.0)?;
    db.add_edge(l2, g2, "LINK", HashMap::new(), 1.0)?;
    db.add_edge(g1, gg1, "LINK", HashMap::new(), 1.0)?;
    db.add_edge(far, hub, "OTHER", HashMap::new(), 1.0)?;

    // 1 跳（出边）：只应包含 hub 与两个直接后继
    let k1 = db.k_hop_subgraph_with(hub, 1, Direction::Outgoing, Some("LINK"))?;
    assert_eq!(k1.nodes, vec![hub, l1, l2]);
    assert_eq!(k1.edges.len(), 2);
    assert!(k1.contains(hub));
    assert!(!k1.contains(g1));

    // 2 跳（出边）：加入 g1 / g2
    let k2 = db.k_hop_subgraph_with(hub, 2, Direction::Outgoing, Some("LINK"))?;
    assert_eq!(k2.nodes, vec![hub, l1, l2, g1, g2]);
    assert_eq!(k2.edges.len(), 4);

    // 3 跳（出边）：加入 gg1，仍不含 OTHER 类型的 far
    let k3 = db.k_hop_subgraph_with(hub, 3, Direction::Outgoing, Some("LINK"))?;
    assert_eq!(k3.nodes, vec![hub, l1, l2, g1, g2, gg1]);
    assert!(!k3.contains(far));

    // 无过滤的无向 1 跳：应额外纳入 far
    let undirected = db.k_hop_subgraph_with(hub, 1, Direction::Both, None)?;
    assert!(undirected.contains(far));

    // 起点不存在必须报错
    assert!(db
        .k_hop_subgraph_with(999_999, 1, Direction::Outgoing, None)
        .is_err());

    Ok(())
}

// =========================================================================
// 算法在受限缓冲池下仍正确（纯磁盘外存寻路）
// =========================================================================
#[test]
fn test_algorithms_under_constrained_buffer_pool() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let db_path = dir.path().join("algo_constrained.db");

    // 256 帧 = 1MB 内存硬约束
    let db = GraphLite::open_with_pool_size(&db_path, 256)?;

    let total: u64 = 600;
    let mut tx = db.begin_transaction()?;
    let mut ids = Vec::new();
    for i in 0..total {
        let mut props = HashMap::new();
        props.insert("idx".to_string(), Value::from(i as i64));
        let mut labels = HashSet::new();
        labels.insert("N".to_string());
        ids.push(tx.add_node(labels, props)?);
        let _ = i;
    }
    tx.commit()?;

    // 环形链：0 -> 1 -> ... -> 599 -> 0，外加若干跨边
    let mut tx = db.begin_transaction()?;
    for i in 0..total as usize {
        let src = ids[i];
        let dst = ids[(i + 1) % total as usize];
        tx.add_edge(src, dst, "RING", HashMap::new(), 1.0)?;
    }
    for i in 0..total as usize {
        let src = ids[i];
        let dst = ids[(i + 7) % total as usize];
        if src != dst {
            let _ = tx.add_edge(src, dst, "CHORD", HashMap::new(), 2.0);
        }
    }
    tx.commit()?;

    // 1. PageRank 在 1MB 内存下完成 20 轮阻尼迭代
    let scores = db.pagerank_with(0.85, 20, 1e-6);
    assert_eq!(scores.len(), total as usize);
    let total_score: f64 = scores.iter().map(|s| s.score).sum();
    assert!((total_score - 1.0).abs() < 1e-4);

    // 2. WCC：整个环应为一个分量
    let components = db.weakly_connected_components();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].len(), total as usize);

    // 3. K-Hop 子图：从 0 出发 2 跳
    let sub = db.k_hop_subgraph_with(ids[0], 2, Direction::Outgoing, None)?;
    assert!(sub.contains(ids[0]));
    assert!(sub.nodes.len() >= 3);
    assert!(db.buffer_stats().used_frames <= 256);

    Ok(())
}

// =========================================================================
// 环检测不得依赖递归深度
// =========================================================================
/// **深链上的环检测不得触发栈溢出。**
///
/// 这是为一个真实的进程级崩溃写的：`has_cycle` 与 `find_cycles` 用的是递归 DFS，
/// 递归深度等于**路径长度**，而路径长度由用户数据决定。一条 6 万节点的链——完全
/// 合法，且是「引用链」「章节顺序」这类数据的自然形态——会耗尽线程栈：
///
/// ```text
/// thread 'main' has overflowed its stack
/// fatal runtime error: stack overflow, aborting
/// ```
///
/// 这是**不可捕获**的 abort：调用方无法用 `catch_unwind` 挽救，宿主进程直接退出。
/// 对嵌入式库来说，让合法数据触发进程崩溃是不可接受的。
///
/// 现在两条路径都改为显式栈（堆上），内存随数据规模增长而与栈上限无关。本测试的
/// 规模足以让旧实现必然崩溃；同时验证结果正确（无环报 false、加回边后报 true），
/// 确保修复不是靠「不检测」换来的。
#[test]
fn test_cycle_detection_handles_deep_chains() -> Result<(), GraphError> {
    let (dir, db) = open_temp("deep_chain.db")?;

    // 6 万节点长链：旧实现在此规模即崩溃
    const N: u64 = 60_000;
    db.with_transaction(|tx| {
        let mut prev = tx.add_node(HashSet::new(), HashMap::new())?;
        for _ in 1..N {
            let cur = tx.add_node(HashSet::new(), HashMap::new())?;
            tx.add_edge(prev, cur, "NEXT", HashMap::new(), 1.0)?;
            prev = cur;
        }
        Ok(())
    })?;
    assert_eq!(db.node_count(), N as usize);

    // 无环：必须返回 false，而不是崩溃
    assert!(!db.has_cycle(), "a long chain has no cycle");
    assert!(
        db.find_cycles().is_empty(),
        "a long chain has no cycles to report"
    );

    // 加一条回边：环检测必须仍然正确
    db.add_edge(N, 1, "BACK", HashMap::new(), 1.0)?;
    assert!(db.has_cycle(), "the back edge creates a cycle");
    let cycles = db.find_cycles();
    assert!(
        !cycles.is_empty(),
        "the back edge must be reported as a cycle"
    );
    // 环应当回到起点，形成闭合路径
    assert!(
        cycles.iter().any(|c| c.first() == c.last()),
        "a reported cycle must be closed (start repeated at the end)"
    );

    drop(db);
    drop(dir);
    Ok(())
}
