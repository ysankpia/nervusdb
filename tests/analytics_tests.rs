//! 战役三验证套件：企业级图算法引擎（PageRank / 弱连通分量 / K-Hop 子图）。

use nervusdb::{Direction, GraphError, NervusDb, Value};
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

fn open_temp(name: &str) -> Result<(tempfile::TempDir, NervusDb), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join(name))?;
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
    let db = NervusDb::open_with_pool_size(&db_path, 256)?;

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

// =========================================================================
// 环检测：共享父节点的兄弟**不得**被判成环
// =========================================================================

/// **`has_cycle` 曾把无环的 DAG 报成有环，只因边里出现了「兄弟」。**
///
/// ## 缺陷
///
/// 显式栈的 DFS 一次性把当前节点的**所有**邻居标灰并压栈，而不是逐个深入。
/// 于是互为兄弟的节点同时处于 Gray：处理兄弟 A 时看到兄弟 B 仍是 Gray，就被判成
/// A→B 的环——而 A 与 B 之间根本没有边。
///
/// 最小复现是菱形 `P→X, P→Y, X→Y`。同一个图，**只因边的插入顺序不同**：
///
/// | 插入顺序 | 边 | 旧结果 | 正确结果 |
/// | --- | --- | --- | --- |
/// | `P→X, P→Y, X→Y` | `[(1,2),(1,3),(2,3)]` | `true` | `false` |
/// | `P→Y, P→X, X→Y` | `[(1,3),(1,2),(2,3)]` | `false` | `false` |
///
/// 随机 DAG（`i<j` 自然序，必然无环）40 次试验中 **3 次**误报。
///
/// ## 为什么原来的测试没抓到
///
/// `test_cycle_detection_handles_deep_chains` 用一条 **60,000 节点的链**——链上每个
/// 节点只有一个出边，因此**没有兄弟**，恰好绕开了这个形状。而「共享父节点的兄弟」
/// 在真实数据里遍地都是（一篇论文的多位作者、一个目录下的多个文件），所以这个
/// 缺陷在单链夹具上是不可见的。
///
/// 这两条断言（无假阳性 + 真环不漏报）缺一不可：只测前者会让「永远返回 false」通过。
#[test]
fn shaft_of_diamonds_has_no_cycle() -> Result<(), GraphError> {
    // 第 1 部分：菱形，两种插入顺序都必须报无环。
    for (label, edges) in [
        ("parent-first", vec![(1u64, 2u64), (1, 3), (2, 3)]),
        ("sibling-first", vec![(1u64, 3u64), (1, 2), (2, 3)]),
    ] {
        let (dir, db) = open_temp(&format!("diamond_{}.db", label.replace('-', "_")))?;
        db.with_transaction(|tx| {
            for _ in 1..=3u64 {
                tx.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
            }
            for (a, b) in &edges {
                tx.add_edge(*a, *b, "E", HashMap::new(), 1.0)?;
            }
            Ok(())
        })?;

        assert!(
            !db.has_cycle(),
            "[{label}] this DAG has no cycle, but has_cycle reported one (edges {edges:?})"
        );
        assert!(
            db.find_cycles().is_empty(),
            "[{label}] find_cycles must agree, got {:?}",
            db.find_cycles()
        );
        drop(db);
        drop(dir);
    }

    Ok(())
}

/// 随机无环图上的**批量**假阳性检查，外加真环不漏报。
///
/// 单独用菱形只覆盖了「两个兄弟」这一种形状；随机化才能覆盖多兄弟、多层兄弟、
/// 以及兄弟自身带子树的组合。用固定种子，失败可精确复现——本测试第一次运行就
/// 在固定的第 1/2/3 次试验报错，正是靠这一点定位的。
///
/// 与上一条测试的关系：上一条是**最小复现**（读起来能直接看懂哪里错了），
/// 这一条是**覆盖广度**。两者都要，因为最小复现不足以证明修法对一般形状成立。
#[test]
fn random_dags_report_no_cycle_but_real_cycles_are_found() -> Result<(), GraphError> {
    /// 确定性 PRNG：同一个种子必须给出同一张图，否则失败无法复现。
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    let (dir, _keep) = open_temp("random_dag_marker.db")?;

    // 40 张随机 DAG，边只从小的 id 指向大的 id，因此**必然无环**。
    for trial in 0..40u64 {
        let db_path = dir.path().join(format!("dag{trial}.db"));
        let db = NervusDb::open(&db_path)?;
        let n = 8u64;
        let mut rng = Rng(1234 + trial);

        db.with_transaction(|tx| {
            for i in 1..=n {
                tx.add_node(
                    HashSet::from(["N".to_string()]),
                    HashMap::from([("i".to_string(), Value::from(i as i64))]),
                )?;
            }
            Ok(())
        })?;

        let mut edges = Vec::new();
        for i in 1..=n {
            for j in (i + 1)..=n {
                if rng.below(100) < 45 {
                    edges.push((i, j));
                }
            }
        }
        db.with_transaction(|tx| {
            for (a, b) in &edges {
                tx.add_edge(*a, *b, "E", HashMap::new(), 1.0)?;
            }
            Ok(())
        })?;

        assert!(
            !db.has_cycle(),
            "trial {trial} is a DAG (edges only run low id to high id) but has_cycle \
             reported a cycle. Edges: {edges:?}"
        );
        assert!(
            db.find_cycles().is_empty(),
            "trial {trial}: find_cycles disagreed with has_cycle. Edges: {edges:?}"
        );
    }

    // 真环仍然必须被检出——否则「永远返回 false」也能让上面全过。
    for (label, edges) in [
        ("self_loop", vec![(1u64, 1u64)]),
        ("two_cycle", vec![(1, 2), (2, 1)]),
        ("three_cycle", vec![(1, 2), (2, 3), (3, 1)]),
        (
            "diamond_plus_back_edge",
            vec![(1, 2), (1, 3), (2, 3), (3, 1)],
        ),
    ] {
        let db_path = dir.path().join(format!("cycle_{label}.db"));
        let db = NervusDb::open(&db_path)?;
        db.with_transaction(|tx| {
            for _ in 1..=3u64 {
                tx.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
            }
            for (a, b) in &edges {
                tx.add_edge(*a, *b, "E", HashMap::new(), 1.0)?;
            }
            Ok(())
        })?;

        assert!(
            db.has_cycle(),
            "[{label}] must be reported as cyclic (edges {edges:?})"
        );
        assert!(
            !db.find_cycles().is_empty(),
            "[{label}] find_cycles must report at least one cycle"
        );
        for c in db.find_cycles() {
            assert!(
                c.len() >= 2 && c.first() == c.last(),
                "[{label}] a reported cycle must be closed, got {c:?}"
            );
        }
    }

    Ok(())
}

// =========================================================================
// K-Hop：边集合必须**完整**，不只是数量对
// =========================================================================

/// 用一条**独立路径**复算「子图内部的边」：Cypher 列出全图边，再按节点集筛选。
///
/// 用 Cypher 而不是复用 `k_hop_subgraph` 自己的邻接遍历，是因为同一个遍历实现
/// 无法验证自己——那只是自证。这正是本缺陷逃过原测试的原因之一：原测试只比较
/// `edges.len()`，没有第二个来源可比。
fn internal_edges_by_cypher(db: &NervusDb, nodes: &[u64]) -> Vec<(u64, u64, u64)> {
    let inside: HashSet<u64> = nodes.iter().copied().collect();
    let mut v: Vec<(u64, u64, u64)> = db
        .run_cypher("MATCH (a)-[r]->(b) RETURN id(r), id(a), id(b)")
        .expect("Cypher must list every edge")
        .rows
        .iter()
        .map(|r| {
            (
                r.values[0].as_i64().unwrap() as u64,
                r.values[1].as_i64().unwrap() as u64,
                r.values[2].as_i64().unwrap() as u64,
            )
        })
        .filter(|(_, a, b)| inside.contains(a) && inside.contains(b))
        .collect();
    v.sort_unstable();
    v
}

/// **K-Hop 曾漏收第 k 层节点之间的边。**
///
/// ## 缺陷
///
/// BFS 在 `depth >= k` 时停止展开，于是**恰好落在第 k 层**的节点从不被展开——
/// 它们之间的边永远不会被看到。而约定是「保留两端都在子图内的边」
/// （`AGENTS.md` §4.5），这些边符合条件却被漏掉。
///
/// 实测（k=1，方向 Both）：
///
/// | 形状 | 节点集 | 旧边数 | 应有 |
/// | --- | --- | --- | --- |
/// | `1↔2` | 2（对） | 1 | **2**（缺 `2→1`） |
/// | 三角形 `1→2→3→1` | 3（对） | 2 | **3**（缺 `2→3`） |
/// | 星 `1→2,1→3,1→4` 加 `2→3` | 4（对） | 3 | **4**（缺 `2→3`） |
///
/// ## 为什么原来的测试没抓到
///
/// `test_k_hop_subgraph` 只断言 `k1.edges.len() == 2` 这样的**数量**。它的夹具把
/// 节点放在**不同层**（hub → l1/l2 → g1/g2），恰好没有「同层相邻」的形状，因此
/// 数量一直是对的。**数量对而内容错，是断言粒度问题，不是夹具问题。**
///
/// 本测试因此断言**具体的边集合**，并与 Cypher 的独立复算逐条比对——只比数量
/// 不足以防住这一类，这正是它存在的理由。
#[test]
fn k_hop_keeps_every_edge_whose_endpoints_are_inside() -> Result<(), GraphError> {
    // 每个形状都选在「同层存在相邻节点」上，即缺陷的触发条件。
    /// 一个形状：(名字, 节点数, 边表)。别名是为了让下面的向量字面量可读——
    /// 内联写出这个元组类型会被 clippy 的 type_complexity 拒掉。
    type Shape<'a> = (&'a str, u64, Vec<(u64, u64)>);
    let shapes: Vec<Shape> = vec![
        ("two_cycle_1_2", 2, vec![(1, 2), (2, 1)]),
        ("triangle", 3, vec![(1, 2), (2, 3), (3, 1)]),
        (
            "star_plus_sibling_edge",
            4,
            vec![(1, 2), (1, 3), (1, 4), (2, 3)],
        ),
        ("chain", 5, vec![(1, 2), (2, 3), (3, 4), (4, 5)]),
        ("diamond", 4, vec![(1, 2), (1, 3), (2, 4), (3, 4)]),
    ];

    for (label, node_count, edges) in shapes {
        for k in 0..=node_count as usize {
            for direction in [Direction::Outgoing, Direction::Incoming, Direction::Both] {
                let (dir_keep, db) = open_temp(&format!("khop_{label}_{k}.db"))?;
                db.with_transaction(|tx| {
                    for _ in 1..=node_count {
                        tx.add_node(HashSet::from(["N".to_string()]), HashMap::new())?;
                    }
                    for (a, b) in &edges {
                        tx.add_edge(*a, *b, "E", HashMap::new(), 1.0)?;
                    }
                    Ok(())
                })?;

                let sub = db.k_hop_subgraph_with(1, k, direction, None)?;

                // 节点集必须含起点、去重、升序（其余承诺）
                assert_eq!(
                    sub.nodes.first().copied(),
                    Some(1),
                    "[{label}] k={k} {direction:?}: start node must be present"
                );
                let unique: HashSet<u64> = sub.nodes.iter().copied().collect();
                assert_eq!(
                    unique.len(),
                    sub.nodes.len(),
                    "[{label}] k={k} {direction:?}: nodes must be deduplicated"
                );
                let mut sorted = sub.nodes.clone();
                sorted.sort_unstable();
                assert_eq!(
                    sub.nodes, sorted,
                    "[{label}] k={k} {direction:?}: nodes must be ascending"
                );

                // 核心断言：边集合必须与独立复算**逐条相等**。
                let mut got: Vec<(u64, u64, u64)> = sub
                    .edges
                    .iter()
                    .map(|e| (e.id, e.src_id, e.dst_id))
                    .collect();
                got.sort_unstable();
                let expected = internal_edges_by_cypher(&db, &sub.nodes);

                assert_eq!(
                    got, expected,
                    "[{label}] k={k} {direction:?}: the edge set must contain exactly \
                     the edges whose both endpoints are inside the subgraph \
                     (nodes {:?})",
                    sub.nodes
                );

                drop(db);
                drop(dir_keep);
            }
        }
    }

    Ok(())
}
