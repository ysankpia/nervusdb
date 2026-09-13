//! 多模式 `MATCH` 的代价模型与连接定序。
//!
//! ## 这里解决什么问题
//!
//! `MATCH (a:P)-[:R]->(b), (b:P)-[:R]->(c)` 有两个模式、共享变量 `b`。
//! 原来的执行器把**每个模式各自**求出完整匹配集再相乘连接：第二个模式会先在图里
//! 展开**全部** `(b)-[:R]->(c)`，然后才用 `b` 做一致性过滤。当第一个模式已经把 `b`
//! 收敛到一个节点时，那些展开是纯浪费。
//!
//! 现在做两件事：
//!
//! 1. **定序**：按估计基数从小到大求解模式，并优先选择与已求解模式**共享变量**的
//!    模式，避免产生笛卡尔积。
//! 2. **索引嵌套循环**：若某模式的起点变量已由前面的模式绑定，就只从那个节点展开，
//!    而不是先算出该模式的全部匹配。
//!
//! 第 2 条是真正的算法改进（复杂度从「两遍全展开」变为「按绑定驱动」），第 1 条
//! 决定第 2 条能不能生效——共享变量的模式若不排在后面，绑定就不存在。
//!
//! ## 代价模型诚实说明
//!
//! 估计值分成两类，不混为一谈：
//!
//! - **实测**：起点候选数。标签索引与 `(label, prop)` 等值索引里存的就是真实集合，
//!   直接取 `len()`，不是估计。
//! - **估计**：每一步展开的扇出。图存储里**没有**按关系类型统计的边数，也没有度数
//!   直方图，所以用平均度数 `2E/N` 近似；目标节点带标签时按该标签的节点占比缩放。
//!
//! 因此这个模型**能**分辨「有索引的等值点查」与「全表扫描」这种数量级差异，
//! **不能**分辨两个候选数相近的模式谁更便宜。EXPLAIN 会把每个模式的估计值和它的
//! 依据一起打印出来，让人能看出哪些数字是实测、哪些是近似——而不是看到一个
//! 精确到小数点的数字就以为它是量出来的。

use crate::cypher::ast::{Expr, NodePattern, PathPattern};
use crate::disk_graph::DiskGraph;
use crate::graph::Direction;
use crate::index::IndexManager;
use std::collections::BTreeSet;

/// 规划器可见的统计量。
///
/// 只读取已经存在的结构（`DiskGraph` 的计数、`IndexManager` 的索引），
/// 不为了规划去扫描磁盘——那会让「规划」比「执行」还贵。
pub struct PlanStats<'a> {
    graph: &'a DiskGraph,
    index_mgr: &'a IndexManager,
}

impl<'a> PlanStats<'a> {
    pub fn new(graph: &'a DiskGraph, index_mgr: &'a IndexManager) -> Self {
        Self { graph, index_mgr }
    }

    /// 平均度数（无向口径 `2E / N`）。
    ///
    /// 无向口径是刻意的：它同时作为出边与入边展开的粗略上界，两个方向都不至于
    /// 低估到 0。空图返回 0。
    pub fn average_degree(&self) -> f64 {
        if self.graph.node_count == 0 {
            0.0
        } else {
            (self.graph.edge_count as f64 * 2.0) / self.graph.node_count as f64
        }
    }

    /// 某标签的基数，以及该数字是否**实测**。
    ///
    /// 索引未建立时退化为全表节点数，第二个返回值为 `false`——调用方据此在
    /// EXPLAIN 里标明「这是上界，不是实测」。
    pub fn label_cardinality(&self, label: &str) -> (f64, bool) {
        if self.index_mgr.is_label_complete(label) {
            if let Some(set) = self.index_mgr.find_by_label(label) {
                return (set.len() as f64, true);
            }
        }
        (self.graph.node_count as f64, false)
    }

    /// 等值属性点查的候选数（实测），返回 `None` 表示索引里没有这个键或索引未建。
    pub fn equality_candidates(
        &self,
        label: &str,
        key: &str,
        value: &crate::graph::Value,
    ) -> Option<f64> {
        if !self.index_mgr.is_label_complete(label) {
            return None;
        }
        self.index_mgr
            .find_by_property_exact(label, key, value)
            .map(|set| set.len() as f64)
    }
}

/// 一个模式的基数估计。
#[derive(Debug, Clone, PartialEq)]
pub struct PatternEstimate {
    /// 估计产出行数（上界口径，见模块文档）
    pub rows: f64,
    /// 起点候选数及其来源（EXPLAIN 展示用）
    pub start_basis: String,
    /// 起点候选数是否为实测（索引里数出来的）而非估计
    pub start_is_measured: bool,
}

/// 定序后的一个模式。
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedPattern {
    /// 该模式在原 `MatchClause::patterns` 中的位置
    pub index: usize,
    pub estimate: PatternEstimate,
    /// 起点变量是否由**前面**的模式绑定（决定能否用索引嵌套循环）
    pub driven_by_binding: bool,
}

/// 估计单个模式产出的行数。
pub fn estimate_pattern(pattern: &PathPattern, stats: &PlanStats) -> PatternEstimate {
    let Some(start) = pattern.nodes.first() else {
        // 空模式不可能产出任何行（`find_single_pattern_matches` 同样返回空）
        return PatternEstimate {
            rows: 0.0,
            start_basis: "empty pattern".to_string(),
            start_is_measured: true,
        };
    };

    let (mut rows, start_basis, start_is_measured) = estimate_start(start, stats);

    let avg_degree = stats.average_degree();
    let total_nodes = stats.node_count_f64();

    for (i, edge) in pattern.edges.iter().enumerate() {
        let mut fanout = if avg_degree > 0.0 { avg_degree } else { 1.0 };
        if edge.direction == Direction::Both {
            fanout *= 2.0;
        }
        // 变长模式用**上界**跳数：估计宁可偏大而不漏，偏大只会让定序差一点，
        // 偏小会让规划器误以为一个昂贵模式很便宜。
        if let Some((_, hi)) = edge.hops {
            fanout *= hi.max(1) as f64;
        }

        // 目标节点带标签时，按该标签占比收窄扇出
        if let Some(target) = pattern.nodes.get(i + 1) {
            if let Some(lbl) = target.labels.first() {
                let (card, measured) = stats.label_cardinality(lbl);
                if measured && total_nodes > 0.0 {
                    fanout *= (card / total_nodes).min(1.0);
                }
            }
        }

        rows *= fanout;
        rows = clamp_estimate(rows);
    }

    PatternEstimate {
        rows,
        start_basis,
        start_is_measured,
    }
}

/// 起点候选数的估计（含决定它是实测还是近似）。
fn estimate_start(start: &NodePattern, stats: &PlanStats) -> (f64, String, bool) {
    let total = stats.node_count_f64();

    if let Some(lbl) = start.labels.first() {
        // 与 `find_initial_candidates` 同一顺序：先看字面量属性等值索引
        for (key, val_expr) in &start.properties {
            if let Expr::Literal(val) = val_expr {
                if let Some(n) = stats.equality_candidates(lbl, key, val) {
                    return (
                        n,
                        format!("property index (:{lbl} {{{key}: {val:?}}})"),
                        true,
                    );
                }
            }
        }
        let (card, measured) = stats.label_cardinality(lbl);
        if measured {
            return (card, format!("label index (:{lbl})"), true);
        }
        return (
            total,
            format!("full scan (label :{lbl} 的索引尚未建立)"),
            false,
        );
    }

    (
        total,
        "full scan (起点无标签约束，将遍历全部节点)".to_string(),
        false,
    )
}

/// 迭代中反复相乘会溢出到 `inf`，那样所有模式都变成「一样贵」而定序失效。
/// 截断到一个仍然远大于任何真实图规模的有限值。
fn clamp_estimate(rows: f64) -> f64 {
    const CEILING: f64 = 1e18;
    if !rows.is_finite() || rows < 0.0 {
        return CEILING;
    }
    rows.min(CEILING)
}

impl<'a> PlanStats<'a> {
    fn node_count_f64(&self) -> f64 {
        self.graph.node_count as f64
    }
}

/// 该模式绑定的变量（节点与边）。
fn pattern_binds(pattern: &PathPattern) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for n in &pattern.nodes {
        if let Some(v) = &n.variable {
            set.insert(v.clone());
        }
    }
    for e in &pattern.edges {
        if let Some(v) = &e.variable {
            set.insert(v.clone());
        }
    }
    set
}

/// 模式的起点变量。
fn start_variable(pattern: &PathPattern) -> Option<&str> {
    pattern.nodes.first().and_then(|n| n.variable.as_deref())
}

/// 贪心定序：基数最小者优先，且优先选择与已定序模式共享变量的模式。
///
/// ## 为什么先连通性、再基数
///
/// 一个基数极小的模式，如果与其它模式**没有共享变量**，把它排在前面只会先产生
/// 一批行、再与后面的模式做笛卡尔积——中间结果被放大。所以选择顺序是：
///
/// 1. 首选「与已定序集合连通」的模式（共享变量意味着能走索引嵌套循环）；
/// 2. 其中取估计基数最小者；
/// 3. 全部都不连通时才退化为纯基数排序（此时笛卡尔积不可避免）。
///
/// 同分一律按原 AST 位置升序，使定序**确定**——否则同一查询两次跑出不同的计划，
/// 而计划是 EXPLAIN 的输出、也是测试断言的对象。
pub fn plan_pattern_order(patterns: &[PathPattern], stats: &PlanStats) -> Vec<PlannedPattern> {
    let estimates: Vec<PatternEstimate> = patterns
        .iter()
        .map(|p| estimate_pattern(p, stats))
        .collect();
    let binds: Vec<BTreeSet<String>> = patterns.iter().map(pattern_binds).collect();

    let mut remaining: Vec<usize> = (0..patterns.len()).collect();
    let mut ordered: Vec<PlannedPattern> = Vec::with_capacity(patterns.len());
    let mut bound: BTreeSet<String> = BTreeSet::new();

    while !remaining.is_empty() {
        // 先看有没有与已绑定变量连通的；没有则全体参与选择
        let connected: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|&i| !binds[i].is_disjoint(&bound))
            .collect();
        let pool = if connected.is_empty() {
            remaining.clone()
        } else {
            connected
        };

        let Some(&best) = pool.iter().min_by(|&&a, &&b| {
            // 基数升序，同分按原位置升序
            estimates[a]
                .rows
                .partial_cmp(&estimates[b].rows)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        }) else {
            break;
        };

        let driven = start_variable(&patterns[best]).is_some_and(|v| bound.contains(v));

        ordered.push(PlannedPattern {
            index: best,
            estimate: estimates[best].clone(),
            driven_by_binding: driven,
        });

        bound.extend(binds[best].iter().cloned());
        remaining.retain(|&i| i != best);
    }

    ordered
}
