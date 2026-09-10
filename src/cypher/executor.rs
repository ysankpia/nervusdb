use crate::cypher::ast::{
    BinaryOperator, CypherStatement, DeleteClause, ExecuteResult, Expr, NodePattern, PathPattern,
    ReturnItem,
};
use crate::disk_graph::DiskGraph;
use crate::graph::{GraphError, Value};
use crate::index::IndexManager;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub values: Vec<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CypherResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
    pub stats: ExecuteResult,
}

impl CypherResultSet {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

/// 只读 Cypher 执行器（脱离排他写锁，支持全并发只读）
pub struct CypherReadOnlyExecutor<'a> {
    graph: &'a DiskGraph,
    index_mgr: &'a IndexManager,
}

impl<'a> CypherReadOnlyExecutor<'a> {
    pub fn new(graph: &'a DiskGraph, index_mgr: &'a IndexManager) -> Self {
        Self { graph, index_mgr }
    }

    /// 执行只读 MATCH 查询
    pub fn execute_match(
        &self,
        pattern: PathPattern,
        where_clause: Option<Expr>,
        return_clause: Option<Vec<ReturnItem>>,
        limit: Option<usize>,
    ) -> Result<CypherResultSet, GraphError> {
        let matched_contexts = self.find_matches(&pattern, &where_clause)?;
        self.project_results(&pattern, matched_contexts, return_clause, limit)
    }

    /// 查找所有匹配上下文
    pub fn find_matches(
        &self,
        pattern: &PathPattern,
        where_clause: &Option<Expr>,
    ) -> Result<Vec<HashMap<String, u64>>, GraphError> {
        if pattern.nodes.is_empty() {
            return Ok(Vec::new());
        }

        let start_pat = &pattern.nodes[0];
        let candidate_start_nodes = self.find_initial_candidates(start_pat, where_clause);

        let mut matched_contexts = Vec::new();
        for start_id in candidate_start_nodes {
            if !self.node_matches_pattern(start_id, start_pat) {
                continue;
            }
            let mut ctx = HashMap::new();
            if let Some(ref var) = start_pat.variable {
                ctx.insert(var.clone(), start_id);
            }
            self.match_path_step(
                pattern,
                0,
                start_id,
                &ctx,
                where_clause,
                &mut matched_contexts,
            )?;
        }

        Ok(matched_contexts)
    }

    /// 结果投影
    pub fn project_results(
        &self,
        pattern: &PathPattern,
        matched_contexts: Vec<HashMap<String, u64>>,
        return_clause: Option<Vec<ReturnItem>>,
        limit: Option<usize>,
    ) -> Result<CypherResultSet, GraphError> {
        let return_items = return_clause.unwrap_or_else(|| {
            let mut items = Vec::new();
            for n in &pattern.nodes {
                if let Some(ref v) = n.variable {
                    items.push(ReturnItem::Variable {
                        var: v.clone(),
                        alias: None,
                    });
                }
            }
            items
        });

        let mut columns = Vec::new();
        let mut all_mode = false;
        for item in &return_items {
            match item {
                ReturnItem::All => {
                    all_mode = true;
                    if let Some(first_ctx) = matched_contexts.first() {
                        let mut keys: Vec<_> = first_ctx.keys().cloned().collect();
                        keys.sort();
                        columns.extend(keys);
                    } else {
                        columns.push("*".to_string());
                    }
                }
                ReturnItem::Variable { var, alias } => {
                    columns.push(alias.clone().unwrap_or_else(|| var.clone()));
                }
                ReturnItem::Property { var, prop, alias } => {
                    columns.push(alias.clone().unwrap_or_else(|| format!("{}.{}", var, prop)));
                }
            }
        }

        let edge_vars: HashSet<String> = pattern
            .edges
            .iter()
            .filter_map(|e| e.variable.clone())
            .collect();

        let mut rows = Vec::new();
        for ctx in &matched_contexts {
            let mut row_values = Vec::new();
            if all_mode {
                let mut keys: Vec<_> = ctx.keys().cloned().collect();
                keys.sort();
                for var in keys {
                    let nid = ctx[&var];
                    if edge_vars.contains(&var) {
                        if let Ok(Some(edge)) = self.graph.get_edge(nid) {
                            let json = serde_json::to_string(&edge.properties)
                                .unwrap_or_else(|_| "{}".to_string());
                            row_values.push(Value::from(json));
                        } else {
                            row_values.push(Value::from("null"));
                        }
                    } else if let Ok(Some(node)) = self.graph.get_node(nid) {
                        let json = serde_json::to_string(&node.properties)
                            .unwrap_or_else(|_| "{}".to_string());
                        row_values.push(Value::from(json));
                    } else {
                        row_values.push(Value::from("null"));
                    }
                }
            } else {
                for item in &return_items {
                    match item {
                        ReturnItem::All => {}
                        ReturnItem::Variable { var, .. } => {
                            if let Some(&nid) = ctx.get(var) {
                                if edge_vars.contains(var) {
                                    if let Ok(Some(edge)) = self.graph.get_edge(nid) {
                                        let json = serde_json::to_string(&edge.properties)
                                            .unwrap_or_else(|_| "{}".to_string());
                                        row_values.push(Value::from(json));
                                    } else {
                                        row_values.push(Value::from("null"));
                                    }
                                } else if let Ok(Some(node)) = self.graph.get_node(nid) {
                                    let json = serde_json::to_string(&node.properties)
                                        .unwrap_or_else(|_| "{}".to_string());
                                    row_values.push(Value::from(json));
                                } else {
                                    row_values.push(Value::from("null"));
                                }
                            } else {
                                row_values.push(Value::from("null"));
                            }
                        }
                        ReturnItem::Property { var, prop, .. } => {
                            if let Some(&nid) = ctx.get(var) {
                                if edge_vars.contains(var) {
                                    if let Ok(Some(edge)) = self.graph.get_edge(nid) {
                                        if let Some(val) = edge.get_prop(prop) {
                                            row_values.push(val.clone());
                                        } else if prop == "weight" {
                                            row_values.push(Value::from(edge.weight));
                                        } else {
                                            row_values.push(Value::from("null"));
                                        }
                                    } else {
                                        row_values.push(Value::from("null"));
                                    }
                                } else if let Ok(Some(node)) = self.graph.get_node(nid) {
                                    if let Some(val) = node.get_prop(prop) {
                                        row_values.push(val.clone());
                                    } else {
                                        row_values.push(Value::from("null"));
                                    }
                                } else {
                                    row_values.push(Value::from("null"));
                                }
                            } else {
                                row_values.push(Value::from("null"));
                            }
                        }
                    }
                }
            }
            rows.push(Row { values: row_values });

            if let Some(max_l) = limit {
                if rows.len() >= max_l {
                    break;
                }
            }
        }

        let count = rows.len();
        Ok(CypherResultSet {
            columns,
            rows,
            stats: ExecuteResult {
                nodes_created: 0,
                edges_created: 0,
                nodes_deleted: 0,
                edges_deleted: 0,
                properties_set: 0,
                message: format!("Query returned {} rows.", count),
            },
        })
    }

    /// 起始候选集检索：智能命中属性索引或标签索引，回退走流式磁盘页扫描
    fn find_initial_candidates(
        &self,
        node_pat: &NodePattern,
        where_clause: &Option<Expr>,
    ) -> Vec<u64> {
        let label_opt = node_pat.label.as_deref();

        if let Some(lbl) = label_opt {
            if self.index_mgr.is_label_complete(lbl) {
                for (key, val) in &node_pat.properties {
                    if let Some(set) = self.index_mgr.find_by_property_exact(lbl, key, val) {
                        return set.iter().copied().collect();
                    }
                }

                if let (Some(ref w_expr), Some(ref v_name)) = (where_clause, &node_pat.variable) {
                    if let Some(candidates) = self.try_find_from_where_expr(lbl, w_expr, v_name) {
                        return candidates;
                    }
                }

                if let Some(set) = self.index_mgr.find_by_label(lbl) {
                    return set.iter().copied().collect();
                }
            }
        }

        self.graph.all_node_ids().unwrap_or_default()
    }

    fn try_find_from_where_expr(
        &self,
        label: &str,
        expr: &Expr,
        expected_var: &str,
    ) -> Option<Vec<u64>> {
        match expr {
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Eq,
                right,
            } => {
                if let (Expr::PropertyAccess { var, prop }, Expr::Literal(val)) =
                    (&**left, &**right)
                {
                    if var == expected_var {
                        if let Some(set) = self.index_mgr.find_by_property_exact(label, prop, val) {
                            return Some(set.iter().copied().collect());
                        }
                    }
                }
                None
            }
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Gt,
                right,
            } => {
                if let (Expr::PropertyAccess { var, prop }, Expr::Literal(val)) =
                    (&**left, &**right)
                {
                    if var == expected_var {
                        let set = self
                            .index_mgr
                            .find_by_property_greater_than(label, prop, val, false);
                        return Some(set.into_iter().collect());
                    }
                }
                None
            }
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Gte,
                right,
            } => {
                if let (Expr::PropertyAccess { var, prop }, Expr::Literal(val)) =
                    (&**left, &**right)
                {
                    if var == expected_var {
                        let set = self
                            .index_mgr
                            .find_by_property_greater_than(label, prop, val, true);
                        return Some(set.into_iter().collect());
                    }
                }
                None
            }
            Expr::BinaryOp {
                left,
                op: BinaryOperator::And,
                right,
            } => self
                .try_find_from_where_expr(label, left, expected_var)
                .or_else(|| self.try_find_from_where_expr(label, right, expected_var)),
            _ => None,
        }
    }

    fn match_path_step(
        &self,
        pattern: &PathPattern,
        step: usize,
        current_node_id: u64,
        ctx: &HashMap<String, u64>,
        where_clause: &Option<Expr>,
        results: &mut Vec<HashMap<String, u64>>,
    ) -> Result<(), GraphError> {
        if step >= pattern.edges.len() {
            if let Some(ref w) = where_clause {
                if !self.eval_expr(w, ctx) {
                    return Ok(());
                }
            }
            results.push(ctx.clone());
            return Ok(());
        }

        let edge_pat = &pattern.edges[step];
        let next_node_pat = &pattern.nodes[step + 1];

        if let Some((min_hops, max_hops)) = edge_pat.hops {
            let mut queue = std::collections::VecDeque::new();
            queue.push_back((current_node_id, 0, HashSet::new()));

            while let Some((nid, h, visited_edges)) = queue.pop_front() {
                if h >= min_hops && h <= max_hops && self.node_matches_pattern(nid, next_node_pat) {
                    let mut next_ctx = ctx.clone();
                    if let Some(ref var) = next_node_pat.variable {
                        next_ctx.insert(var.clone(), nid);
                    }
                    self.match_path_step(pattern, step + 1, nid, &next_ctx, where_clause, results)?;
                }

                if h < max_hops {
                    let edges = match edge_pat.direction {
                        crate::graph::Direction::Outgoing => self.graph.outgoing_edges(nid)?,
                        crate::graph::Direction::Incoming => self.graph.incoming_edges(nid)?,
                        crate::graph::Direction::Both => {
                            let mut e = self.graph.outgoing_edges(nid)?;
                            e.extend(self.graph.incoming_edges(nid)?);
                            e
                        }
                    };

                    for edge in edges {
                        if visited_edges.contains(&edge.id) {
                            continue;
                        }

                        if let Some(ref expected_type) = edge_pat.rel_type {
                            if &edge.edge_type != expected_type {
                                continue;
                            }
                        }

                        let mut props_match = true;
                        for (k, expected_v) in &edge_pat.properties {
                            if edge.get_prop(k) != Some(expected_v) {
                                props_match = false;
                                break;
                            }
                        }
                        if !props_match {
                            continue;
                        }

                        let next_nid = if edge.src_id == nid {
                            edge.dst_id
                        } else {
                            edge.src_id
                        };

                        let mut next_visited = visited_edges.clone();
                        next_visited.insert(edge.id);
                        queue.push_back((next_nid, h + 1, next_visited));
                    }
                }
            }
            return Ok(());
        }

        let edges = match edge_pat.direction {
            crate::graph::Direction::Outgoing => self.graph.outgoing_edges(current_node_id)?,
            crate::graph::Direction::Incoming => self.graph.incoming_edges(current_node_id)?,
            crate::graph::Direction::Both => {
                let mut e = self.graph.outgoing_edges(current_node_id)?;
                e.extend(self.graph.incoming_edges(current_node_id)?);
                e
            }
        };

        for edge in edges {
            if let Some(ref expected_type) = edge_pat.rel_type {
                if &edge.edge_type != expected_type {
                    continue;
                }
            }

            for (k, expected_v) in &edge_pat.properties {
                if edge.get_prop(k) != Some(expected_v) {
                    continue;
                }
            }

            let next_nid = if edge.src_id == current_node_id {
                edge.dst_id
            } else {
                edge.src_id
            };

            if self.node_matches_pattern(next_nid, next_node_pat) {
                let mut next_ctx = ctx.clone();
                if let Some(ref var) = next_node_pat.variable {
                    next_ctx.insert(var.clone(), next_nid);
                }
                if let Some(ref edge_var) = edge_pat.variable {
                    next_ctx.insert(edge_var.clone(), edge.id);
                }
                self.match_path_step(
                    pattern,
                    step + 1,
                    next_nid,
                    &next_ctx,
                    where_clause,
                    results,
                )?;
            }
        }

        Ok(())
    }

    fn node_matches_pattern(&self, node_id: u64, pattern: &NodePattern) -> bool {
        let node = match self.graph.get_node(node_id) {
            Ok(Some(n)) => n,
            _ => return false,
        };

        if let Some(ref expected_label) = pattern.label {
            if !node.has_label(expected_label) {
                return false;
            }
        }

        for (k, expected_v) in &pattern.properties {
            if node.get_prop(k) != Some(expected_v) {
                return false;
            }
        }

        true
    }

    fn eval_expr(&self, expr: &Expr, ctx: &HashMap<String, u64>) -> bool {
        match expr {
            Expr::Literal(Value::Bool(b)) => *b,
            Expr::BinaryOp { left, op, right } => match op {
                BinaryOperator::And => self.eval_expr(left, ctx) && self.eval_expr(right, ctx),
                BinaryOperator::Or => self.eval_expr(left, ctx) || self.eval_expr(right, ctx),
                _ => {
                    let l_val = self.eval_val(left, ctx);
                    let r_val = self.eval_val(right, ctx);
                    match (l_val, r_val) {
                        (Some(l), Some(r)) => match op {
                            BinaryOperator::Eq => l == r,
                            BinaryOperator::Neq => l != r,
                            BinaryOperator::Lt => l < r,
                            BinaryOperator::Lte => l <= r,
                            BinaryOperator::Gt => l > r,
                            BinaryOperator::Gte => l >= r,
                            _ => false,
                        },
                        _ => false,
                    }
                }
            },
            _ => true,
        }
    }

    fn eval_val(&self, expr: &Expr, ctx: &HashMap<String, u64>) -> Option<Value> {
        match expr {
            Expr::Literal(v) => Some(v.clone()),
            Expr::PropertyAccess { var, prop } => {
                if let Some(&id) = ctx.get(var) {
                    if let Ok(Some(node)) = self.graph.get_node(id) {
                        return node.get_prop(prop).cloned();
                    } else if let Ok(Some(edge)) = self.graph.get_edge(id) {
                        return edge.get_prop(prop).cloned().or_else(|| {
                            if prop == "weight" {
                                Some(Value::from(edge.weight))
                            } else {
                                None
                            }
                        });
                    }
                }
                None
            }
            Expr::Variable(var) => {
                if let Some(&node_id) = ctx.get(var) {
                    return Some(Value::from(node_id as i64));
                }
                None
            }
            _ => None,
        }
    }
}

/// 完整 Cypher 执行器（支持 CREATE 与 DELETE 等写操作）
pub struct CypherExecutor<'a> {
    graph: &'a mut DiskGraph,
    index_mgr: &'a mut IndexManager,
}

impl<'a> CypherExecutor<'a> {
    pub fn new(graph: &'a mut DiskGraph, index_mgr: &'a mut IndexManager) -> Self {
        Self { graph, index_mgr }
    }

    pub fn execute_statement(
        &mut self,
        stmt: CypherStatement,
    ) -> Result<CypherResultSet, GraphError> {
        match stmt {
            CypherStatement::Create { pattern } => self.execute_create(pattern),
            CypherStatement::Match {
                pattern,
                where_clause,
                return_clause,
                delete_clause,
                limit,
            } => {
                if let Some(start_pat) = pattern.nodes.first() {
                    if let Some(ref lbl) = start_pat.label {
                        self.index_mgr.ensure_label_index(self.graph, lbl);
                    }
                }
                if let Some(del) = delete_clause {
                    self.execute_match_with_delete(pattern, where_clause, del)
                } else {
                    let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
                    ro.execute_match(pattern, where_clause, return_clause, limit)
                }
            }
        }
    }

    /// 执行 CREATE 语句（纯磁盘写入，维护索引）
    fn execute_create(&mut self, pattern: PathPattern) -> Result<CypherResultSet, GraphError> {
        let mut var_bindings: HashMap<String, u64> = HashMap::new();
        let mut nodes_created = 0;
        let mut edges_created = 0;

        let mut node_ids = Vec::new();

        for node_pat in &pattern.nodes {
            let mut labels = HashSet::new();
            if let Some(ref l) = node_pat.label {
                labels.insert(l.clone());
            }

            let node_id = self
                .graph
                .add_node(labels.clone(), node_pat.properties.clone())?;
            nodes_created += 1;
            node_ids.push(node_id);

            for l in &labels {
                self.index_mgr.insert_label(l, node_id);
                self.graph.index_catalog.labels.insert(l.clone());
                for (k, v) in &node_pat.properties {
                    self.index_mgr.insert_property(l, k, v.clone(), node_id);
                    self.graph
                        .index_catalog
                        .properties
                        .insert((l.clone(), k.clone()));
                }
            }

            if let Some(ref var) = node_pat.variable {
                var_bindings.insert(var.clone(), node_id);
            }
        }

        for (i, edge_pat) in pattern.edges.iter().enumerate() {
            let src_id = node_ids[i];
            let dst_id = node_ids[i + 1];

            let rel_type = edge_pat
                .rel_type
                .clone()
                .unwrap_or_else(|| "RELATED".to_string());
            let weight = edge_pat.weight.unwrap_or(1.0);

            self.graph.add_edge(
                src_id,
                dst_id,
                &rel_type,
                edge_pat.properties.clone(),
                weight,
            )?;
            edges_created += 1;
        }

        let stats = ExecuteResult {
            nodes_created,
            edges_created,
            nodes_deleted: 0,
            edges_deleted: 0,
            properties_set: 0,
            message: format!(
                "Created {} nodes, {} relationships.",
                nodes_created, edges_created
            ),
        };

        Ok(CypherResultSet {
            columns: vec!["Status".to_string()],
            rows: vec![Row {
                values: vec![Value::from(stats.message.clone())],
            }],
            stats,
        })
    }

    /// 执行带 DELETE / DETACH DELETE 的 MATCH 语句
    fn execute_match_with_delete(
        &mut self,
        pattern: PathPattern,
        where_clause: Option<Expr>,
        del: DeleteClause,
    ) -> Result<CypherResultSet, GraphError> {
        let matched_contexts = {
            let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
            ro.find_matches(&pattern, &where_clause)?
        };

        let node_vars: HashSet<String> = pattern
            .nodes
            .iter()
            .filter_map(|n| n.variable.clone())
            .collect();
        let edge_vars: HashSet<String> = pattern
            .edges
            .iter()
            .filter_map(|e| e.variable.clone())
            .collect();

        let mut nodes_deleted = 0;
        let mut edges_deleted = 0;
        let mut deleted_node_ids = HashSet::new();
        let mut deleted_edge_ids = HashSet::new();

        for ctx in &matched_contexts {
            for target_var in &del.targets {
                if let Some(&entity_id) = ctx.get(target_var) {
                    if node_vars.contains(target_var) {
                        if deleted_node_ids.insert(entity_id) {
                            if let Ok(n) = self.graph.remove_node(entity_id) {
                                nodes_deleted += 1;
                                edges_deleted += n.outgoing.len() + n.incoming.len();
                                let labels_bt: BTreeSet<String> = n.labels.into_iter().collect();
                                self.index_mgr.remove_node_all_indices(
                                    entity_id,
                                    &labels_bt,
                                    &n.properties,
                                );
                            }
                        }
                    } else if edge_vars.contains(target_var) {
                        if deleted_edge_ids.insert(entity_id) {
                            if self.graph.remove_edge(entity_id).is_ok() {
                                edges_deleted += 1;
                            }
                        }
                    }
                }
            }
        }

        let stats = ExecuteResult {
            nodes_created: 0,
            edges_created: 0,
            nodes_deleted,
            edges_deleted,
            properties_set: 0,
            message: format!("Deleted {} nodes, {} edges.", nodes_deleted, edges_deleted),
        };

        Ok(CypherResultSet {
            columns: vec!["Status".to_string()],
            rows: vec![Row {
                values: vec![Value::from(stats.message.clone())],
            }],
            stats,
        })
    }
}

/// 快捷执行 Cypher 字符串查询入口（只读模式）
pub fn execute_cypher_read(
    query_str: &str,
    graph: &DiskGraph,
    index_mgr: &IndexManager,
) -> Result<CypherResultSet, GraphError> {
    let lexer = crate::cypher::lexer::Lexer::new(query_str);
    let tokens = lexer.tokenize()?;
    let mut parser = crate::cypher::parser::Parser::new(tokens);
    let statement = parser.parse()?;
    match statement {
        CypherStatement::Match {
            pattern,
            where_clause,
            return_clause,
            delete_clause: None,
            limit,
        } => {
            let executor = CypherReadOnlyExecutor::new(graph, index_mgr);
            executor.execute_match(pattern, where_clause, return_clause, limit)
        }
        _ => Err(GraphError::General(
            "Read-only executor cannot execute mutating Cypher statement".into(),
        )),
    }
}

/// 快捷执行 Cypher 字符串写查询入口
pub fn execute_cypher(
    query_str: &str,
    graph: &mut DiskGraph,
    index_mgr: &mut IndexManager,
) -> Result<CypherResultSet, GraphError> {
    let lexer = crate::cypher::lexer::Lexer::new(query_str);
    let tokens = lexer.tokenize()?;
    let mut parser = crate::cypher::parser::Parser::new(tokens);
    let statement = parser.parse()?;
    let mut executor = CypherExecutor::new(graph, index_mgr);
    executor.execute_statement(statement)
}

/// 执行 Cypher 只读查询 (MATCH ... RETURN)
pub fn execute_query(
    query_str: &str,
    graph: &DiskGraph,
    index_mgr: &IndexManager,
) -> Result<CypherResultSet, GraphError> {
    execute_cypher_read(query_str, graph, index_mgr)
}

/// 执行 Cypher 变更语句 (CREATE, SET, DELETE)
pub fn execute_mutate(
    query_str: &str,
    graph: &mut DiskGraph,
    index_mgr: &mut IndexManager,
) -> Result<CypherResultSet, GraphError> {
    execute_cypher(query_str, graph, index_mgr)
}
