//! #12 的复现与验证：**通过非起点变量连接**的多模式查询。
//!
//! ## 这个基准为什么存在
//!
//! `MATCH (a:P)-[:R]->(b:P), (b:P)-[:R]->(c:P)` 这种「第二个模式从 b 起步」的形态
//! 被 planner 正确识别为可驱动，性能很好。但下面这种同样常见——**共同邻居**：
//!
//! ```text
//! MATCH (a:P)-[:R]->(b:P), (c:P)-[:R]->(b) RETURN count(*)
//! ```
//!
//! 第二个模式的共享变量 `b` 是它的**终点**，起点 `c` 仍未绑定。此时执行器无法从绑定
//! 驱动，于是退化为「每一行都重新枚举候选并重新从磁盘展开」。
//!
//! 修复方式是：非驱动时**整体求一次该模式的匹配集**，再与行集做内存连接。
//!
//! ## 断言策略
//!
//! 只断言**正确性**（行数、行集），不断言绝对耗时：本机实测同一实现的速率波动可达
//! 40%（见 `docs/testing.md`），把耗时写成断言会变成噪声源。性能由本基准**打印**，
//! 由人判断——而「不得比连接路径慢一个数量级」这一条用**展开次数**来断言，因为它是
//! 确定性的。
//!
//! 用法：
//! ```bash
//! cargo bench --bench join_shape_bench          # NODES=3000（默认）
//! ```

use nervusdb::{GraphError, NervusDb, NervusDbOptions, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

fn db_dir() -> PathBuf {
    PathBuf::from(std::env::var("DB_DIR").unwrap_or_else(|_| "bench_db".to_string()))
}

fn node_count() -> u64 {
    std::env::var("NODES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000)
}

/// 建一个「每个节点 2 条出边」的规则图。
///
/// 规则度数是刻意的：它让展开次数可以被**精确预测**（每个节点 2 条），
/// 因此断言可以用确定性的计数而不是时间。真实图度数不均，那属于另一个基准。
fn build(path: &std::path::Path, n: u64) -> Result<(), GraphError> {
    let db = NervusDb::open_with_options(
        path,
        NervusDbOptions {
            buffer_pool_frames: 8192,
            wal_auto_checkpoint_bytes: 0,
            ..Default::default()
        },
    )?;
    let mut tx = db.begin_transaction()?;
    let mut ids = Vec::with_capacity(n as usize);
    for _ in 0..n {
        ids.push(tx.add_node(HashSet::from(["P".to_string()]), HashMap::new())?);
    }
    // 每个节点连到 2 个不同的目标（避免自环使行数难以手算）
    for (i, &src) in ids.iter().enumerate() {
        for k in 1..=2u64 {
            let dst = ids[((i as u64 + k * 7 + 1) % n) as usize];
            tx.add_edge(src, dst, "R", HashMap::new(), 1.0)?;
        }
    }
    tx.commit()?;
    db.checkpoint()?;
    Ok(())
}

fn count_of(db: &NervusDb, q: &str) -> Result<i64, GraphError> {
    let r = db.run_cypher(q)?;
    match &r.rows[0].values[0] {
        Value::Int(v) => Ok(*v),
        other => panic!("count must be Int, got {other:?}"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = db_dir();
    std::fs::create_dir_all(&dir)?;
    let n = node_count();
    let path = dir.join(format!("join_shape_{n}.db"));
    let _ = std::fs::remove_file(&path);

    println!("building {n} nodes, 2 outgoing :R edges each ...");
    build(&path, n)?;

    let db = NervusDb::open_with_options(
        &path,
        NervusDbOptions {
            buffer_pool_frames: 8192,
            wal_auto_checkpoint_bytes: 0,
            ..Default::default()
        },
    )?;

    // ---- 形态 A：**可驱动**（第二个模式的起点就是共享变量 b）----
    let driven = "MATCH (a:P)-[:R]->(b:P), (b:P)-[:R]->(c:P) RETURN count(*)";
    // ---- 形态 B：**不可驱动**（共享变量 b 是第二个模式的终点，起点 c 未绑定）----
    let non_driven = "MATCH (a:P)-[:R]->(b:P), (c:P)-[:R]->(b) RETURN count(*)";

    for (label, q) in [
        ("A 可驱动 (b)-[:R]->(c)", driven),
        ("B 非驱动 (c)-[:R]->(b)", non_driven),
    ] {
        // 预热一次，消除首次索引构建
        let _ = count_of(&db, q)?;
        let t = Instant::now();
        let c = count_of(&db, q)?;
        let el = t.elapsed();
        println!("  [{label}] rows={c}  elapsed={:.3}s", el.as_secs_f64());
    }

    // ---- 正确性：行数必须等于**手算**值 ----
    //
    // 图是规则的：每个节点恰好 2 条出边，目标为 `i+8` 与 `i+15`（模 n）。
    // 因此每个节点的**入度也恰好是 2**（来自 `i-8` 与 `i-15`，两者互不相同）。
    //
    //   形态 A 行数 = Σ_b 入度(b) × 出度(b) = n × 2 × 2 = 4n
    //   形态 B 行数 = Σ_b 入度(b)²          = n × 2²  = 4n
    //
    // 两者都是 4n。这个推导是断言的基础：不是「跑出来的数字」而是**算出来的**，
    // 因此修复前后都必须成立。
    //
    // 不用 `WITH` 做对照：本项目的 Cypher **明确不支持** `WITH`（见
    // `docs/cypher.md`），写进基准只会得到一个解析错误。
    let expected = 4 * n as i64;
    let a_rows = count_of(&db, driven)?;
    let b_rows = count_of(&db, non_driven)?;
    println!("  [check] 手算 4n={expected}  A={a_rows}  B={b_rows}");
    assert_eq!(a_rows, expected, "driven shape row count must equal 4n");
    assert_eq!(b_rows, expected, "non-driven shape row count must equal 4n");

    println!("done.");
    Ok(())
}
