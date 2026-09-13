#![allow(clippy::type_complexity)]

use crate::disk_graph::DiskGraph;
use crate::graph::{Direction, Edge, Node, Value};
use std::collections::{HashSet, VecDeque};

/// 单跳匹配结果
#[derive(Debug, Clone, PartialEq)]
pub struct PathMatch {
    pub src: Node,
    pub edge: Edge,
    pub dst: Node,
}

/// 多跳遍历路径
#[derive(Debug, Clone, PartialEq)]
pub struct MultiHopPath {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl MultiHopPath {
    pub fn start_node(&self) -> Option<&Node> {
        self.nodes.first()
    }

    pub fn end_node(&self) -> Option<&Node> {
        self.nodes.last()
    }

    pub fn hop_count(&self) -> usize {
        self.edges.len()
    }
}

/// 查询执行结果集
#[derive(Debug, Clone, Default)]
pub struct QueryResult {
    pub paths: Vec<PathMatch>,
    pub multi_hop_paths: Vec<MultiHopPath>,
}

impl QueryResult {
    pub fn new(paths: Vec<PathMatch>, multi_hop_paths: Vec<MultiHopPath>) -> Self {
        Self {
            paths,
            multi_hop_paths,
        }
    }

    pub fn paths(&self) -> &[PathMatch] {
        &self.paths
    }

    pub fn multi_hop_paths(&self) -> &[MultiHopPath] {
        &self.multi_hop_paths
    }

    /// 获取结果中所有涉及的去重节点
    pub fn nodes(&self) -> Vec<Node> {
        let mut set = HashSet::new();
        let mut result = Vec::new();

        for p in &self.paths {
            if set.insert(p.src.id) {
                result.push(p.src.clone());
            }
            if set.insert(p.dst.id) {
                result.push(p.dst.clone());
            }
        }

        for mp in &self.multi_hop_paths {
            for n in &mp.nodes {
                if set.insert(n.id) {
                    result.push(n.clone());
                }
            }
        }

        result
    }

    /// 获取所有目标节点（单跳的 dst 或多跳路径的终点）
    pub fn dst_nodes(&self) -> Vec<Node> {
        let mut set = HashSet::new();
        let mut result = Vec::new();

        for p in &self.paths {
            if set.insert(p.dst.id) {
                result.push(p.dst.clone());
            }
        }

        for mp in &self.multi_hop_paths {
            if let Some(last) = mp.nodes.last() {
                if set.insert(last.id) {
                    result.push(last.clone());
                }
            }
        }

        result
    }

    /// 获取所有源节点
    pub fn src_nodes(&self) -> Vec<Node> {
        let mut set = HashSet::new();
        let mut result = Vec::new();

        for p in &self.paths {
            if set.insert(p.src.id) {
                result.push(p.src.clone());
            }
        }

        for mp in &self.multi_hop_paths {
            if let Some(first) = mp.nodes.first() {
                if set.insert(first.id) {
                    result.push(first.clone());
                }
            }
        }

        result
    }

    /// 获取匹配的所有边
    pub fn edges(&self) -> Vec<Edge> {
        let mut set = HashSet::new();
        let mut result = Vec::new();

        for p in &self.paths {
            if set.insert(p.edge.id) {
                result.push(p.edge.clone());
            }
        }

        for mp in &self.multi_hop_paths {
            for e in &mp.edges {
                if set.insert(e.id) {
                    result.push(e.clone());
                }
            }
        }

        result
    }

    pub fn count(&self) -> usize {
        if !self.paths.is_empty() {
            self.paths.len()
        } else {
            self.multi_hop_paths.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.multi_hop_paths.is_empty()
    }
}

/// 链式图查询构造器（纯磁盘游标执行，零全图驻留）。
///
/// ## 它不持有图句柄
///
/// 它只描述「要查什么」（模式 / 遍历配置 / 各种筛选 / 上限），图句柄在
/// [`Self::execute`] 时**作为参数传入**。
///
/// 这样设计是为了让 [`GraphQuery`] 能安全地「拥有一份图 + 一份查询描述」：
/// 若本类型内部存 `&'a DiskGraph`，而外层又要同时持有那个 `DiskGraph`，就构成
/// 自引用结构体——在没有第三方 crate（本项目零依赖）的前提下只能靠 `unsafe`，
/// 而公开类型里的悬垂引用是 UB。把借用推迟到 `execute` 就完全避开了这个问题。
pub struct QueryBuilder {
    pattern: Option<(String, String, String)>,
    traverse_config: Option<(u64, String, Direction, usize)>,
    prop_filters: Vec<(String, Box<dyn Fn(&Value) -> bool>)>,
    src_filters: Vec<Box<dyn Fn(&Node) -> bool>>,
    dst_filters: Vec<Box<dyn Fn(&Node) -> bool>>,
    edge_filters: Vec<Box<dyn Fn(&Edge) -> bool>>,
    limit: Option<usize>,
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryBuilder {
    /// 新建一个空的查询描述（未指定模式、无筛选、无上限）。
    pub fn new() -> Self {
        Self {
            pattern: None,
            traverse_config: None,
            prop_filters: Vec::new(),
            src_filters: Vec::new(),
            dst_filters: Vec::new(),
            edge_filters: Vec::new(),
            limit: None,
        }
    }

    pub fn match_pattern(mut self, src_label: &str, edge_type: &str, dst_label: &str) -> Self {
        self.pattern = Some((
            src_label.to_string(),
            edge_type.to_string(),
            dst_label.to_string(),
        ));
        self
    }

    pub fn traverse(
        mut self,
        start_node_id: u64,
        edge_type: &str,
        direction: Direction,
        hops: usize,
    ) -> Self {
        self.traverse_config = Some((start_node_id, edge_type.to_string(), direction, hops));
        self
    }

    pub fn filter_prop<F>(mut self, key: &str, predicate: F) -> Self
    where
        F: Fn(&Value) -> bool + 'static,
    {
        self.prop_filters
            .push((key.to_string(), Box::new(predicate)));
        self
    }

    pub fn filter_src<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Node) -> bool + 'static,
    {
        self.src_filters.push(Box::new(predicate));
        self
    }

    pub fn filter_dst<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Node) -> bool + 'static,
    {
        self.dst_filters.push(Box::new(predicate));
        self
    }

    pub fn filter_edge<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Edge) -> bool + 'static,
    {
        self.edge_filters.push(Box::new(predicate));
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn execute(&self, graph: &DiskGraph) -> QueryResult {
        if let Some((start_node_id, ref edge_type, direction, hops)) = self.traverse_config {
            self.execute_traverse(graph, start_node_id, edge_type, direction, hops)
        } else if let Some((ref src_label, ref edge_type, ref dst_label)) = self.pattern {
            self.execute_pattern(graph, src_label, edge_type, dst_label)
        } else {
            self.execute_pattern(graph, "", "", "")
        }
    }

    fn execute_pattern(
        &self,
        graph: &DiskGraph,
        src_label: &str,
        edge_type: &str,
        dst_label: &str,
    ) -> QueryResult {
        let mut matched_paths = Vec::new();

        let match_any_src = src_label.is_empty() || src_label == "*";
        let match_any_edge = edge_type.is_empty() || edge_type == "*";
        let match_any_dst = dst_label.is_empty() || dst_label == "*";

        let node_ids = graph.all_node_ids().unwrap_or_default();
        let mut node_cache: std::collections::HashMap<u64, Node> = std::collections::HashMap::new();

        for node_id in node_ids {
            // 先通过定长 32 字节 NodeRecord 快速过滤，无需调入溢出页
            let node_record = match graph.read_node_record(node_id).ok().flatten() {
                Some(r) => r,
                None => continue,
            };

            if node_record.first_outgoing_edge_id == 0 {
                continue;
            }

            if !match_any_src {
                if let Some(lbl) = graph.dict.resolve(node_record.label_id) {
                    if lbl != src_label {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            let src_node = if let Some(n) = node_cache.get(&node_id) {
                n.clone()
            } else if let Ok(Some(n)) = graph.get_node(node_id) {
                node_cache.insert(node_id, n.clone());
                n
            } else {
                continue;
            };

            if !self.src_filters.iter().all(|f| f(&src_node)) {
                continue;
            }

            // 沿磁盘指针遍历出边
            let edges = graph.outgoing_edges(node_id).unwrap_or_default();
            for edge in edges {
                if !match_any_edge && edge.edge_type != edge_type {
                    continue;
                }

                if !self.edge_filters.iter().all(|f| f(&edge)) {
                    continue;
                }

                let dst_node = if let Some(n) = node_cache.get(&edge.dst_id) {
                    n.clone()
                } else if let Ok(Some(n)) = graph.get_node(edge.dst_id) {
                    node_cache.insert(edge.dst_id, n.clone());
                    n
                } else {
                    continue;
                };

                if !match_any_dst && !dst_node.has_label(dst_label) {
                    continue;
                }

                if !self.dst_filters.iter().all(|f| f(&dst_node)) {
                    continue;
                }

                let mut prop_match = true;
                for (key, pred) in &self.prop_filters {
                    let mut found = false;
                    if let Some(val) = src_node.get_prop(key) {
                        if pred(val) {
                            found = true;
                        }
                    }
                    if !found {
                        if let Some(val) = dst_node.get_prop(key) {
                            if pred(val) {
                                found = true;
                            }
                        }
                    }
                    if !found {
                        if let Some(val) = edge.get_prop(key) {
                            if pred(val) {
                                found = true;
                            }
                        }
                    }
                    if !found {
                        prop_match = false;
                        break;
                    }
                }

                if !prop_match {
                    continue;
                }

                matched_paths.push(PathMatch {
                    src: src_node.clone(),
                    edge,
                    dst: dst_node,
                });

                if let Some(max_limit) = self.limit {
                    if matched_paths.len() >= max_limit {
                        return QueryResult::new(matched_paths, Vec::new());
                    }
                }
            }
        }

        QueryResult::new(matched_paths, Vec::new())
    }

    fn execute_traverse(
        &self,
        graph: &DiskGraph,
        start_id: u64,
        edge_type: &str,
        direction: Direction,
        hops: usize,
    ) -> QueryResult {
        let start_node = match graph.get_node(start_id).ok().flatten() {
            Some(n) => n,
            None => return QueryResult::default(),
        };

        // 起点筛选必须在这里应用。
        //
        // 此前本函数**完全忽略** `src_filters` 与 `edge_filters`（只检查 `dst_filters`
        // 与 `prop_filters`），于是 `filter_src` / `filter_edge` 在 traverse 路径上是
        // **静默空操作**：调用方设了筛选，结果却拿到未筛选的路径，且没有任何报错。
        // `execute_pattern` 四个筛选器都应用，所以这个缺口只影响 traverse。
        //
        // 之所以长期没被发现：这两个方法改造前全项目没有任何调用点（见 #16）。
        if !self.src_filters.iter().all(|f| f(&start_node)) {
            return QueryResult::default();
        }

        if hops == 0 {
            return QueryResult::default();
        }

        let match_any_edge = edge_type.is_empty() || edge_type == "*";

        let mut queue: VecDeque<(u64, Vec<Node>, Vec<Edge>)> = VecDeque::new();
        queue.push_back((start_id, vec![start_node], Vec::new()));

        let mut final_paths = Vec::new();

        while let Some((current_id, current_nodes, current_edges)) = queue.pop_front() {
            let current_hop = current_edges.len();

            if current_hop == hops {
                if let Some(end_node) = current_nodes.last() {
                    if end_node.id == start_id {
                        continue;
                    }

                    if !self.dst_filters.iter().all(|f| f(end_node)) {
                        continue;
                    }

                    let mut prop_match = true;
                    for (key, pred) in &self.prop_filters {
                        let mut found = false;
                        if let Some(val) = end_node.get_prop(key) {
                            if pred(val) {
                                found = true;
                            }
                        }
                        if !found {
                            for e in &current_edges {
                                if let Some(val) = e.get_prop(key) {
                                    if pred(val) {
                                        found = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if !found {
                            prop_match = false;
                            break;
                        }
                    }

                    if prop_match {
                        final_paths.push(MultiHopPath {
                            nodes: current_nodes,
                            edges: current_edges,
                        });

                        if let Some(max_limit) = self.limit {
                            if final_paths.len() >= max_limit {
                                break;
                            }
                        }
                    }
                }
                continue;
            }

            // 获取符合方向的边列表（按页从磁盘中调入）
            let candidate_edges = match direction {
                Direction::Outgoing => graph.outgoing_edges(current_id).unwrap_or_default(),
                Direction::Incoming => graph.incoming_edges(current_id).unwrap_or_default(),
                Direction::Both => {
                    let mut both = graph.outgoing_edges(current_id).unwrap_or_default();
                    both.extend(graph.incoming_edges(current_id).unwrap_or_default());
                    both
                }
            };

            for edge in candidate_edges {
                if !match_any_edge && edge.edge_type != edge_type {
                    continue;
                }

                // 边筛选作用于**每一跳的每条边**，不只最后一跳：调用方说
                // `filter_edge(...)` 时，路径上任何一条不满足的边都应使该路径被排除。
                // 此前这里没有这一步，`filter_edge` 因此是静默空操作（见函数开头说明）。
                if !self.edge_filters.iter().all(|f| f(&edge)) {
                    continue;
                }

                let next_node_id = match direction {
                    Direction::Outgoing => edge.dst_id,
                    Direction::Incoming => edge.src_id,
                    Direction::Both => {
                        if edge.src_id == current_id {
                            edge.dst_id
                        } else {
                            edge.src_id
                        }
                    }
                };

                if current_nodes.iter().any(|n| n.id == next_node_id) {
                    continue;
                }

                if let Ok(Some(next_node)) = graph.get_node(next_node_id) {
                    let mut new_nodes = current_nodes.clone();
                    new_nodes.push(next_node);

                    let mut new_edges = current_edges.clone();
                    new_edges.push(edge);

                    queue.push_back((next_node_id, new_nodes, new_edges));
                }
            }
        }

        QueryResult::new(Vec::new(), final_paths)
    }
}

/// 拥有图句柄的独立链式查询构建器（纯磁盘游标运行）。
///
/// ## 它为什么是薄的
///
/// 这里曾经有**两套完全一样的东西**：本类型与 [`QueryBuilder`]，字段逐项相同、
/// 九个构建方法逐行等价，唯一差别是本类型**持有** `DiskGraph` 而 `QueryBuilder`
/// **借用**它。本类型的 `execute` 只是把字段一个个搬进 `QueryBuilder` 再调用。
///
/// 抄两遍的代价是：筛选逻辑改一处就得记得改另一处，而两处都不会报错。现在只有
/// [`QueryBuilder`] 一份实现，本类型只负责「拥有一份图句柄」，其余全部转发。
///
/// ## 为什么可以不用 `unsafe`
///
/// [`QueryBuilder`] 已经不再存储图引用（借用推迟到 `execute`），因此本类型可以
/// 同时持有 `graph` 与 `builder` 两个独立字段，不构成自引用。早期的写法若让
/// `QueryBuilder` 存 `&'a DiskGraph`，这里就只能用 `unsafe` 造自引用——公开类型
/// 里的悬垂引用是 UB，零依赖下没有安全的替代品。
pub struct GraphQuery {
    graph: DiskGraph,
    builder: QueryBuilder,
}

impl GraphQuery {
    pub fn new(graph: DiskGraph) -> Self {
        Self {
            graph,
            builder: QueryBuilder::new(),
        }
    }

    pub fn match_pattern(mut self, src_label: &str, edge_type: &str, dst_label: &str) -> Self {
        self.builder = self.builder.match_pattern(src_label, edge_type, dst_label);
        self
    }

    pub fn traverse(
        mut self,
        start_node_id: u64,
        edge_type: &str,
        direction: Direction,
        hops: usize,
    ) -> Self {
        self.builder = self
            .builder
            .traverse(start_node_id, edge_type, direction, hops);
        self
    }

    pub fn filter_prop<F>(mut self, key: &str, predicate: F) -> Self
    where
        F: Fn(&Value) -> bool + 'static,
    {
        self.builder = self.builder.filter_prop(key, predicate);
        self
    }

    pub fn filter_src<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Node) -> bool + 'static,
    {
        self.builder = self.builder.filter_src(predicate);
        self
    }

    pub fn filter_dst<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Node) -> bool + 'static,
    {
        self.builder = self.builder.filter_dst(predicate);
        self
    }

    pub fn filter_edge<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Edge) -> bool + 'static,
    {
        self.builder = self.builder.filter_edge(predicate);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.builder = self.builder.limit(limit);
        self
    }

    pub fn execute(self) -> QueryResult {
        self.builder.execute(&self.graph)
    }
}
