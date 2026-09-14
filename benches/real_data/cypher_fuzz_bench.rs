//! Cypher 模糊测试：随机生成查询，验证**结果与内存模型一致**。
//!
//! ## 为什么需要它
//!
//! 已有的 `differential_test` 用随机**写**操作对拍内存模型，但它只覆盖 CRUD：
//! `add_node` / `add_edge` / `update` / `remove`。**Cypher 查询本身从未被对拍过。**
//!
//! 而 `has_cycle` 那个假阳性缺陷说明了一件事：缺陷藏在「没人构造过的形状」里。
//! 查询的形状空间比读写操作大得多——多模式 `MATCH`、变长路径、`WHERE` 组合、
//! 聚合、`ORDER BY`……手写测试只能覆盖想到的那些。
//!
//! ## 方法（照搬 SQLite 的 differential/oracle 思路）
//!
//! 1. 用确定性种子建一张**随机图**，同时建一份**内存模型**；
//! 2. 随机生成一批查询，每个查询都有一条**独立的内存实现**给出期望结果；
//! 3. 逐条比对。任何不一致都是缺陷，**并带上可复现的种子与查询**。
//!
//! 关键约束：**内存实现必须独立于引擎实现**。用引擎的遍历去算期望值等于自证。
//! 因此这里对图的所有期望值都从 `HashMap` 模型直接算。
//!
//! 运行：
//! ```bash
//! cargo bench --bench cypher_fuzz_bench                    # 默认 50 轮
//! SEEDS=5000 cargo bench --bench cypher_fuzz_bench         # 更多轮次
//! SEED0=12345 cargo bench --bench cypher_fuzz_bench        # 从指定种子开始（复现）
//! ```

use nervusdb::{NervusDb, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

// =========================================================================
// 确定性 PRNG
// =========================================================================

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // 种子 0 会让 LCG 退化，加一个非零偏差。
        Self(
            seed.wrapping_mul(2862933555777941757)
                .wrapping_add(3037000493),
        )
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

// =========================================================================
// 内存模型：期望值的唯一来源，不依赖引擎
// =========================================================================

#[derive(Clone, Default)]
struct ModelNode {
    labels: BTreeSet<String>,
    props: HashMap<String, Value>,
}

#[derive(Clone)]
struct ModelEdge {
    src: u64,
    dst: u64,
    etype: String,
}

#[derive(Default)]
struct Model {
    nodes: HashMap<u64, ModelNode>,
    edges: HashMap<u64, ModelEdge>,
}

impl Model {
    /// 独立计算的标签基数（引擎必须给出同一个数）。
    fn count_label(&self, label: &str) -> usize {
        self.nodes
            .values()
            .filter(|n| n.labels.contains(label))
            .count()
    }

    /// 独立计算的「某标签节点的某整数属性之和」。
    fn sum_prop(&self, label: &str, key: &str) -> i64 {
        self.nodes
            .values()
            .filter(|n| n.labels.contains(label))
            .filter_map(|n| n.props.get(key).and_then(|v| v.as_i64()))
            .sum()
    }

    /// 独立计算的「按标签与属性值筛选后的数量」。
    fn count_label_prop_eq(&self, label: &str, key: &str, val: i64) -> usize {
        self.nodes
            .values()
            .filter(|n| n.labels.contains(label))
            .filter(|n| n.props.get(key).and_then(|v| v.as_i64()) == Some(val))
            .count()
    }
}

// =========================================================================
// 夹具
// =========================================================================

const LABELS: [&str; 3] = ["A", "B", "C"];
const ETYPES: [&str; 2] = ["R", "S"];

/// 建图：返回 (引擎, 模型)。两者由**同一串随机决策**驱动，因此必须一致。
fn build(seed: u64, node_count: u64, edge_count: u64) -> (NervusDb, Model, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = NervusDb::open(dir.path().join("fuzz.db")).expect("open");
    let mut model = Model::default();
    let mut rng = Rng::new(seed);

    // 节点：随机标签集合 + 随机整数属性。
    db.with_transaction(|tx| {
        for id in 1..=node_count {
            let mut labels = HashSet::new();
            let count = 1 + rng.below(2); // 1..=2 个标签
            for _ in 0..count {
                labels.insert(rng.pick(&LABELS).to_string());
            }
            let val = rng.below(20) as i64;
            let props: HashMap<String, Value> =
                HashMap::from([("v".to_string(), Value::from(val))]);

            tx.add_node(labels.clone(), props.clone())?;

            model.nodes.insert(
                id,
                ModelNode {
                    labels: labels.into_iter().collect(),
                    props,
                },
            );
        }
        Ok(())
    })
    .expect("seed nodes");

    // 边：随机两端与类型。允许自环与重复边（两者都是必须支持的形状）。
    let ids: Vec<u64> = (1..=node_count).collect();
    db.with_transaction(|tx| {
        for i in 0..edge_count {
            let src = *rng.pick(&ids);
            let dst = *rng.pick(&ids);
            let etype = rng.pick(&ETYPES).to_string();
            let id = tx.add_edge(src, dst, &etype, HashMap::new(), 1.0)?;
            model.edges.insert(id, ModelEdge { src, dst, etype });
            let _ = i;
        }
        Ok(())
    })
    .expect("seed edges");

    (db, model, dir)
}

// =========================================================================
// 单条查询的比对
// =========================================================================

/// 从结果集取出第一列第一个值（单值查询用）。
fn scalar(db: &NervusDb, query: &str) -> Result<Value, String> {
    let res = db.run_cypher(query).map_err(|e| e.to_string())?;
    res.rows
        .first()
        .and_then(|r| r.values.first().cloned())
        .ok_or_else(|| format!("query returned no rows: {query}"))
}

/// 比对一条查询，返回 `Err(说明)` 若与模型不符。
fn compare(db: &NervusDb, query: &str, expected: Value) -> Result<(), String> {
    match scalar(db, query) {
        Ok(got) => {
            if got == expected {
                Ok(())
            } else {
                Err(format!(
                    "query: {query}\n  expected: {expected:?}\n  got:      {got:?}"
                ))
            }
        }
        Err(e) => Err(format!("query: {query}\n  engine error: {e}")),
    }
}

// =========================================================================
// 主循环
// =========================================================================

fn main() {
    let seeds: u64 = std::env::var("SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let seed0: u64 = std::env::var("SEED0")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    println!("============================================================");
    println!(" CYPHER FUZZ: random queries vs an independent in-memory model");
    println!(
        " Seeds   : {seed0}..{} ({} rounds)",
        seed0 + seeds - 1,
        seeds
    );
    println!(
        " Reproduce a failure with: SEED0=<seed> SEEDS=1 cargo bench --bench cypher_fuzz_bench"
    );
    println!("============================================================");

    let started = Instant::now();
    let mut total_checks = 0usize;
    let mut failures: Vec<(u64, String)> = Vec::new();

    for seed in seed0..seed0 + seeds {
        // 图规模随种子变化：小图覆盖形状，大图覆盖分页。
        let mut size_rng = Rng::new(seed);
        let nodes = 5 + size_rng.below(60);
        let edges = size_rng.below(nodes * 2);

        let (db, model, _dir) = build(seed, nodes, edges);
        let mut rng = Rng::new(seed ^ 0xABCD_EF01);
        let mut round_checks = 0usize;

        // ---- 1. 计数：每个标签的基数 ----
        for label in LABELS {
            let q = format!("MATCH (n:{label}) RETURN count(n)");
            let exp = Value::from(model.count_label(label) as i64);
            total_checks += 1;
            round_checks += 1;
            if let Err(e) = compare(&db, &q, exp) {
                failures.push((seed, e));
            }
        }

        // ---- 2. 属性筛选 ----
        for label in LABELS {
            let v = rng.below(20) as i64;
            let q = format!("MATCH (n:{label} {{v: {v}}}) RETURN count(n)");
            let exp = Value::from(model.count_label_prop_eq(label, "v", v) as i64);
            total_checks += 1;
            round_checks += 1;
            if let Err(e) = compare(&db, &q, exp) {
                failures.push((seed, e));
            }
        }

        // ---- 3. 聚合：某标签的 v 之和 ----
        for label in LABELS {
            let q = format!("MATCH (n:{label}) RETURN sum(n.v)");
            let exp = Value::from(model.sum_prop(label, "v"));
            total_checks += 1;
            round_checks += 1;
            if let Err(e) = compare(&db, &q, exp) {
                failures.push((seed, e));
            }
        }

        // ---- 4. 边计数：按两端标签 ----
        for sl in LABELS {
            for dl in LABELS {
                let q = format!("MATCH (a:{sl})-[:R]->(b:{dl}) RETURN count(*)");
                let exp = Value::from(
                    model
                        .edges
                        .values()
                        .filter(|e| {
                            e.etype == "R"
                                && model
                                    .nodes
                                    .get(&e.src)
                                    .is_some_and(|n| n.labels.contains(sl))
                                && model
                                    .nodes
                                    .get(&e.dst)
                                    .is_some_and(|n| n.labels.contains(dl))
                        })
                        .count() as i64,
                );
                total_checks += 1;
                round_checks += 1;
                if let Err(e) = compare(&db, &q, exp) {
                    failures.push((seed, e));
                }
            }
        }

        // ---- 5. 边计数：按类型 ----
        for et in ETYPES {
            let q = format!("MATCH ()-[r:{et}]->() RETURN count(r)");
            let exp = Value::from(model.edges.values().filter(|e| e.etype == et).count() as i64);
            total_checks += 1;
            round_checks += 1;
            if let Err(e) = compare(&db, &q, exp) {
                failures.push((seed, e));
            }
        }

        // ---- 6. 与全图节点数一致性（`MATCH (n)` 必须等于模型节点数）----
        {
            let q = "MATCH (n) RETURN count(n)".to_string();
            let exp = Value::from(model.nodes.len() as i64);
            total_checks += 1;
            round_checks += 1;
            if let Err(e) = compare(&db, &q, exp) {
                failures.push((seed, e));
            }
        }

        // ---- 7. 深度：随机节点出发的 1 跳出边数，与模型对 ----
        let probe_count = 1 + rng.below(3);
        for _ in 0..probe_count {
            let nid = 1 + rng.below(nodes);
            let q = format!("MATCH (a)-[:R]->(b) WHERE id(a) = {nid} RETURN count(*)");
            let exp = Value::from(
                model
                    .edges
                    .values()
                    .filter(|e| e.etype == "R" && e.src == nid)
                    .count() as i64,
            );
            total_checks += 1;
            round_checks += 1;
            if let Err(e) = compare(&db, &q, exp) {
                failures.push((seed, e));
            }
        }

        // ---- 8. 引擎自检：随机写入后完整性必须无问题 ----
        {
            total_checks += 1;
            round_checks += 1;
            match db.integrity_check() {
                Ok(r) if r.issues.is_empty() => {}
                Ok(r) => failures.push((
                    seed,
                    format!(
                        "integrity_check reported {} issue(s): {:?}",
                        r.issues.len(),
                        r.issues
                    ),
                )),
                Err(e) => failures.push((seed, format!("integrity_check failed: {e}"))),
            }
        }

        // ---- 9. ORDER BY / SKIP / LIMIT 与模型一致 ----
        //
        // 只断言**行数**：夹具里同一属性的值取自同一集合，排序后的值序列可能相同，
        // 用「值相等」断言会掩盖长度错误；而行数错误直接说明 SKIP/LIMIT 语义有误。
        {
            let v = rng.below(20) as i64;
            let skip = rng.below(5) as usize;
            let limit = 1 + rng.below(5) as usize;
            let q = format!(
                "MATCH (n:A {{v: {v}}}) RETURN n.v ORDER BY n.v DESC SKIP {skip} LIMIT {limit}"
            );
            let matching = model.count_label_prop_eq("A", "v", v);
            let exp_rows = limit.min(matching.saturating_sub(skip));
            total_checks += 1;
            round_checks += 1;
            match db.run_cypher(&q) {
                Ok(res) if res.rows.len() == exp_rows => {}
                Ok(res) => failures.push((
                    seed,
                    format!(
                        "query: {q}\n  expected {exp_rows} row(s), got {} \
                         (matching={matching}, skip={skip}, limit={limit})",
                        res.rows.len()
                    ),
                )),
                Err(e) => failures.push((seed, format!("query: {q}\n  engine error: {e}"))),
            }
        }

        // ---- 10. min / max 与模型一致 ----
        for label in LABELS {
            let vals: Vec<i64> = model
                .nodes
                .values()
                .filter(|n| n.labels.contains(label))
                .filter_map(|n| n.props.get("v").and_then(|x| x.as_i64()))
                .collect();
            if vals.is_empty() {
                continue;
            }
            let lo = *vals.iter().min().unwrap();
            let hi = *vals.iter().max().unwrap();
            for (func, exp) in [("min", lo), ("max", hi)] {
                let q = format!("MATCH (n:{label}) RETURN {func}(n.v)");
                total_checks += 1;
                round_checks += 1;
                if let Err(e) = compare(&db, &q, Value::from(exp)) {
                    failures.push((seed, e));
                }
            }
        }

        // ---- 11. 变长路径 1..=2 跳的**路径数**，与模型一致 ----
        //
        // ## 期望值必须实现「关系唯一性」
        //
        // 本测试对这段的期望值**改过两次，两次都错**，这条注释记录它为什么是现在这样。
        //
        // 第一版算成去重可达节点数；第二版算成不加约束的路径数。两者都与引擎不符，
        // 看起来都像引擎的缺陷。核实之后是模型错了。官方 openCypher CIP
        // （CIR-2017-174）写明：
        //
        // > Cypher pattern matching assumes relationship uniqueness: A relationship can
        // > only be matched once per instance of a pattern.
        // > Pattern matching in Cypher by default only returns relationship-unique matches.
        //
        // 即：一次模式匹配中**同一条边最多用一次**。以自环 `8→8`（记为 e1）与
        // `8→5`（e2）为例，2 跳下只有 `8→8→5` 合法；`8→8→8` 需要用 e1 两次，违反
        // 唯一性。引擎返回 1 是对的。
        //
        // 模型因此必须携带**已用边集合**。这里用递归（引擎用 BFS 队列）——两条独立
        // 实现，若对唯一性的理解有分歧，这条断言会暴露出来。
        {
            let nid = 1 + rng.below(nodes);
            let mut adj: HashMap<u64, Vec<(u64, u64)>> = HashMap::new();
            for (eid, e) in &model.edges {
                if e.etype == "R" {
                    adj.entry(e.src).or_default().push((*eid, e.dst));
                }
            }
            /// 枚举 1..=2 跳路径数，携带已用边（关系唯一性）。
            fn walk(
                adj: &HashMap<u64, Vec<(u64, u64)>>,
                node: u64,
                used: &mut Vec<u64>,
                hops: usize,
                min_hops: usize,
                max_hops: usize,
            ) -> u64 {
                if hops >= max_hops {
                    return 0;
                }
                let mut count = 0u64;
                if let Some(edges) = adj.get(&node) {
                    for (eid, dst) in edges {
                        if used.contains(eid) {
                            continue; // 关系唯一性：同一条边不得重复使用
                        }
                        let next_hops = hops + 1;
                        if next_hops >= min_hops {
                            count += 1;
                        }
                        used.push(*eid);
                        count += walk(adj, *dst, used, next_hops, min_hops, max_hops);
                        used.pop();
                    }
                }
                count
            }
            let mut used = Vec::new();
            let paths = walk(&adj, nid, &mut used, 0, 1, 2);

            let q = format!("MATCH (a)-[:R*1..2]->(b) WHERE id(a) = {nid} RETURN count(b)");
            total_checks += 1;
            round_checks += 1;
            if let Err(e) = compare(&db, &q, Value::from(paths as i64)) {
                failures.push((seed, e));
            }
        }

        // ---- 12. 同一事务写入后立即可见 ----
        {
            let extra_label = format!("Fuzz{seed}");
            let n_extra = 1 + rng.below(5);
            let write = db.with_transaction(|tx| {
                for i in 0..n_extra {
                    tx.add_node(
                        HashSet::from([extra_label.clone()]),
                        HashMap::from([("k".to_string(), Value::from(i as i64))]),
                    )?;
                }
                Ok(())
            });
            total_checks += 1;
            round_checks += 1;
            match write {
                Ok(()) => {
                    let q = format!("MATCH (n:{extra_label}) RETURN count(n)");
                    if let Err(e) = compare(&db, &q, Value::from(n_extra as i64)) {
                        failures.push((seed, e));
                    }
                }
                Err(e) => failures.push((seed, format!("bulk write failed: {e}"))),
            }
        }

        if seed % 10 == 0 || !failures.is_empty() {
            println!(
                "  seed {seed:<5} nodes={nodes:<4} edges={edges:<4} checks={round_checks:<4} \
                 failures_so_far={}",
                failures.len()
            );
        }

        // 一旦有失败就停：继续跑只会产生同一原因的重复噪声。
        if !failures.is_empty() {
            break;
        }
    }

    let elapsed = started.elapsed();
    println!("\n------------------------------------------------------------");
    println!(" Checks run : {total_checks}");
    println!(" Failures   : {}", failures.len());
    println!(" Elapsed    : {:.2?}", elapsed);
    println!("------------------------------------------------------------");

    if failures.is_empty() {
        println!("CYPHER FUZZ PASSED");
    } else {
        for (seed, msg) in failures.iter().take(20) {
            println!("\n[seed {seed}] {msg}");
        }
        println!(
            "\nCYPHER FUZZ FAILED ({} distinct failure(s))",
            failures.len()
        );
        std::process::exit(1);
    }
}
