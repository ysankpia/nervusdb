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

/// 链式图查询构造器（纯磁盘游标执行，零全图驻留）
pub struct QueryBuilder<'a> {
    graph: &'a DiskGraph,
    pattern: Option<(String, String, String)>,
    traverse_config: Option<(u64, String, Direction, usize)>,
    prop_filters: Vec<(String, Box<dyn Fn(&Value) -> bool>)>,
    src_filters: Vec<Box<dyn Fn(&Node) -> bool>>,
    dst_filters: Vec<Box<dyn Fn(&Node) -> bool>>,
    edge_filters: Vec<Box<dyn Fn(&Edge) -> bool>>,
    limit: Option<usize>,
}

impl<'a> QueryBuilder<'a> {
    pub fn new(graph: &'a DiskGraph) -> Self {
        Self {
            graph,
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

    pub fn execute(self) -> QueryResult {
        if let Some((start_node_id, ref edge_type, direction, hops)) = self.traverse_config {
            self.execute_traverse(start_node_id, edge_type, direction, hops)
        } else if let Some((ref src_label, ref edge_type, ref dst_label)) = self.pattern {
            self.execute_pattern(src_label, edge_type, dst_label)
        } else {
            self.execute_pattern("", "", "")
        }
    }

    fn execute_pattern(&self, src_label: &str, edge_type: &str, dst_label: &str) -> QueryResult {
        let mut matched_paths = Vec::new();

        let match_any_src = src_label.is_empty() || src_label == "*";
        let match_any_edge = edge_type.is_empty() || edge_type == "*";
        let match_any_dst = dst_label.is_empty() || dst_label == "*";

        let node_ids = self.graph.all_node_ids().unwrap_or_default();
        let mut node_cache: std::collections::HashMap<u64, Node> = std::collections::HashMap::new();

        for node_id in node_ids {
            // 先通过定长 32 字节 NodeRecord 快速过滤，无需调入溢出页
            let node_record = match self.graph.read_node_record(node_id).ok().flatten() {
                Some(r) => r,
                None => continue,
            };

            if node_record.first_outgoing_edge_id == 0 {
                continue;
            }

            if !match_any_src {
                if let Some(lbl) = self.graph.dict.resolve(node_record.label_id) {
                    if lbl != src_label {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            let src_node = if let Some(n) = node_cache.get(&node_id) {
                n.clone()
            } else if let Ok(Some(n)) = self.graph.get_node(node_id) {
                node_cache.insert(node_id, n.clone());
                n
            } else {
                continue;
            };

            if !self.src_filters.iter().all(|f| f(&src_node)) {
                continue;
            }

            // 沿磁盘指针遍历出边
            let edges = self.graph.outgoing_edges(node_id).unwrap_or_default();
            for edge in edges {
                if !match_any_edge && edge.edge_type != edge_type {
                    continue;
                }

                if !self.edge_filters.iter().all(|f| f(&edge)) {
                    continue;
                }

                let dst_node = if let Some(n) = node_cache.get(&edge.dst_id) {
                    n.clone()
                } else if let Ok(Some(n)) = self.graph.get_node(edge.dst_id) {
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
        start_id: u64,
        edge_type: &str,
        direction: Direction,
        hops: usize,
    ) -> QueryResult {
        let start_node = match self.graph.get_node(start_id).ok().flatten() {
            Some(n) => n,
            None => return QueryResult::default(),
        };

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
                Direction::Outgoing => self.graph.outgoing_edges(current_id).unwrap_or_default(),
                Direction::Incoming => self.graph.incoming_edges(current_id).unwrap_or_default(),
                Direction::Both => {
                    let mut both = self.graph.outgoing_edges(current_id).unwrap_or_default();
                    both.extend(self.graph.incoming_edges(current_id).unwrap_or_default());
                    both
                }
            };

            for edge in candidate_edges {
                if !match_any_edge && edge.edge_type != edge_type {
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

                if let Ok(Some(next_node)) = self.graph.get_node(next_node_id) {
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

/// 拥有图句柄的独立链式查询构建器（纯磁盘游标运行）
pub struct GraphQuery {
    graph: DiskGraph,
    pattern: Option<(String, String, String)>,
    traverse_config: Option<(u64, String, Direction, usize)>,
    prop_filters: Vec<(String, Box<dyn Fn(&Value) -> bool>)>,
    src_filters: Vec<Box<dyn Fn(&Node) -> bool>>,
    dst_filters: Vec<Box<dyn Fn(&Node) -> bool>>,
    edge_filters: Vec<Box<dyn Fn(&Edge) -> bool>>,
    limit: Option<usize>,
}

impl GraphQuery {
    pub fn new(graph: DiskGraph) -> Self {
        Self {
            graph,
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

    pub fn execute(self) -> QueryResult {
        let mut builder = QueryBuilder::new(&self.graph);
        if let Some((src, edge, dst)) = self.pattern {
            builder = builder.match_pattern(&src, &edge, &dst);
        }
        if let Some((start_id, edge_type, dir, hops)) = self.traverse_config {
            builder = builder.traverse(start_id, &edge_type, dir, hops);
        }
        for (key, pred) in self.prop_filters {
            builder = builder.filter_prop(&key, pred);
        }
        for f in self.src_filters {
            builder = builder.filter_src(f);
        }
        for f in self.dst_filters {
            builder = builder.filter_dst(f);
        }
        for f in self.edge_filters {
            builder = builder.filter_edge(f);
        }
        if let Some(l) = self.limit {
            builder = builder.limit(l);
        }
        builder.execute()
    }
}
