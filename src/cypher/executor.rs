use crate::cypher::ast::{
    AggregateArg, AggregateFunc, BinaryOperator, CypherStatement, DeleteClause, ExecuteResult,
    Expr, NodePattern, OrderItem, PathPattern, ReturnItem, SetItem,
};
use crate::disk_graph::DiskGraph;
use crate::graph::{Direction, GraphError, Value};
use crate::index::IndexManager;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// 结果集行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub values: Vec<Value>,
}

/// Cypher 查询结果集
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

/// 变量绑定：明确区分节点与边，彻底杜绝「边 ID 与节点 ID 同号混淆」
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Node(u64),
    Edge(u64),
}

/// 单行匹配上下文：变量名 -> 实体绑定
pub type RowCtx = HashMap<String, Binding>;

/// 投影后的中间行（保留上下文以支持 ORDER BY 表达式求值）
struct ProjectedRow {
    ctx: RowCtx,
    values: Vec<Value>,
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

    /// 执行只读 MATCH 查询（完整管线：匹配 → WHERE → 投影/聚合 → ORDER BY → SKIP → LIMIT）
    pub fn execute_match(
        &self,
        patterns: Vec<PathPattern>,
        where_clause: Option<Expr>,
        return_clause: Option<Vec<ReturnItem>>,
        order_by: &[OrderItem],
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<CypherResultSet, GraphError> {
        let matched = self.find_matches(&patterns, &where_clause)?;
        self.project_results(&patterns, matched, return_clause, order_by, skip, limit)
    }

    /// 多模式匹配：逐模式求解后按共享变量做连接（笛卡尔积 + 一致性约束）
    pub fn find_matches(
        &self,
        patterns: &[PathPattern],
        where_clause: &Option<Expr>,
    ) -> Result<Vec<RowCtx>, GraphError> {
        if patterns.is_empty() {
            return Ok(Vec::new());
        }

        let mut combined: Vec<RowCtx> = vec![RowCtx::new()];

        for (idx, pattern) in patterns.iter().enumerate() {
            // 仅对首个模式启用索引加速（WHERE 中的候选集推导基于整体表达式）
            let pattern_rows = if idx == 0 {
                self.find_single_pattern_matches(pattern, where_clause)?
            } else {
                self.find_single_pattern_matches(pattern, &None)?
            };

            if pattern_rows.is_empty() {
                return Ok(Vec::new());
            }

            let mut joined: Vec<RowCtx> = Vec::new();
            for base in &combined {
                for candidate in &pattern_rows {
                    if let Some(merged) = merge_contexts(base, candidate) {
                        joined.push(merged);
                    }
                }
            }
            combined = joined;

            if combined.is_empty() {
                return Ok(Vec::new());
            }
        }

        if let Some(ref w) = where_clause {
            combined.retain(|ctx| self.eval_expr_truthy(w, ctx));
        }

        Ok(combined)
    }

    /// 求解单个路径模式的全部匹配上下文
    fn find_single_pattern_matches(
        &self,
        pattern: &PathPattern,
        where_clause: &Option<Expr>,
    ) -> Result<Vec<RowCtx>, GraphError> {
        if pattern.nodes.is_empty() {
            return Ok(Vec::new());
        }

        let start_pat = &pattern.nodes[0];
        let candidate_start_nodes = self.find_initial_candidates(start_pat, where_clause);

        let mut matched = Vec::new();
        for start_id in candidate_start_nodes {
            // 通过定长 NodeRecord 做快速过滤，避免无谓的溢出页调入
            if self.graph.read_node_record(start_id)?.is_none() {
                continue;
            }
            if !self.node_matches_pattern(start_id, start_pat) {
                continue;
            }

            let mut ctx = RowCtx::new();
            if let Some(ref var) = start_pat.variable {
                ctx.insert(var.clone(), Binding::Node(start_id));
            }

            self.match_path_step(pattern, 0, start_id, &ctx, &mut matched)?;
        }

        Ok(matched)
    }

    /// 结果投影：聚合分组 → ORDER BY → SKIP → LIMIT
    pub fn project_results(
        &self,
        patterns: &[PathPattern],
        matched: Vec<RowCtx>,
        return_clause: Option<Vec<ReturnItem>>,
        order_by: &[OrderItem],
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<CypherResultSet, GraphError> {
        let default_all = return_clause.is_none();
        let return_items = match return_clause {
            Some(items) if !items.is_empty() => items,
            _ => {
                let mut items = Vec::new();
                for p in patterns {
                    for n in &p.nodes {
                        if let Some(ref v) = n.variable {
                            if !items
                                .iter()
                                .any(|i| matches!(i, ReturnItem::Variable { var, .. } if var == v))
                            {
                                items.push(ReturnItem::Variable {
                                    var: v.clone(),
                                    alias: None,
                                });
                            }
                        }
                    }
                }
                items
            }
        };

        // `RETURN *` 展开为上下文中出现过的具体变量（按列名有序），与 sqlite 的 `SELECT *` 语义对齐
        let mut return_items = return_items;
        if return_items.iter().any(|i| matches!(i, ReturnItem::All)) {
            let mut all_vars: BTreeSet<String> = BTreeSet::new();
            for ctx in &matched {
                for var in ctx.keys() {
                    if !return_items
                        .iter()
                        .any(|i| matches!(i, ReturnItem::Variable { var: v, .. } if v == var))
                    {
                        all_vars.insert(var.clone());
                    }
                }
            }

            // 保持 `*` 所在位置，其余变量按名称有序追加
            let mut expanded: Vec<ReturnItem> = Vec::new();
            let mut taken: HashSet<String> = HashSet::new();
            for item in return_items {
                match item {
                    ReturnItem::All => {
                        for var in &all_vars {
                            if taken.insert(var.clone()) {
                                expanded.push(ReturnItem::Variable {
                                    var: var.clone(),
                                    alias: None,
                                });
                            }
                        }
                    }
                    other => {
                        if let ReturnItem::Variable { var, .. } = &other {
                            taken.insert(var.clone());
                        }
                        expanded.push(other);
                    }
                }
            }

            // 空结果集无法从上下文推导变量，退回字面 `*` 列
            if expanded.is_empty() {
                expanded.push(ReturnItem::All);
            }
            return_items = expanded;
        }

        let has_aggregate = return_items
            .iter()
            .any(|i| matches!(i, ReturnItem::Aggregate { .. }));

        let mut projected = if has_aggregate {
            self.project_aggregated(&return_items, &matched)?
        } else {
            self.project_plain(&return_items, default_all, &matched)?
        };

        // 列名解析
        let mut columns = Vec::new();
        for item in &return_items {
            columns.push(Self::column_name(item, &projected));
        }

        // ORDER BY（先尝试别名列，再回退上下文表达式求值）
        if !order_by.is_empty() {
            let mut sortable: Vec<(usize, Vec<Value>)> = Vec::with_capacity(projected.len());
            for (i, row) in projected.iter().enumerate() {
                let mut keys = Vec::with_capacity(order_by.len());
                for item in order_by {
                    keys.push(self.eval_order_key(item, &row.ctx, &columns, &row.values));
                }
                sortable.push((i, keys));
            }
            sortable.sort_by(|a, b| {
                for (idx, item) in order_by.iter().enumerate() {
                    let ord = a.1[idx].cmp(&b.1[idx]);
                    let ord = if item.desc { ord.reverse() } else { ord };
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
                a.0.cmp(&b.0)
            });
            let mut reordered = Vec::with_capacity(projected.len());
            let mut taken: Vec<Option<ProjectedRow>> = projected.into_iter().map(Some).collect();
            for (i, _) in sortable {
                if let Some(row) = taken[i].take() {
                    reordered.push(row);
                }
            }
            projected = reordered;
        }

        // SKIP / LIMIT 分页
        let start = skip.unwrap_or(0).min(projected.len());
        let mut end = projected.len();
        if let Some(max) = limit {
            end = start.saturating_add(max).min(projected.len());
        }
        let paged: Vec<ProjectedRow> = projected.drain(..).skip(start).take(end - start).collect();

        let count = paged.len();
        let rows: Vec<Row> = paged
            .into_iter()
            .map(|r| Row { values: r.values })
            .collect();

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

    fn column_name(item: &ReturnItem, projected: &[ProjectedRow]) -> String {
        let _ = projected;
        match item {
            ReturnItem::All => "*".to_string(),
            ReturnItem::Variable { var, alias } => alias.clone().unwrap_or_else(|| var.clone()),
            ReturnItem::Property { var, prop, alias } => {
                alias.clone().unwrap_or_else(|| format!("{}.{}", var, prop))
            }
            ReturnItem::Aggregate { func, arg, alias } => alias.clone().unwrap_or_else(|| {
                let arg_repr = match arg {
                    AggregateArg::Star => "*".to_string(),
                    AggregateArg::Variable(v) => v.clone(),
                    AggregateArg::Property { var, prop } => format!("{}.{}", var, prop),
                };
                format!("{}({})", func.as_str(), arg_repr)
            }),
        }
    }

    /// 非聚合投影（逐匹配行展开）
    fn project_plain(
        &self,
        return_items: &[ReturnItem],
        default_all: bool,
        matched: &[RowCtx],
    ) -> Result<Vec<ProjectedRow>, GraphError> {
        let mut rows = Vec::with_capacity(matched.len());

        for ctx in matched {
            let mut values = Vec::new();
            for item in return_items {
                match item {
                    ReturnItem::All => {
                        let mut keys: Vec<&String> = ctx.keys().collect();
                        keys.sort();
                        for var in keys {
                            values.push(self.render_binding(var, ctx[var])?);
                        }
                    }
                    ReturnItem::Variable { var, .. } => match ctx.get(var) {
                        Some(binding) => values.push(self.render_binding(var, *binding)?),
                        None => values.push(null_value()),
                    },
                    ReturnItem::Property { var, prop, .. } => {
                        values.push(self.render_property(var, prop, ctx)?)
                    }
                    ReturnItem::Aggregate { .. } => {}
                }
            }
            let _ = default_all;
            rows.push(ProjectedRow {
                ctx: ctx.clone(),
                values,
            });
        }

        Ok(rows)
    }

    /// 聚合投影（按非聚合投影项分组，组内计算聚合函数）
    fn project_aggregated(
        &self,
        return_items: &[ReturnItem],
        matched: &[RowCtx],
    ) -> Result<Vec<ProjectedRow>, GraphError> {
        let group_items: Vec<&ReturnItem> = return_items
            .iter()
            .filter(|i| !matches!(i, ReturnItem::Aggregate { .. }))
            .collect();

        let mut groups: BTreeMap<Vec<Value>, Vec<RowCtx>> = BTreeMap::new();

        if group_items.is_empty() {
            // 全局聚合：空结果集也必须产出一行（count=0 / sum=0 / 其余 null）
            let key = Vec::new();
            let entry = groups.entry(key).or_default();
            for ctx in matched {
                entry.push(ctx.clone());
            }
        } else {
            for ctx in matched {
                let mut key = Vec::with_capacity(group_items.len());
                for item in &group_items {
                    key.push(self.eval_item_value(item, ctx)?);
                }
                groups.entry(key).or_default().push(ctx.clone());
            }
        }

        let mut rows = Vec::new();
        for (_key, ctxs) in groups {
            let representative = ctxs.first().cloned().unwrap_or_default();
            let mut values = Vec::new();
            for item in return_items {
                match item {
                    ReturnItem::Aggregate { func, arg, .. } => {
                        values.push(self.eval_aggregate(*func, arg, &ctxs)?);
                    }
                    other => {
                        values.push(self.eval_item_value(other, &representative)?);
                    }
                }
            }
            rows.push(ProjectedRow {
                ctx: representative,
                values,
            });
        }

        Ok(rows)
    }

    fn eval_aggregate(
        &self,
        func: AggregateFunc,
        arg: &AggregateArg,
        ctxs: &[RowCtx],
    ) -> Result<Value, GraphError> {
        let mut collected: Vec<Value> = Vec::new();
        let mut non_null_count: i64 = 0;

        for ctx in ctxs {
            match arg {
                AggregateArg::Star => {
                    non_null_count += 1;
                }
                AggregateArg::Variable(var) => {
                    if ctx.contains_key(var) {
                        non_null_count += 1;
                        collected.push(Value::Int(1));
                    }
                }
                AggregateArg::Property { var, prop } => {
                    let val = self.render_property(var, prop, ctx)?;
                    if !is_null(&val) {
                        non_null_count += 1;
                        collected.push(val);
                    }
                }
            }
        }

        let result = match func {
            AggregateFunc::Count => Value::Int(non_null_count),
            AggregateFunc::Sum => {
                if collected.is_empty() {
                    Value::Int(0)
                } else {
                    let all_int = collected.iter().all(|v| matches!(v, Value::Int(_)));
                    let sum_f: f64 = collected.iter().filter_map(|v| v.as_f64()).sum();
                    if all_int {
                        Value::Int(sum_f as i64)
                    } else {
                        Value::Float(sum_f)
                    }
                }
            }
            AggregateFunc::Avg => {
                if collected.is_empty() {
                    null_value()
                } else {
                    let sum_f: f64 = collected.iter().filter_map(|v| v.as_f64()).sum();
                    Value::Float(sum_f / collected.len() as f64)
                }
            }
            AggregateFunc::Min => collected.into_iter().min().unwrap_or_else(null_value),
            AggregateFunc::Max => collected.into_iter().max().unwrap_or_else(null_value),
        };

        Ok(result)
    }

    fn eval_item_value(&self, item: &ReturnItem, ctx: &RowCtx) -> Result<Value, GraphError> {
        match item {
            ReturnItem::Variable { var, .. } => match ctx.get(var) {
                Some(binding) => self.render_binding(var, *binding),
                None => Ok(null_value()),
            },
            ReturnItem::Property { var, prop, .. } => self.render_property(var, prop, ctx),
            ReturnItem::All => Ok(null_value()),
            ReturnItem::Aggregate { .. } => Ok(null_value()),
        }
    }

    fn render_binding(&self, var: &str, binding: Binding) -> Result<Value, GraphError> {
        let _ = var;
        match binding {
            Binding::Node(id) => {
                if let Some(node) = self.graph.get_node(id)? {
                    let json = serde_json::to_string(&node.properties).unwrap_or_default();
                    Ok(Value::from(json))
                } else {
                    Ok(Value::from("{}"))
                }
            }
            Binding::Edge(id) => {
                if let Some(edge) = self.graph.get_edge(id)? {
                    let json = serde_json::to_string(&edge.properties).unwrap_or_default();
                    Ok(Value::from(json))
                } else {
                    Ok(Value::from("{}"))
                }
            }
        }
    }

    fn render_property(&self, var: &str, prop: &str, ctx: &RowCtx) -> Result<Value, GraphError> {
        match ctx.get(var) {
            Some(Binding::Edge(id)) => {
                if let Some(edge) = self.graph.get_edge(*id)? {
                    if let Some(val) = edge.get_prop(prop) {
                        Ok(val.clone())
                    } else if prop == "weight" {
                        Ok(Value::from(edge.weight))
                    } else {
                        Ok(null_value())
                    }
                } else {
                    Ok(null_value())
                }
            }
            Some(Binding::Node(id)) => {
                if let Some(node) = self.graph.get_node(*id)? {
                    if let Some(val) = node.get_prop(prop) {
                        Ok(val.clone())
                    } else if prop == "id" {
                        Ok(Value::Int(*id as i64))
                    } else {
                        Ok(null_value())
                    }
                } else {
                    Ok(null_value())
                }
            }
            None => Ok(null_value()),
        }
    }

    fn eval_order_key(
        &self,
        item: &OrderItem,
        ctx: &RowCtx,
        columns: &[String],
        values: &[Value],
    ) -> Value {
        // 优先按投影别名解析（如 ORDER BY total DESC）
        match &item.expr {
            Expr::Variable(name) => {
                if let Some(idx) = columns.iter().position(|c| c == name) {
                    if let Some(v) = values.get(idx) {
                        return v.clone();
                    }
                }
                self.eval_expr_value(&item.expr, ctx)
                    .unwrap_or_else(null_value)
            }
            _ => self
                .eval_expr_value(&item.expr, ctx)
                .unwrap_or_else(null_value),
        }
    }

    /// 起始候选集检索：智能命中属性索引或标签索引，回退走流式磁盘页扫描
    fn find_initial_candidates(
        &self,
        node_pat: &NodePattern,
        where_clause: &Option<Expr>,
    ) -> Vec<u64> {
        if let Some(lbl) = node_pat.labels.first() {
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

    /// 沿路径模式逐跳推进匹配（含变长多跳 BFS 与环检测守卫）
    fn match_path_step(
        &self,
        pattern: &PathPattern,
        step: usize,
        current_node_id: u64,
        ctx: &RowCtx,
        results: &mut Vec<RowCtx>,
    ) -> Result<(), GraphError> {
        if step >= pattern.edges.len() {
            results.push(ctx.clone());
            return Ok(());
        }

        let edge_pat = &pattern.edges[step];
        let next_node_pat = &pattern.nodes[step + 1];

        // 变长多跳：BFS 展开并以 visited_edges 作为环检测守卫
        if let Some((min_hops, max_hops)) = edge_pat.hops {
            // (当前节点, 已走跳数, 本路径已用边集合, 最后一跳的边 ID)
            let mut queue: std::collections::VecDeque<(u64, usize, HashSet<u64>, u64)> =
                std::collections::VecDeque::new();
            queue.push_back((current_node_id, 0, HashSet::new(), 0));

            while let Some((nid, h, visited_edges, last_edge)) = queue.pop_front() {
                if h >= min_hops && h <= max_hops && self.node_matches_pattern(nid, next_node_pat) {
                    let mut next_ctx = ctx.clone();
                    if let Some(ref var) = next_node_pat.variable {
                        // 同名变量是连接约束而非覆盖（如自环 `(a)-[:R*1..1]->(a)`）
                        if !bind_or_reject(&mut next_ctx, var, Binding::Node(nid)) {
                            continue;
                        }
                    }
                    if let Some(ref edge_var) = edge_pat.variable {
                        if last_edge != 0 {
                            next_ctx.insert(edge_var.clone(), Binding::Edge(last_edge));
                        }
                    }
                    self.match_path_step(pattern, step + 1, nid, &next_ctx, results)?;
                }

                if h >= max_hops {
                    continue;
                }

                for edge in self.candidate_edges(nid, edge_pat.direction)? {
                    if visited_edges.contains(&edge.id) {
                        continue;
                    }
                    if !self.edge_matches_pattern(&edge, edge_pat) {
                        continue;
                    }

                    let next_nid = neighbor_of(&edge, nid);
                    let mut next_visited = visited_edges.clone();
                    next_visited.insert(edge.id);
                    queue.push_back((next_nid, h + 1, next_visited, edge.id));
                }
            }
            return Ok(());
        }

        for edge in self.candidate_edges(current_node_id, edge_pat.direction)? {
            if !self.edge_matches_pattern(&edge, edge_pat) {
                continue;
            }

            let next_nid = neighbor_of(&edge, current_node_id);

            if self.node_matches_pattern(next_nid, next_node_pat) {
                let mut next_ctx = ctx.clone();
                if let Some(ref var) = next_node_pat.variable {
                    // 同名变量是连接约束而非覆盖（如自环 `(a)-[:R]->(a)`）
                    if !bind_or_reject(&mut next_ctx, var, Binding::Node(next_nid)) {
                        continue;
                    }
                }
                if let Some(ref edge_var) = edge_pat.variable {
                    next_ctx.insert(edge_var.clone(), Binding::Edge(edge.id));
                }
                self.match_path_step(pattern, step + 1, next_nid, &next_ctx, results)?;
            }
        }

        Ok(())
    }

    fn candidate_edges(
        &self,
        node_id: u64,
        direction: Direction,
    ) -> Result<Vec<crate::graph::Edge>, GraphError> {
        match direction {
            Direction::Outgoing => self.graph.outgoing_edges(node_id),
            Direction::Incoming => self.graph.incoming_edges(node_id),
            Direction::Both => {
                let mut both = self.graph.outgoing_edges(node_id)?;
                both.extend(self.graph.incoming_edges(node_id)?);
                Ok(both)
            }
        }
    }

    fn edge_matches_pattern(
        &self,
        edge: &crate::graph::Edge,
        pattern: &crate::cypher::ast::RelPattern,
    ) -> bool {
        if let Some(ref expected_type) = pattern.rel_type {
            if &edge.edge_type != expected_type {
                return false;
            }
        }
        for (k, expected_v) in &pattern.properties {
            if k == "weight" {
                if (edge.weight - expected_v.as_f64().unwrap_or(edge.weight)).abs() > f64::EPSILON {
                    return false;
                }
                continue;
            }
            if edge.get_prop(k) != Some(expected_v) {
                return false;
            }
        }
        true
    }

    fn node_matches_pattern(&self, node_id: u64, pattern: &NodePattern) -> bool {
        let node = match self.graph.get_node(node_id) {
            Ok(Some(n)) => n,
            _ => return false,
        };

        for label in &pattern.labels {
            if !node.has_label(label) {
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

    /// 求值布尔表达式（WHERE / 谓词）
    pub fn eval_expr_truthy(&self, expr: &Expr, ctx: &RowCtx) -> bool {
        match expr {
            Expr::Literal(Value::Bool(b)) => *b,
            Expr::LabelCheck { var, label } => match ctx.get(var) {
                Some(Binding::Node(id)) => self
                    .graph
                    .get_node(*id)
                    .ok()
                    .flatten()
                    .map(|n| n.has_label(label))
                    .unwrap_or(false),
                _ => false,
            },
            Expr::BinaryOp { left, op, right } => match op {
                BinaryOperator::And => {
                    self.eval_expr_truthy(left, ctx) && self.eval_expr_truthy(right, ctx)
                }
                BinaryOperator::Or => {
                    self.eval_expr_truthy(left, ctx) || self.eval_expr_truthy(right, ctx)
                }
                _ => {
                    let l_val = self.eval_expr_value(left, ctx);
                    let r_val = self.eval_expr_value(right, ctx);
                    match (l_val, r_val) {
                        (Some(l), Some(r)) => {
                            // null 参与比较一律为 false（三值逻辑的安全近似）
                            if is_null(&l) || is_null(&r) {
                                return false;
                            }
                            match op {
                                BinaryOperator::Eq => l == r,
                                BinaryOperator::Neq => l != r,
                                BinaryOperator::Lt => l < r,
                                BinaryOperator::Lte => l <= r,
                                BinaryOperator::Gt => l > r,
                                BinaryOperator::Gte => l >= r,
                                _ => false,
                            }
                        }
                        _ => false,
                    }
                }
            },
            _ => true,
        }
    }

    /// 求值任意表达式的值
    pub fn eval_expr_value(&self, expr: &Expr, ctx: &RowCtx) -> Option<Value> {
        match expr {
            Expr::Literal(v) => Some(v.clone()),
            Expr::PropertyAccess { var, prop } => self.render_property(var, prop, ctx).ok(),
            Expr::Variable(var) => match ctx.get(var) {
                Some(Binding::Node(id)) => Some(Value::Int(*id as i64)),
                Some(Binding::Edge(id)) => Some(Value::Int(*id as i64)),
                None => None,
            },
            Expr::LabelCheck { var, label } => match ctx.get(var) {
                Some(Binding::Node(id)) => Some(Value::Bool(
                    self.graph
                        .get_node(*id)
                        .ok()
                        .flatten()
                        .map(|n| n.has_label(label))
                        .unwrap_or(false),
                )),
                _ => Some(Value::Bool(false)),
            },
            Expr::BinaryOp { .. } => {
                if self.eval_expr_truthy(expr, ctx) {
                    Some(Value::Bool(true))
                } else {
                    Some(Value::Bool(false))
                }
            }
        }
    }
}

/// 把变量绑定进上下文；若同名变量已绑定到不同实体则判定连接失败。
///
/// 这保证 `MATCH (a)-[:R]->(a)` 这类**同名变量**被解释为连接约束
/// （起止必须是同一节点），而不是把先前绑定静默覆盖掉。
fn bind_or_reject(ctx: &mut RowCtx, var: &str, binding: Binding) -> bool {
    match ctx.get(var) {
        Some(existing) if *existing != binding => false,
        Some(_) => true,
        None => {
            ctx.insert(var.to_string(), binding);
            true
        }
    }
}

/// 合并两个上下文（共享变量必须绑定一致，否则连接失败）
fn merge_contexts(base: &RowCtx, candidate: &RowCtx) -> Option<RowCtx> {
    let mut merged = base.clone();
    for (k, v) in candidate {
        match merged.get(k) {
            Some(existing) if existing != v => return None,
            Some(_) => {}
            None => {
                merged.insert(k.clone(), *v);
            }
        }
    }
    Some(merged)
}

/// 依据遍历方向求取边上「对端」节点
fn neighbor_of(edge: &crate::graph::Edge, from: u64) -> u64 {
    if edge.src_id == from && edge.dst_id == from {
        from
    } else if edge.src_id == from {
        edge.dst_id
    } else if edge.dst_id == from {
        edge.src_id
    } else {
        edge.dst_id
    }
}

fn null_value() -> Value {
    Value::from("null")
}

fn is_null(v: &Value) -> bool {
    matches!(v, Value::String(s) if s == "null")
}

/// 完整 Cypher 执行器（支持 CREATE / SET / DELETE 等写操作）
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
            CypherStatement::Create { pattern } => self.execute_create(&[pattern]),
            CypherStatement::Match {
                patterns,
                where_clause,
                set_clause,
                delete_clause,
                create_clause,
                return_clause,
                order_by,
                skip,
                limit,
            } => {
                for p in &patterns {
                    if let Some(start_pat) = p.nodes.first() {
                        for lbl in &start_pat.labels {
                            self.index_mgr.ensure_label_index(self.graph, lbl);
                        }
                    }
                }

                let mut properties_set = 0;
                let mut nodes_created = 0;
                let mut edges_created = 0;
                let mut nodes_deleted = 0;
                let mut edges_deleted = 0;

                let writes =
                    !set_clause.is_empty() || delete_clause.is_some() || create_clause.is_some();

                if !writes {
                    let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
                    return ro.execute_match(
                        patterns,
                        where_clause,
                        return_clause,
                        &order_by,
                        skip,
                        limit,
                    );
                }

                let matched = {
                    let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
                    ro.find_matches(&patterns, &where_clause)?
                };

                if !set_clause.is_empty() {
                    properties_set += self.apply_set(&set_clause, &matched)?;
                }

                if let Some(ref cc) = create_clause {
                    let (nc, ec) = self.apply_create_clause(cc, &matched)?;
                    nodes_created += nc;
                    edges_created += ec;
                }

                if let Some(ref del) = delete_clause {
                    let (nd, ed) = self.apply_delete(del, &matched)?;
                    nodes_deleted += nd;
                    edges_deleted += ed;
                }

                // 写语句若有 RETURN 子句，按变更后的图状态重新投影
                if let Some(items) = return_clause {
                    let refreshed = {
                        let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
                        ro.find_matches(&patterns, &where_clause)?
                    };
                    let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
                    let mut result = ro.project_results(
                        &patterns,
                        refreshed,
                        Some(items),
                        &order_by,
                        skip,
                        limit,
                    )?;
                    result.stats = ExecuteResult {
                        nodes_created,
                        edges_created,
                        nodes_deleted,
                        edges_deleted,
                        properties_set,
                        message: format!(
                            "Set {} properties, created {} nodes / {} edges, deleted {} nodes / {} edges.",
                            properties_set, nodes_created, edges_created, nodes_deleted, edges_deleted
                        ),
                    };
                    return Ok(result);
                }

                let stats = ExecuteResult {
                    nodes_created,
                    edges_created,
                    nodes_deleted,
                    edges_deleted,
                    properties_set,
                    message: format!(
                        "Set {} properties, created {} nodes / {} edges, deleted {} nodes / {} edges.",
                        properties_set, nodes_created, edges_created, nodes_deleted, edges_deleted
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
        }
    }

    /// 执行 CREATE 语句（纯磁盘写入，维护二级索引）
    fn execute_create(&mut self, patterns: &[PathPattern]) -> Result<CypherResultSet, GraphError> {
        let mut nodes_created = 0;
        let mut edges_created = 0;

        for pattern in patterns {
            let mut node_ids = Vec::new();

            for node_pat in &pattern.nodes {
                let labels: HashSet<String> = node_pat.labels.iter().cloned().collect();

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

    /// 应用 SET 子句：更新属性或追加标签（同步维护二级索引）
    fn apply_set(&mut self, items: &[SetItem], matched: &[RowCtx]) -> Result<usize, GraphError> {
        let mut applied = 0;

        for ctx in matched {
            for item in items {
                match item {
                    SetItem::Property { var, key, value } => {
                        let node_id = match ctx.get(var) {
                            Some(Binding::Node(id)) => *id,
                            Some(Binding::Edge(id)) => {
                                let val = {
                                    let ro =
                                        CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
                                    ro.eval_expr_value(value, ctx).unwrap_or_else(null_value)
                                };
                                // 边属性不参与二级索引，直接写入磁盘溢出页
                                self.graph.update_edge_property(*id, key.clone(), val)?;
                                applied += 1;
                                continue;
                            }
                            None => continue,
                        };

                        let val = {
                            let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
                            ro.eval_expr_value(value, ctx).unwrap_or_else(null_value)
                        };

                        let old_val = self
                            .graph
                            .get_node(node_id)?
                            .and_then(|n| n.get_prop(key).cloned());

                        self.graph
                            .update_node_property(node_id, key.clone(), val.clone())?;

                        if let Some(node) = self.graph.get_node(node_id)? {
                            for l in &node.labels {
                                if let Some(ref ov) = old_val {
                                    if ov != &val {
                                        self.index_mgr.remove_property(l, key, ov, node_id);
                                    }
                                }
                                self.index_mgr.insert_property(l, key, val.clone(), node_id);
                                self.graph.index_catalog.labels.insert(l.clone());
                                self.graph
                                    .index_catalog
                                    .properties
                                    .insert((l.clone(), key.clone()));
                            }
                        }
                        applied += 1;
                    }
                    SetItem::Label { var, label } => {
                        let node_id = match ctx.get(var) {
                            Some(Binding::Node(id)) => *id,
                            _ => continue,
                        };

                        let mut node = match self.graph.get_node(node_id)? {
                            Some(n) => n,
                            None => continue,
                        };
                        if node.has_label(label) {
                            continue;
                        }
                        node.labels.insert(label.clone());
                        // 原地改写节点载荷：绝不触发新节点分配或计数膨胀
                        self.graph.update_node_payload(
                            node_id,
                            node.labels.clone(),
                            node.properties.clone(),
                        )?;

                        self.index_mgr.insert_label(label, node_id);
                        self.graph.index_catalog.labels.insert(label.clone());
                        for (k, v) in &node.properties {
                            self.index_mgr.insert_property(label, k, v.clone(), node_id);
                            self.graph
                                .index_catalog
                                .properties
                                .insert((label.clone(), k.clone()));
                        }
                        applied += 1;
                    }
                }
            }
        }

        Ok(applied)
    }

    /// 应用 MATCH ... CREATE 子句：复用已绑定变量，仅创建新模式中的新实体
    fn apply_create_clause(
        &mut self,
        pattern: &PathPattern,
        matched: &[RowCtx],
    ) -> Result<(usize, usize), GraphError> {
        let mut nodes_created = 0;
        let mut edges_created = 0;

        for ctx in matched {
            let mut node_ids: Vec<u64> = Vec::with_capacity(pattern.nodes.len());

            for node_pat in &pattern.nodes {
                if let Some(ref var) = node_pat.variable {
                    if let Some(Binding::Node(existing)) = ctx.get(var) {
                        node_ids.push(*existing);
                        continue;
                    }
                }

                let labels: HashSet<String> = node_pat.labels.iter().cloned().collect();
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
        }

        Ok((nodes_created, edges_created))
    }

    /// 应用 DELETE / DETACH DELETE：级联删除关联边并回收 Freelist 槽位
    fn apply_delete(
        &mut self,
        del: &DeleteClause,
        matched: &[RowCtx],
    ) -> Result<(usize, usize), GraphError> {
        let mut nodes_deleted = 0;
        let mut edges_deleted = 0;
        let mut deleted_nodes: HashSet<u64> = HashSet::new();
        let mut deleted_edges: HashSet<u64> = HashSet::new();

        for ctx in matched {
            for target_var in &del.targets {
                let binding = match ctx.get(target_var) {
                    Some(b) => *b,
                    None => continue,
                };

                match binding {
                    Binding::Node(id) => {
                        if !deleted_nodes.insert(id) {
                            continue;
                        }

                        // 先读取节点当前的邻接边（级联删除会清空这些链表）
                        let node = match self.graph.get_node(id)? {
                            Some(n) => n,
                            None => continue,
                        };
                        let live_edges: Vec<u64> = node
                            .outgoing
                            .iter()
                            .chain(node.incoming.iter())
                            .copied()
                            .filter(|eid| {
                                self.graph.read_edge_record(*eid).ok().flatten().is_some()
                            })
                            .collect();

                        // 标准 Cypher 语义：非 DETACH 删除带关联边的节点必须显式报错
                        if !del.detach && !live_edges.is_empty() {
                            return Err(GraphError::General(format!(
                                "Cannot delete node {} because it still has relationships. \
                                 Use DETACH DELETE to delete the node and its relationships.",
                                id
                            )));
                        }

                        let removed = self.graph.remove_node(id)?;
                        nodes_deleted += 1;
                        for eid in live_edges {
                            if deleted_edges.insert(eid) {
                                edges_deleted += 1;
                            }
                        }
                        let labels_bt: BTreeSet<String> = removed.labels.into_iter().collect();
                        self.index_mgr
                            .remove_node_all_indices(id, &labels_bt, &removed.properties);
                    }
                    Binding::Edge(id) => {
                        if deleted_edges.insert(id) {
                            if self.graph.remove_edge(id).is_ok() {
                                edges_deleted += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok((nodes_deleted, edges_deleted))
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
    if statement.is_mutating() {
        return Err(GraphError::General(
            "Read-only executor cannot execute mutating Cypher statement".into(),
        ));
    }
    match statement {
        CypherStatement::Match {
            patterns,
            where_clause,
            return_clause,
            order_by,
            skip,
            limit,
            ..
        } => {
            let executor = CypherReadOnlyExecutor::new(graph, index_mgr);
            executor.execute_match(
                patterns,
                where_clause,
                return_clause,
                &order_by,
                skip,
                limit,
            )
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
