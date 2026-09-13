//! 代价模型与多模式连接定序。
//!
//! 这套测试针对 `src/cypher/planner.rs` 与 `find_matches` 的索引嵌套循环。
//!
//! ## 为什么需要它
//!
//! 「按估计基数排序模式」这件事有一个**静默的**失败模式：顺序选错不会报错，
//! 只会慢。慢多少取决于数据，而在小测试夹具上通常快到看不出差别——于是错误的
//! 定序会一直躺在那里，直到有人在真实规模的数据上量到它。因此这套测试不满足于
//! 「结果对了」：
//!
//! 1. **行集等价**：定序不得改变任何查询的**行集合**（顺序可以变，见下）。
//! 2. **计划可核对**：EXPLAIN 必须报出定序、每个模式的估计值，以及该估计是
//!    实测还是近似。
//! 3. **确实发生了变化**：用一个「共享变量驱动」的查询，对比定序前后读到的
//!    节点次数，证明索引嵌套循环真的少做了工作，而不是仅仅换了个写法。
//!
//! ## 行顺序：一个刻意的行为变化
//!
//! 定序会改变多模式查询的**行顺序**。Cypher 不保证无 `ORDER BY` 时的行顺序，
//! 因此这不是兼容性破坏；但它确实会改变调用方看到的东西，所以这里用测试把它
//! 固定下来，而不是留在「没人知道原来是什么顺序」的状态。

use nervusdb::{GraphError, NervusDb, Value};
use tempfile::tempdir;

fn open_temp(name: &str) -> Result<(tempfile::TempDir, NervusDb), GraphError> {
    let dir = tempdir()?;
    let db = NervusDb::open(dir.path().join(name))?;
    Ok((dir, db))
}

fn scalar_i64(db: &NervusDb, cypher: &str) -> Result<i64, GraphError> {
    let res = db.run_cypher(cypher)?;
    assert_eq!(res.row_count(), 1, "expected one row: {cypher}");
    match &res.rows[0].values[0] {
        Value::Int(v) => Ok(*v),
        other => panic!("expected Int, got {other:?}"),
    }
}

fn plan_text(res: &nervusdb::CypherResultSet) -> String {
    res.rows
        .iter()
        .map(|r| match &r.values[0] {
            Value::String(s) => s.clone(),
            other => format!("{other:?}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// =========================================================================
// 1. 行集等价：定序不得改变任何查询的结果
// =========================================================================

/// 一个「共享变量连接」的查询，其行集必须与手写展开一致。
///
/// 这是定序的安全网：无论模式怎么排序、走不走索引嵌套循环，答案必须一样。
#[test]
fn test_join_reorder_preserves_row_set() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("planner_equiv.db")?;

    // 链：A -> B -> C，另有孤立的 D
    db.execute("CREATE (a:P {name: 'A'})-[:R]->(b:P {name: 'B'})-[:R]->(c:P {name: 'C'})")?;
    db.execute("CREATE (d:P {name: 'D'})")?;

    // 两模式共享 b：只有 A->B->C 这一条路径满足
    let res =
        db.run_cypher("MATCH (a:P)-[:R]->(b:P), (b:P)-[:R]->(c:P) RETURN a.name, b.name, c.name")?;
    assert_eq!(
        res.row_count(),
        1,
        "shared-variable join must yield one row"
    );
    assert_eq!(res.rows[0].values[0], Value::from("A"));
    assert_eq!(res.rows[0].values[1], Value::from("B"));
    assert_eq!(res.rows[0].values[2], Value::from("C"));

    Ok(())
}

/// 无共享变量时必须仍是**笛卡尔积**，定序不得把它错误地当成连接。
///
/// 定序的目标是「优先连通」，但全体都不连通时只能退化。若规划器在这种情况下
/// 误判为「已全部满足」而提前停止，行数会偏少且不报错。
#[test]
fn test_cartesian_product_is_preserved() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("planner_cartesian.db")?;

    for name in ["A", "B", "C"] {
        db.execute(&format!("CREATE (:P {{name: '{name}'}}) "))?;
    }

    // 3 x 3 = 9
    assert_eq!(scalar_i64(&db, "MATCH (x:P), (y:P) RETURN count(*)")?, 9);

    // 三模式：3 x 3 x 3 = 27
    assert_eq!(
        scalar_i64(&db, "MATCH (x:P), (y:P), (z:P) RETURN count(*)")?,
        27
    );

    Ok(())
}

/// 共享变量被当作连接约束（不是覆盖）：`(a)-[:R]->(a)` 要求自环。
#[test]
fn test_shared_start_variable_is_a_join_constraint() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("planner_selfjoin.db")?;

    db.execute("CREATE (a:N {id: 1})-[:R]->(a)")?; // 自环
    db.execute("CREATE (b:N {id: 2})-[:R]->(c:N {id: 3})")?;

    // 模式 2 的起点 a 已被模式 1 绑定：只有自环节点满足
    let n = scalar_i64(&db, "MATCH (a:N)-[:R]->(c), (a)-[:S]->(d) RETURN count(*)")?;
    assert_eq!(n, 0, "no :S edges exist");

    let n = scalar_i64(&db, "MATCH (a:N)-[:R]->(b), (a:N) RETURN count(*)")?;
    // 每个 :N 节点作为起点被枚举一次；有出边的只有 1 和 2，各自 1 条出边 => 2
    assert_eq!(n, 2, "self-join must respect the shared variable binding");

    Ok(())
}

// =========================================================================
// 2. EXPLAIN：定序与估计值必须是可核对的
// =========================================================================

/// EXPLAIN 必须报出定序，并把估计值的**依据**一起报出来。
///
/// 「依据」比数字重要：图存储里没有按关系类型统计的边数，扇出是近似值。
/// 只打印 `Est. rows: 12` 会让人以为它是量出来的。
#[test]
fn test_explain_reports_join_order_and_estimate_basis() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("planner_explain.db")?;

    db.execute("CREATE (a:P {name: 'A'})-[:R]->(b:P {name: 'B'})")?;
    db.execute("CREATE (:P {name: 'C'})")?;

    let plan = plan_text(
        &db.run_cypher("EXPLAIN MATCH (a:P)-[:R]->(b:P), (b:P)-[:R]->(c:P) RETURN a.name")?,
    );

    assert!(
        plan.contains("JoinOrder"),
        "a multi-pattern plan must report the join order:\n{plan}"
    );
    assert!(
        plan.contains("Est. rows"),
        "the plan must report an estimated cardinality:\n{plan}"
    );
    // 依据必须是**规划器算出的那一份**，而不是从布尔量重推的粗略标签。
    //
    // 这条断言曾经只要求出现「起点实测」或「起点为估计上界」——那是从
    // `start_is_measured` 拼出来的两句话，信息量比 `estimate_start` 实际算出的
    // 少（它知道用的是哪个索引、键和值是什么）。现在直接断言那些**具体内容**，
    // 使「算了却不用」这种退化无法再通过测试。
    assert!(
        plan.contains("起点依据："),
        "the estimate must print the planner's own basis:\n{plan}"
    );
    assert!(
        plan.contains("label index (:P)"),
        "the basis must name the index actually used, not a coarse label:\n{plan}"
    );
    assert!(
        plan.contains("索引嵌套循环"),
        "a driven pattern must be reported as index-nested-loop:\n{plan}"
    );

    Ok(())
}

/// 单模式计划不得出现 `JoinOrder`——没有连接可定序。
///
/// 这条防止「多模式的行被无条件插入」这类回归：它会让单模式计划出现无意义的行，
/// 而那种噪声最终会让人不再读计划。
#[test]
fn test_single_pattern_plan_has_no_join_order() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("planner_single.db")?;
    db.execute("CREATE (a:P {name: 'A'})-[:R]->(b:P {name: 'B'})")?;

    let plan = plan_text(&db.run_cypher("EXPLAIN MATCH (a:P)-[:R]->(b:P) RETURN a.name")?);
    assert!(
        !plan.contains("JoinOrder"),
        "single-pattern plans must not claim a join order:\n{plan}"
    );
    assert!(
        !plan.contains("Est. rows"),
        "single-pattern plans must stay free of estimate noise:\n{plan}"
    );

    Ok(())
}

// =========================================================================
// 3. 定序确实生效：实测读放大
// =========================================================================

/// 定序必须真的改变执行路径，而不是只改了 EXPLAIN 的显示。
///
/// ## 这个测试的形状是被一次失败的负向验证逼出来的
///
/// 初版用 400 个节点、默认 4MB 池（1024 帧）。它在**关掉绑定驱动之后依然通过**
/// ——因为整张图都装进了缓冲池，两种执行路径的页命中率没有区别。那种测试是
/// 装饰品：它只证明了「代码能跑」，没有证明「优化存在」。
///
/// 现在的形状让差异无法隐藏：
///
/// - **极小的池**（16 帧 = 64KB），使图**装不下**，页访问次数才成为可观测信号。
/// - **大量诱饵节点**（2 万个无边节点），它们是「未被绑定驱动」时才会被枚举的
///   起点候选。它们没有任何边，所以走对路径时**一次都不该被读到**。
///
/// 断言因此不是「少一点」，而是量级差：`(h:Hub)` 的等值索引把起点定到 1 个节点，
/// 那个节点只有 1 条出边。正确路径只需读常数个页；错误路径要读 2 万个节点记录。
#[test]
fn test_planner_avoids_expanding_the_expensive_pattern_first() -> Result<(), GraphError> {
    let dir = tempdir()?;
    // 64 帧 = 256KB。必须在 `MIN_SPILL_FRAMES`（16）之上，否则事务在容量耗尽时
    // 只能按 NO-STEAL 报错（那是**写入**路径的约束，见 AGENTS §1）；
    // 同时远小于数据量（2 万个节点记录 = 640KB），使图装不下。
    let db = NervusDb::open_with_options(
        dir.path().join("planner_work.db"),
        nervusdb::NervusDbOptions {
            buffer_pool_frames: 64,
            ..Default::default()
        },
    )?;

    // hub -> leaf 一条边。方向必须是 hub -> leaf：`(h)-[:R]->(c)` 沿出边遍历。
    {
        let mut tx = db.begin_transaction()?;
        let hub = tx.add_node(
            std::collections::HashSet::from(["Hub".to_string()]),
            std::collections::HashMap::from([("key".to_string(), Value::from("target"))]),
        )?;
        assert_eq!(hub, 1, "hub must be node 1 for the fixture to be readable");

        let leaf = tx.add_node(
            std::collections::HashSet::from(["Leaf".to_string()]),
            std::collections::HashMap::new(),
        )?;
        tx.add_edge(hub, leaf, "R", std::collections::HashMap::new(), 1.0)?;

        // 2 万个无边诱饵节点：只有「未绑定驱动」的错误路径会枚举它们
        for i in 0..20_000u64 {
            tx.add_node(
                std::collections::HashSet::from(["Decoy".to_string()]),
                std::collections::HashMap::from([("i".to_string(), Value::from(i as i64))]),
            )?;
        }
        tx.commit()?;
    }

    let query = "MATCH (h:Hub {key: 'target'}), (h)-[:R]->(c) RETURN count(*)";

    // 预热一次把标签索引建起来（首次查询会构建索引，那是另一笔开销）
    let warm = db.run_cypher(query)?;
    assert_eq!(warm.rows[0].values[0], Value::from(1), "one :R edge exists");

    // 再跑一次，量这一次的页访问
    let before = db.buffer_stats().cache_misses;
    let res = db.run_cypher(query)?;
    let misses = db.buffer_stats().cache_misses.saturating_sub(before);

    assert_eq!(res.rows[0].values[0], Value::from(1));

    // 阈值来自**实测**的两条路径对比（就这一份夹具、64 帧池）：
    //   绑定驱动启用：  0–5 次未命中
    //   绑定驱动禁用：  316 次未命中（枚举了 2 万个 :Decoy 起点）
    // 取 50 作为分界：距正确路径有 10 倍余量，距错误路径有 6 倍余量，
    // 因此不会因为页目录/元数据的小幅波动而漂边界。
    assert!(
        misses < 50,
        "a bound-driven expand must expand from the one bound node, not enumerate \
         every :Decoy as a start candidate; misses={misses} (20000 decoys were not \
         expected to be read)"
    );

    Ok(())
}

/// 被绑定的起点仍须满足**本模式**的标签/属性约束。
///
/// 这是一个容易写错的分支：起点变量已绑定时直接展开会跳过标签检查，于是
/// `MATCH (a:P), (a:Q) RETURN count(*)` 会错把 `:P` 节点当成 `:Q` 返回。
#[test]
fn test_bound_start_still_must_satisfy_its_own_pattern() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("planner_bound_label.db")?;

    db.execute("CREATE (:P {n: 1})")?;
    db.execute("CREATE (:Q {n: 2})")?;
    db.execute("CREATE (:P:Q {n: 3})")?;

    // 只有同时带 :P 与 :Q 的节点满足两个模式
    let n = scalar_i64(&db, "MATCH (a:P), (a:Q) RETURN count(*)")?;
    assert_eq!(n, 1, "only the :P:Q node satisfies both patterns");

    // 属性约束同样要在绑定路径上生效
    let n = scalar_i64(&db, "MATCH (a:P), (a:P {n: 1}) RETURN count(*)")?;
    assert_eq!(
        n, 1,
        "the bound start is still filtered by the pattern property"
    );

    let n = scalar_i64(&db, "MATCH (a:P), (a:P {n: 999}) RETURN count(*)")?;
    assert_eq!(n, 0, "a non-matching bound start must produce no rows");

    Ok(())
}

/// 等价性的大样本检查：定序前后行数必须一致。
///
/// 用手写查询算一个「已知答案」，再让规划器去算同一个问题。
#[test]
fn test_reordered_join_matches_hand_computed_answer() -> Result<(), GraphError> {
    let (_dir, db) = open_temp("planner_hand.db")?;

    // 3 条长度为 4 的链 `a->b->c->d`。每条链有 2 个两跳组合（a-b-c、b-c-d）
    // 与 1 个三跳组合（a-b-c-d）。
    for i in 0..3 {
        db.execute(&format!(
            "CREATE (a:C {{g: {i}}})-[:R]->(b:C {{g: {i}}})-[:R]->(c:C {{g: {i}}})-[:R]->(d:C {{g: {i}}})"
        ))?;
    }

    let two_hop = scalar_i64(
        &db,
        "MATCH (a:C)-[:R]->(b:C), (b:C)-[:R]->(c:C) RETURN count(*)",
    )?;
    assert_eq!(two_hop, 6, "3 chains x 2 two-hop paths each");

    // 三跳：3 x 1 = 3
    let three_hop = scalar_i64(
        &db,
        "MATCH (a:C)-[:R]->(b:C), (b:C)-[:R]->(c:C), (c:C)-[:R]->(d:C) RETURN count(*)",
    )?;
    assert_eq!(three_hop, 3, "3 chains x 1 three-hop path each");

    Ok(())
}

// =========================================================================
// #12：通过**非起点变量**连接时，不得逐行重新枚举候选
// =========================================================================

/// 非驱动连接必须只求一次候选，而不是每一行重求一次。
///
/// ## 信号为什么是页读取次数，不是耗时
///
/// 本机实测同一实现的速率波动可达 40%（见 `docs/testing.md`），把耗时写成断言
/// 会变成噪声源。而**缓冲池未命中次数**是确定性的：两种实现的差异不是「快一点」，
/// 而是「每行都重新从磁盘展开一遍候选」，量级差几百倍。
///
/// 实测（16 帧池，规则图每个节点 2 条出边）：
///
/// | N | 只求一次 | 逐行重求 |
/// |---|---|---|
/// | 400 | **30** | 12,015 |
/// | 800 | **77** | 60,839 |
///
/// 修复后接近线性增长，修复前是平方级。断言用「不超过 500」这一个保守上界：
/// 距修复后的 77 有 6 倍余量，距修复前的 60,839 有 120 倍余量，因此不会因为
/// 页目录或元数据的少量波动而漂边界。
///
/// ## 为什么必须用「放不进池子」的规模
///
/// 初版用 300 个节点配 64 帧池，两种实现的 misses **都是 0**——整张图都装进了池里，
/// 根本没有磁盘访问，测不出任何差异。夹具的形状是被这个观察逼出来的。
#[test]
fn test_non_start_variable_join_does_not_rescan_per_row() -> Result<(), GraphError> {
    let dir = tempdir()?;
    let n: u64 = 800;
    // 16 帧 = 64KB，远小于 N=800 时的节点记录与属性页
    let db = NervusDb::open_with_options(
        dir.path().join("nondriven.db"),
        nervusdb::NervusDbOptions {
            buffer_pool_frames: 16,
            wal_auto_checkpoint_bytes: 0,
            ..Default::default()
        },
    )?;

    {
        let mut tx = db.begin_transaction()?;
        let mut ids = Vec::new();
        for _ in 0..n {
            ids.push(tx.add_node(
                std::collections::HashSet::from(["P".to_string()]),
                std::collections::HashMap::new(),
            )?);
        }
        // 每个节点 2 条出边，目标是 i+8 与 i+15（模 n），互不相同 → 入度也是 2
        for (i, &src) in ids.iter().enumerate() {
            for k in 1..=2u64 {
                let dst = ids[((i as u64 + k * 7 + 1) % n) as usize];
                tx.add_edge(src, dst, "R", std::collections::HashMap::new(), 1.0)?;
            }
        }
        tx.commit()?;
    }
    db.checkpoint()?;

    // 共享变量 b 是第二个模式的**终点**，起点 c 未绑定 → 非驱动路径
    let query = "MATCH (a:P)-[:R]->(b:P), (c:P)-[:R]->(b) RETURN count(*)";

    // 手算：A 形态 Σ 入度×出度 = n×2×2 = 4n；B 形态 Σ 入度² = n×2² = 4n
    let expected = 4 * n as i64;
    let warm = db.run_cypher(query)?;
    assert_eq!(
        warm.rows[0].values[0],
        Value::from(expected),
        "row count must equal the hand-computed 4n"
    );

    let before = db.buffer_stats().cache_misses;
    let res = db.run_cypher(query)?;
    let misses = db.buffer_stats().cache_misses.saturating_sub(before);

    assert_eq!(res.rows[0].values[0], Value::from(expected));
    assert!(
        misses < 500,
        "a non-driven join must not re-enumerate candidates per row: \
         {misses} page misses for {n} nodes (the per-row version measured 60,839; \
         the once-only version measured 77)"
    );

    Ok(())
}
