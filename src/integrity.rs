//! 数据库结构性完整性校验。
//!
//! 设计取舍：本模块**只读扫描**，不修改任何页格式，也不尝试自动修复。
//! 自动修复需要独立的策略设计（且必须用户显式授权），不属于校验的职责。
//!
//! 校验依据的是「守恒律」而非抽样：同一量用两条独立口径计算并比对，
//! 因此不需要第二份实现就能发现不一致。

use crate::disk_graph::DiskGraph;
use crate::graph::GraphError;

/// 单条完整性问题
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityIssue {
    /// 问题分类（便于程序化断言与统计）
    pub kind: IntegrityIssueKind,
    /// 人类可读的细节
    pub detail: String,
}

/// 完整性问题分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityIssueKind {
    /// 页头（魔数/版本/页大小）非法
    HeaderInvalid,
    /// 节点记录声明的 id 与位置不符
    NodeRecordMismatch,
    /// 边引用了不存在的节点
    DanglingEdge,
    /// 出边链长度与该节点作为源的实际边数不符
    OutgoingChainCountMismatch,
    /// 入边链长度与该节点作为目标的实际边数不符
    IncomingChainCountMismatch,
    /// 链指针出现环或超出有效范围
    ChainCycleOrOutOfRange,
    /// 节点/边计数与扫描结果不符
    CountMismatch,
    /// Freelist 出现环
    FreelistCycle,
    /// 属性记录不可读取（页损坏或不存在的槽位）
    PropertyUnreadable,
    /// 自增 id 小于已使用的 id（单调性被破坏）
    SequenceRegression,
}

impl IntegrityIssueKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HeaderInvalid => "header_invalid",
            Self::NodeRecordMismatch => "node_record_mismatch",
            Self::DanglingEdge => "dangling_edge",
            Self::OutgoingChainCountMismatch => "outgoing_chain_count_mismatch",
            Self::IncomingChainCountMismatch => "incoming_chain_count_mismatch",
            Self::ChainCycleOrOutOfRange => "chain_cycle_or_out_of_range",
            Self::CountMismatch => "count_mismatch",
            Self::FreelistCycle => "freelist_cycle",
            Self::PropertyUnreadable => "property_unreadable",
            Self::SequenceRegression => "sequence_regression",
        }
    }
}

/// 校验结果报告
#[derive(Debug, Clone, Default)]
pub struct IntegrityReport {
    /// 已扫描的存活节点数
    pub nodes_checked: usize,
    /// 已扫描的存活边数
    pub edges_checked: usize,
    /// 发现的全部问题（按发现顺序）
    pub issues: Vec<IntegrityIssue>,
}

impl IntegrityReport {
    /// 是否通过校验
    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }

    /// 按分类统计问题数量
    pub fn count_of(&self, kind: IntegrityIssueKind) -> usize {
        self.issues.iter().filter(|i| i.kind == kind).count()
    }

    fn push(&mut self, kind: IntegrityIssueKind, detail: impl Into<String>) {
        self.issues.push(IntegrityIssue {
            kind,
            detail: detail.into(),
        });
    }

    /// 汇总为单条错误（供 `Result` 返回）
    pub fn into_error(self) -> GraphError {
        let summary = self
            .issues
            .iter()
            .take(5)
            .map(|i| format!("[{}] {}", i.kind.as_str(), i.detail))
            .collect::<Vec<_>>()
            .join("; ");
        GraphError::IntegrityError(format!(
            "{} issue(s) found ({} nodes / {} edges checked): {}{}",
            self.issues.len(),
            self.nodes_checked,
            self.edges_checked,
            summary,
            if self.issues.len() > 5 { "; ..." } else { "" }
        ))
    }
}

/// 扫描范围上限，避免损坏文件导致无界扫描
const MAX_FREELIST_WALK: usize = 10_000_000;

/// 执行结构性完整性校验（只读，不修改任何状态）。
pub fn check_integrity(graph: &DiskGraph) -> Result<IntegrityReport, GraphError> {
    let mut report = IntegrityReport::default();

    // ---------- 1. 枚举存活节点 ----------
    // all_node_ids 依赖 header 中的 node_count 提前终止，因此先做计数守恒校验。
    let node_ids = graph.all_node_ids()?;
    report.nodes_checked = node_ids.len();

    // ---------- 2. 计数守恒 ----------
    if node_ids.len() != graph.node_count {
        report.push(
            IntegrityIssueKind::CountMismatch,
            format!(
                "header node_count={} but scan found {} live nodes",
                graph.node_count,
                node_ids.len()
            ),
        );
    }

    let max_node_id = node_ids.iter().copied().max().unwrap_or(0);
    if max_node_id >= graph.next_node_id {
        report.push(
            IntegrityIssueKind::SequenceRegression,
            format!(
                "max live node id {} >= next_node_id {}",
                max_node_id, graph.next_node_id
            ),
        );
    }

    // ---------- 3. 逐节点校验：属性可读 + 出/入边链健全 ----------
    // 守恒 oracle：链口径的度数由「沿链指针走出」得到，
    // 期望度数由「遍历全部边记录」独立得到，二者稍后必须相等。
    let mut chain_out_deg: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    let mut chain_in_deg: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();

    let mut live_nodes: std::collections::HashSet<u64> =
        std::collections::HashSet::with_capacity(node_ids.len());

    for &nid in &node_ids {
        let record = match graph.read_node_record(nid)? {
            Some(r) => r,
            None => {
                report.push(
                    IntegrityIssueKind::NodeRecordMismatch,
                    format!("node {} listed as live but its record is not in use", nid),
                );
                continue;
            }
        };
        live_nodes.insert(nid);

        // 属性载荷必须可完整反序列化（多页溢出链损坏会在此暴露）
        if let Err(e) = graph.read_node_data(record.prop_page_id) {
            report.push(
                IntegrityIssueKind::PropertyUnreadable,
                format!("node {} property payload unreadable: {}", nid, e),
            );
        }

        // 出边链：每条边必须确实以本节点为源，且链不得自环
        let out_ids = graph.collect_outgoing_edge_ids(record.first_outgoing_edge_id)?;
        let mut out_seen = std::collections::HashSet::new();
        let mut valid_out = 0usize;
        for &eid in &out_ids {
            if !out_seen.insert(eid) {
                report.push(
                    IntegrityIssueKind::ChainCycleOrOutOfRange,
                    format!("node {} outgoing chain revisits edge {}", nid, eid),
                );
                break;
            }
            match graph.read_edge_record(eid)? {
                Some(edge) => {
                    if edge.src_id != nid {
                        report.push(
                            IntegrityIssueKind::OutgoingChainCountMismatch,
                            format!(
                                "edge {} on node {} outgoing chain has src_id {}",
                                eid, nid, edge.src_id
                            ),
                        );
                    } else {
                        valid_out += 1;
                    }
                }
                None => report.push(
                    IntegrityIssueKind::DanglingEdge,
                    format!(
                        "node {} outgoing chain references missing edge {}",
                        nid, eid
                    ),
                ),
            }
        }
        chain_out_deg.insert(nid, valid_out);

        // 入边链：每条边必须确实以本节点为目标
        let in_ids = graph.collect_incoming_edge_ids(record.first_incoming_edge_id)?;
        let mut in_seen = std::collections::HashSet::new();
        let mut valid_in = 0usize;
        for &eid in &in_ids {
            if !in_seen.insert(eid) {
                report.push(
                    IntegrityIssueKind::ChainCycleOrOutOfRange,
                    format!("node {} incoming chain revisits edge {}", nid, eid),
                );
                break;
            }
            match graph.read_edge_record(eid)? {
                Some(edge) => {
                    if edge.dst_id != nid {
                        report.push(
                            IntegrityIssueKind::IncomingChainCountMismatch,
                            format!(
                                "edge {} on node {} incoming chain has dst_id {}",
                                eid, nid, edge.dst_id
                            ),
                        );
                    } else {
                        valid_in += 1;
                    }
                }
                None => report.push(
                    IntegrityIssueKind::DanglingEdge,
                    format!(
                        "node {} incoming chain references missing edge {}",
                        nid, eid
                    ),
                ),
            }
        }
        chain_in_deg.insert(nid, valid_in);
    }

    // ---------- 4. 边集合与度数守恒 ----------
    // 独立口径：**直接扫遍 edge id 空间**读取 in-use 边记录。
    // 这与「沿链指针走出」完全独立，因此两者的度数比对是真正的守恒 oracle，
    // 而非同一遍历的自我印证。
    let mut expected_out: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    let mut expected_in: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    let mut all_edge_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

    let mut eid = 1u64;
    while eid < graph.next_edge_id {
        if let Some(edge) = graph.read_edge_record(eid)? {
            all_edge_ids.insert(eid);
            *expected_out.entry(edge.src_id).or_insert(0) += 1;
            *expected_in.entry(edge.dst_id).or_insert(0) += 1;

            if !live_nodes.contains(&edge.src_id) || !live_nodes.contains(&edge.dst_id) {
                report.push(
                    IntegrityIssueKind::DanglingEdge,
                    format!(
                        "edge {} connects {} -> {}, referencing a non-live node",
                        eid, edge.src_id, edge.dst_id
                    ),
                );
            }
        }
        eid += 1;
    }

    report.edges_checked = all_edge_ids.len();

    if all_edge_ids.len() != graph.edge_count {
        report.push(
            IntegrityIssueKind::CountMismatch,
            format!(
                "header edge_count={} but only {} live edge records exist",
                graph.edge_count,
                all_edge_ids.len()
            ),
        );
    }

    // 度数守恒：链口径必须等于独立扫描口径。
    // 链指针被破坏时，链会变短或走偏，此处即报出。
    for &nid in &node_ids {
        let actual_out = chain_out_deg.get(&nid).copied().unwrap_or(0);
        let expect_out = expected_out.get(&nid).copied().unwrap_or(0);
        if actual_out != expect_out {
            report.push(
                IntegrityIssueKind::OutgoingChainCountMismatch,
                format!(
                    "node {} outgoing chain length {} != actual out-degree {}",
                    nid, actual_out, expect_out
                ),
            );
        }

        let actual_in = chain_in_deg.get(&nid).copied().unwrap_or(0);
        let expect_in = expected_in.get(&nid).copied().unwrap_or(0);
        if actual_in != expect_in {
            report.push(
                IntegrityIssueKind::IncomingChainCountMismatch,
                format!(
                    "node {} incoming chain length {} != actual in-degree {}",
                    nid, actual_in, expect_in
                ),
            );
        }
    }

    // ---------- 5. Freelist 环检测 ----------
    let mut seen = std::collections::HashSet::new();
    let mut cur = graph.first_free_node_id;
    let mut steps = 0usize;
    while cur != 0 && steps < MAX_FREELIST_WALK {
        if !seen.insert(cur) {
            report.push(
                IntegrityIssueKind::FreelistCycle,
                format!("node freelist cycles at id {}", cur),
            );
            break;
        }
        match graph.read_node_record_raw(cur)? {
            Some(r) => cur = r.first_outgoing_edge_id,
            None => break,
        }
        steps += 1;
    }

    let mut seen = std::collections::HashSet::new();
    let mut cur = graph.first_free_edge_id;
    let mut steps = 0usize;
    while cur != 0 && steps < MAX_FREELIST_WALK {
        if !seen.insert(cur) {
            report.push(
                IntegrityIssueKind::FreelistCycle,
                format!("edge freelist cycles at id {}", cur),
            );
            break;
        }
        match graph.read_edge_record_raw(cur)? {
            Some(r) => cur = r.src_next_edge_id,
            None => break,
        }
        steps += 1;
    }

    Ok(report)
}
