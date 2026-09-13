use crate::cypher::ast::{
    AggregateArg, AggregateFunc, BinaryOperator, CypherStatement, DeleteClause, ExecuteResult,
    Expr, MatchClause, NodePattern, OrderItem, PathPattern, ReturnItem, SetItem,
};
use crate::cypher::planner::{plan_pattern_order, PlanStats};
use crate::disk_graph::DiskGraph;
use crate::graph::{Direction, GraphError, Value};
use crate::index::IndexManager;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// 结果集行
#[derive(Debug, Clone)]
pub struct Row {
    pub values: Vec<Value>,
}

/// Cypher 查询结果集
#[derive(Debug, Clone, Default)]
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

/// `create_patterns` 的返回值：`(新建节点数, 新建边数, 新节点的变量绑定)`。
///
/// 第三个元素让 `UNWIND ... CREATE (n ...) RETURN n.x` 能读到刚写入的值。
type CreateOutcome = (usize, usize, Vec<(String, u64)>);

/// 变量绑定：明确区分节点、边与标量值。
///
/// 节点与边用独立变体，彻底杜绝「边 ID 与节点 ID 同号混淆」。
/// `Value` 变体供 `UNWIND ... AS x` 绑定列表元素——那些元素不是实体，没有 ID，
/// 用 `Int(id)` 冒充会让 `RETURN x` 打印出一个没有意义的数字。
///
/// 因为 `Value` 含 `String`/`List`，`Binding` 不再是 `Copy`。
#[derive(Debug, Clone, PartialEq)]
pub enum Binding {
    Node(u64),
    Edge(u64),
    Value(Value),
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
        // LIMIT 下推：能提前停就绝不展开全部。
        //
        // 安全性条件必须全部成立才可下推，缺一不可：
        // - 无 ORDER BY：排序需要全部行，提前截断会拿到错误的有序前缀
        // - 无 SKIP：SKIP 要先丢掉 N 行，截断位置会算错
        // - 无聚合：count/sum 需要遍历全部行；聚合由 `has_aggregate` 判定
        let pushdown = Self::limit_pushdown(return_clause.as_deref(), order_by, skip, limit);
        let matched = self.find_matches_limited(&patterns, &where_clause, pushdown)?;
        self.project_results(&patterns, matched, return_clause, order_by, skip, limit)
    }

    /// 判断 LIMIT 能否安全下推到匹配阶段；能则返回下推上限。
    ///
    /// 返回 `None` 表示必须展开全部匹配。
    fn limit_pushdown(
        return_clause: Option<&[ReturnItem]>,
        order_by: &[OrderItem],
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Option<usize> {
        let limit = limit?;
        // ORDER BY 需要全部行才能排序；SKIP 需要先丢弃前缀
        if !order_by.is_empty() || skip.is_some() {
            return None;
        }
        // 聚合（含 `RETURN *` 以外任何 count/sum/avg/min/max）需要全部行
        if let Some(items) = return_clause {
            if items
                .iter()
                .any(|i| matches!(i, ReturnItem::Aggregate { .. }))
            {
                return None;
            }
            // `RETURN *` 的列集合由上下文推导，也需要看到全部行才能定列
            if items.iter().any(|i| matches!(i, ReturnItem::All)) {
                return None;
            }
        }
        Some(limit)
    }

    /// 与 [`Self::find_matches`] 相同，但允许在凑够 `cap` 行后提前停止。
    ///
    /// 这是 `LIMIT` 的性能实现：`LIMIT 1` 在 32000 条边的图上从 60 秒降到常数级，
    /// 因为不再展开全部结果再截断。
    ///
    /// **语义不变**：`cap` 只影响何时停止枚举，返回的行及其顺序与不设上限时
    /// 所取的前 `cap` 行完全一致（连通性枚举本身是确定性的）。
    fn find_matches_limited(
        &self,
        patterns: &[PathPattern],
        where_clause: &Option<Expr>,
        cap: Option<usize>,
    ) -> Result<Vec<RowCtx>, GraphError> {
        if cap.is_none() {
            return self.find_matches(patterns, where_clause);
        }
        let cap = cap.unwrap_or(0);
        if cap == 0 {
            // LIMIT 0：不需要匹配任何行
            return Ok(Vec::new());
        }

        // 多模式连接会放大行数，无法在单模式阶段安全截断；
        // 单模式（最常见）才下推。
        if patterns.len() != 1 {
            return self.find_matches(patterns, where_clause);
        }

        self.find_single_pattern_matches_capped(&patterns[0], where_clause, cap)
    }

    /// 多模式匹配：按代价模型定序，再以索引嵌套循环求解。
    ///
    /// ## 与旧实现的关系
    ///
    /// 旧实现把每个模式**各自**求完整匹配集，再两两相乘做一致性连接。当后一个模式
    /// 的起点与前面的模式**共享变量**时，这次相乘是纯浪费：后一个模式会先在全图展开，
    /// 之后才用共享变量把绝大部分结果过滤掉。
    ///
    /// 现在由 `planner` 决定求解顺序，并在起点变量已被绑定时**只从那个节点展开**。
    ///
    /// 单模式路径的结果与行序与旧实现**逐位相同**：定序退化为 `[0]`，而「空上下文 +
    /// 枚举候选起点」正是原来的 `find_single_pattern_matches`。
    pub fn find_matches(
        &self,
        patterns: &[PathPattern],
        where_clause: &Option<Expr>,
    ) -> Result<Vec<RowCtx>, GraphError> {
        if patterns.is_empty() {
            return Ok(Vec::new());
        }

        let stats = PlanStats::new(self.graph, self.index_mgr);
        let order = plan_pattern_order(patterns, &stats);

        let mut combined: Vec<RowCtx> = vec![RowCtx::new()];

        for planned in order.iter() {
            let pattern = &patterns[planned.index];

            let mut next: Vec<RowCtx> = Vec::new();
            for base in &combined {
                // WHERE 交给每个模式：`find_initial_candidates` 只在 WHERE 里出现
                // **本模式起点变量**的等值/范围条件时才会用它收窄候选，否则原样退回
                // 标签索引或全扫。因此对不相干的模式它是惰性的。
                //
                // 定序之前只有 `patterns[0]` 得到这份收窄，而 `patterns[0]` 未必是
                // 被选中先执行的那个。传给它自己，收窄才跟着它走；漏传只是少一次
                // 优化（行集不变），传错模式也不可能——收窄条件仍受最终 WHERE 过滤。
                self.expand_pattern(pattern, base, where_clause, &mut next)?;
            }

            if next.is_empty() {
                return Ok(Vec::new());
            }
            combined = next;
        }

        if let Some(ref w) = where_clause {
            combined.retain(|ctx| self.eval_expr_truthy(w, ctx));
        }

        Ok(combined)
    }

    /// 在一个既有行上下文上展开一个模式（索引嵌套循环的内层）。
    ///
    /// 起点变量若已被前面的模式绑定，就直接从那个节点展开；否则枚举候选起点。
    /// 这条 `if` 就是复杂度差异的来源：绑定存在时，展开量由「一个节点」决定，
    /// 而不是由「该模式在全图中的匹配总数」决定。
    fn expand_pattern(
        &self,
        pattern: &PathPattern,
        base: &RowCtx,
        where_clause: &Option<Expr>,
        out: &mut Vec<RowCtx>,
    ) -> Result<(), GraphError> {
        let Some(start_pat) = pattern.nodes.first() else {
            return Ok(());
        };

        if let Some(var) = &start_pat.variable {
            if let Some(Binding::Node(bound_id)) = base.get(var) {
                let start_id = *bound_id;
                // 模式对起点可能还有额外约束（`MATCH (a:P), (a:Q)-[:R]->(b)`），
                // 已绑定不等于满足本模式的标签/属性要求。
                if self.graph.read_node_record(start_id)?.is_some()
                    && self.node_matches_pattern(start_id, start_pat)
                {
                    self.match_path_step(pattern, 0, start_id, base, out)?;
                }
                return Ok(());
            }
        }

        for start_id in self.find_initial_candidates(start_pat, where_clause)? {
            // 通过定长 NodeRecord 做快速过滤，避免无谓的溢出页调入
            if self.graph.read_node_record(start_id)?.is_none() {
                continue;
            }
            if !self.node_matches_pattern(start_id, start_pat) {
                continue;
            }

            let mut ctx = base.clone();
            if let Some(ref var) = start_pat.variable {
                // 同名变量是连接约束而非覆盖
                if !bind_or_reject(&mut ctx, var, Binding::Node(start_id)) {
                    continue;
                }
            }
            self.match_path_step(pattern, 0, start_id, &ctx, out)?;
        }

        Ok(())
    }

    /// 与单模式展开相同，但凑够 `cap` 行即停。
    ///
    /// 这是 `LIMIT` 下推的实现（`execute_match` → `limit_pushdown`）。它走的是
    /// **未定序**的单模式路径，与 [`Self::expand_pattern`] 的区别只有两点：起点
    /// 恒为空上下文，以及行数达到 `cap` 后停止枚举。
    ///
    /// 提前停止的条件是**外层候选循环**：一旦 `matched.len() >= cap` 就不再尝试
    /// 下一个起点。这是 `LIMIT` 真正的加速点——`LIMIT 1` 只需要第一个能匹配的
    /// 起点，无需展开整个图。
    ///
    /// 返回的仍是完整匹配（可能略多于 `cap`），由调用方按原有语义截断；
    /// 这里只保证「不会少于」且「按同一确定顺序」。
    fn find_single_pattern_matches_capped(
        &self,
        pattern: &PathPattern,
        where_clause: &Option<Expr>,
        cap: usize,
    ) -> Result<Vec<RowCtx>, GraphError> {
        if pattern.nodes.is_empty() {
            return Ok(Vec::new());
        }

        let start_pat = &pattern.nodes[0];
        let candidate_start_nodes = self.find_initial_candidates(start_pat, where_clause)?;

        let mut matched = Vec::new();
        for start_id in candidate_start_nodes {
            if matched.len() >= cap {
                break;
            }
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

            // 下推到本起点的行先收进临时缓冲，**过滤 WHERE 之后**才计入 cap。
            //
            // 这里不能直接把 `match_path_step` 的输出并进 `matched`：`cap` 统计的
            // 必须是「已经满足 WHERE 的行数」。若先按未过滤的行数截断，`LIMIT n`
            // 就可能返回少于 n 行——而那些被丢掉的行里本来有满足条件的。
            let mut produced = Vec::new();
            self.match_path_step(pattern, 0, start_id, &ctx, &mut produced)?;

            match where_clause {
                Some(w) => {
                    matched.extend(produced.into_iter().filter(|c| self.eval_expr_truthy(w, c)))
                }
                None => matched.extend(produced),
            }
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
            ReturnItem::Function { name, arg, alias } => alias.clone().unwrap_or_else(|| {
                let arg_repr = match &**arg {
                    Expr::Variable(v) => v.clone(),
                    other => format!("{:?}", other),
                };
                format!("{}({})", name, arg_repr)
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
                            values.push(self.render_binding(var, &ctx[var])?);
                        }
                    }
                    ReturnItem::Variable { var, .. } => match ctx.get(var) {
                        Some(binding) => values.push(self.render_binding(var, binding)?),
                        None => values.push(null_value()),
                    },
                    ReturnItem::Property { var, prop, .. } => {
                        values.push(self.render_property(var, prop, ctx)?)
                    }
                    ReturnItem::Aggregate { .. } => {}
                    ReturnItem::Function { name, arg, .. } => {
                        values.push(
                            self.eval_expr_value(
                                &Expr::FunctionCall {
                                    name: name.clone(),
                                    args: vec![(**arg).clone()],
                                },
                                ctx,
                            )
                            .unwrap_or_else(null_value),
                        );
                    }
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
                AggregateArg::Variable(var) => match ctx.get(var) {
                    // 标量绑定：聚合取**值本身**。
                    //
                    // 这里原来无条件 `collected.push(Value::Int(1))`，于是
                    // `UNWIND [1,2,3,4] AS x RETURN sum(x)` 返回 4（行数）而不是 10。
                    // 在 `UNWIND` 出现之前，变量只能绑定节点或边，这条路径很难被触到；
                    // 而「对展开出来的数值求和」恰恰是 UNWIND 最常见的用法。
                    Some(Binding::Value(v)) => {
                        if !is_null(v) {
                            non_null_count += 1;
                            collected.push(v.clone());
                        }
                    }
                    // 实体绑定：`count(x)` 只关心「绑定了」；而 sum/avg/min/max 需要
                    // 一个数值，实体不是数值。静默按 1 累加会给出一个看起来合法却
                    // 毫无意义的数字，所以显式报错。
                    Some(Binding::Node(_) | Binding::Edge(_)) => {
                        non_null_count += 1;
                        if func != AggregateFunc::Count {
                            return Err(GraphError::General(format!(
                                "{}({}) is not a number: aggregate over an entity is undefined. \
                                 Use {}({}.property) to aggregate a property.",
                                func.as_str(),
                                var,
                                func.as_str(),
                                var
                            )));
                        }
                    }
                    None => {}
                },
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
                } else if collected.iter().all(|v| matches!(v, Value::Int(_))) {
                    // 全整数时**必须用 i64 累加**，不能借道 f64。
                    //
                    // f64 的尾数只有 53 位，超过 2^53 的整数无法精确表示。原来的实现
                    // 是 `sum as f64` 再 `as i64`，于是：
                    //
                    // ```text
                    // 写入 9007199254740993 (2^53+1) -> sum() 读回 9007199254740992
                    // ```
                    //
                    // 差 1 且**没有任何提示**。这类静默错误正是本项目一直在消除的。
                    let mut acc: i64 = 0;
                    for v in &collected {
                        let Value::Int(i) = v else {
                            unreachable!("checked all-Int above")
                        };
                        acc = acc.checked_add(*i).ok_or_else(|| {
                            GraphError::General(format!(
                                "sum() overflowed i64 (adding {} to {})",
                                i, acc
                            ))
                        })?;
                    }
                    Value::Int(acc)
                } else {
                    Value::Float(collected.iter().filter_map(|v| v.as_f64()).sum())
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
                Some(binding) => self.render_binding(var, binding),
                None => Ok(null_value()),
            },
            ReturnItem::Property { var, prop, .. } => self.render_property(var, prop, ctx),
            ReturnItem::All => Ok(null_value()),
            ReturnItem::Aggregate { .. } => Ok(null_value()),
            ReturnItem::Function { name, arg, .. } => Ok(self
                .eval_expr_value(
                    &Expr::FunctionCall {
                        name: name.clone(),
                        args: vec![(**arg).clone()],
                    },
                    ctx,
                )
                .unwrap_or_else(null_value)),
        }
    }

    fn render_binding(&self, var: &str, binding: &Binding) -> Result<Value, GraphError> {
        let _ = var;
        match binding {
            Binding::Node(id) => {
                if let Some(node) = self.graph.get_node(*id)? {
                    let json = crate::json::map_to_string(&node.properties);
                    Ok(Value::from(json))
                } else {
                    Ok(Value::from("{}"))
                }
            }
            Binding::Edge(id) => {
                if let Some(edge) = self.graph.get_edge(*id)? {
                    let json = crate::json::map_to_string(&edge.properties);
                    Ok(Value::from(json))
                } else {
                    Ok(Value::from("{}"))
                }
            }
            Binding::Value(v) => Ok(v.clone()),
        }
    }

    /// 构造 `UNWIND <expr> AS <var>` 的行集合。
    ///
    /// 读路径与写路径共用这一份实现：`UNWIND` 的展开语义（非列表值当单元素、
    /// 元素的绑定方式）必须完全一致，否则 `UNWIND ... RETURN` 与
    /// `UNWIND ... CREATE` 会对同一输入给出不同行数。
    fn unwind_rows(
        &self,
        expr: &Expr,
        variable: &str,
    ) -> Result<(Vec<RowCtx>, PathPattern), GraphError> {
        let seed = RowCtx::new();
        let value = self.eval_expr_value(expr, &seed).ok_or_else(|| {
            GraphError::General(
                "UNWIND requires an evaluable list expression (e.g. `UNWIND [1,2,3] AS x`)"
                    .to_string(),
            )
        })?;

        let items: Vec<Value> = match value {
            Value::List(items) => items,
            // 单个非列表值当作单元素列表：`UNWIND 1 AS x` 产生一行。
            other => vec![other],
        };

        let rows: Vec<RowCtx> = items
            .into_iter()
            .map(|v| {
                let mut ctx = RowCtx::new();
                ctx.insert(variable.to_string(), Binding::Value(v));
                ctx
            })
            .collect();

        // 列集合的推导依据。`RETURN *` 在没有模式可依赖时必须至少给出 UNWIND 变量，
        // 否则会返回一张零列的表。
        let synthetic = PathPattern {
            nodes: vec![NodePattern {
                variable: Some(variable.to_string()),
                labels: Vec::new(),
                properties: HashMap::new(),
            }],
            edges: Vec::new(),
        };

        Ok((rows, synthetic))
    }

    /// 执行只读的 `UNWIND ... RETURN ...`（无 `CREATE`）。
    pub fn execute_unwind(
        &self,
        expr: &Expr,
        variable: &str,
        return_clause: Option<Vec<ReturnItem>>,
        order_by: &[OrderItem],
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<CypherResultSet, GraphError> {
        let (rows, synthetic) = self.unwind_rows(expr, variable)?;
        let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
        ro.project_results(&[synthetic], rows, return_clause, order_by, skip, limit)
    }

    /// 把模式的属性映射求值为具体值。
    ///
    /// `RowCtx::default()` 用于 MATCH：模式属性必须与行无关（Cypher 的
    /// `MATCH (a {k: b.v})` 里 `b` 尚未绑定），因此遇到变量会**报错**而不是
    /// 静默判为不匹配。§12 的「禁止静默读错误」同样适用于匹配条件：
    /// 一个求不出来的条件如果退化成「永远不匹配」，调用方只会看到 0 行。
    fn resolve_pattern_properties(
        &self,
        props: &HashMap<String, Expr>,
        ctx: &RowCtx,
    ) -> Result<HashMap<String, Value>, GraphError> {
        if props.is_empty() {
            return Ok(HashMap::new());
        }
        let mut out = HashMap::with_capacity(props.len());
        for (k, expr) in props {
            match self.eval_expr_value(expr, ctx) {
                Some(v) => {
                    out.insert(k.clone(), v);
                }
                None => {
                    return Err(GraphError::General(format!(
                        "Cannot evaluate property `{k}`: pattern property values must be \
                         literals or expressions resolvable from the row (found `{expr:?}`)"
                    )))
                }
            }
        }
        Ok(out)
    }

    /// 与 `resolve_pattern_properties` 相同，但额外拒绝无法落盘的值。
    ///
    /// 写入路径必须走这里：`Value::Null` 表示「删除该属性」，`Value::List` 无编码，
    /// 二者都不是可存储的属性值。让它们进入 `add_node` 会被 `PropCodec` 拒绝，
    /// 但那条错误信息只会说「无法编码」，不会指出是哪个键。
    fn resolve_storable_properties(
        &self,
        props: &HashMap<String, Expr>,
        ctx: &RowCtx,
    ) -> Result<HashMap<String, Value>, GraphError> {
        let resolved = self.resolve_pattern_properties(props, ctx)?;
        for (k, v) in &resolved {
            if !v.is_storable() {
                return Err(GraphError::General(format!(
                    "Property `{k}` cannot be stored: {v:?} is not a storable value \
                     (null deletes a property in SET, and lists are not encodable)"
                )));
            }
        }
        Ok(resolved)
    }

    fn render_property(&self, var: &str, prop: &str, ctx: &RowCtx) -> Result<Value, GraphError> {
        match ctx.get(var) {
            // 标量绑定没有属性可取。返回 NULL 而不是报错：`UNWIND ... AS x RETURN x.k`
            // 在 Cypher 里就是 NULL（属性访问不存在的属性也是 NULL）。
            Some(Binding::Value(_)) => Ok(null_value()),
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
    /// 推导起点候选节点。
    ///
    /// 返回 `Result` 而非 `Vec`：全表扫描路径原来是
    /// `all_node_ids().unwrap_or_default()`，于是**读错误会变成空候选集**，
    /// 查询静默返回 0 行。实测把数据库截断到 1/3 后，`MATCH (n:T) RETURN count(*)`
    /// 返回 0（原 3000 节点）且不报任何错——调用方看到的是「这张表是空的」，
    /// 而真相是「有一页读不出来」。这正是 AGENTS.md §12 禁止的静默读错误。
    fn find_initial_candidates(
        &self,
        node_pat: &NodePattern,
        where_clause: &Option<Expr>,
    ) -> Result<Vec<u64>, GraphError> {
        if let Some(lbl) = node_pat.labels.first() {
            if self.index_mgr.is_label_complete(lbl) {
                // 索引探测只在属性值为**字面量**时可用：索引里存的是常量，
                // 而 `{k: someVar}` 的值取决于行上下文，无从查表。
                for (key, val_expr) in &node_pat.properties {
                    if let Expr::Literal(val) = val_expr {
                        if let Some(set) = self.index_mgr.find_by_property_exact(lbl, key, val) {
                            return Ok(set.iter().copied().collect());
                        }
                    }
                }

                if let (Some(ref w_expr), Some(ref v_name)) = (where_clause, &node_pat.variable) {
                    if let Some(candidates) = self.try_find_from_where_expr(lbl, w_expr, v_name) {
                        return Ok(candidates);
                    }
                }

                if let Some(set) = self.index_mgr.find_by_label(lbl) {
                    return Ok(set.iter().copied().collect());
                }
            }
        }

        self.graph.all_node_ids()
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
        let expected = match self.resolve_pattern_properties(&pattern.properties, &RowCtx::new()) {
            Ok(m) => m,
            // 解析期已拒绝非字面量属性，走到这里说明有内部调用绕过了校验。
            // 宁可判为不匹配（真实磁盘状态不会无辜受损）也不能假装匹配成功。
            Err(_) => return false,
        };
        for (k, expected_v) in &expected {
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

    /// 判断节点是否满足模式（标签与属性约束）。
    ///
    /// ## 为什么不用 `graph.get_node`
    ///
    /// `get_node` 会顺带遍历该节点的**整条出边链与入边链**（它返回一个完整的
    /// `Node`，含邻接表）。而这里只需要标签与属性。
    ///
    /// 这个区别在本项目里是 O(N) 与 O(N²) 的分界：`match_path_step` 对每一步的
    /// 每个候选目标都调用本函数，若每次都走完整条邻接链，那么在一个度为 D 的
    /// 枢纽上展开 1 跳的代价就是 O(D²) 而不是 O(D)。
    ///
    /// 实测（4 个枢纽、边数翻倍）：改用 `read_node_data` 之前，1000/2000/4000/8000
    /// 条边分别耗时 55/126/573/2385 ms —— 典型的平方增长。
    fn node_matches_pattern(&self, node_id: u64, pattern: &NodePattern) -> bool {
        // 无约束时无需触碰磁盘：大多数模式不是每个节点都带标签与属性
        if pattern.labels.is_empty() && pattern.properties.is_empty() {
            // 仍需确认节点存在（已删除的槽位不得匹配）
            return matches!(self.graph.read_node_record(node_id), Ok(Some(_)));
        }

        let record = match self.graph.read_node_record(node_id) {
            Ok(Some(r)) => r,
            _ => return false,
        };

        let data = match self.graph.read_node_data(record.prop_page_id) {
            Ok(d) => d,
            Err(_) => return false,
        };

        for label in &pattern.labels {
            if !data.labels.contains(label) {
                return false;
            }
        }

        if !pattern.properties.is_empty() {
            let expected =
                match self.resolve_pattern_properties(&pattern.properties, &RowCtx::new()) {
                    Ok(m) => m,
                    Err(_) => return false,
                };
            for (k, expected_v) in &expected {
                if data.properties.get(k) != Some(expected_v) {
                    return false;
                }
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
            // 列表字面量：逐元素求值。任一元素求值失败则整体为 None——
            // 返回一个「部分填充的列表」会让 UNWIND 产生错误的行数，而错误行数
            // 是静默的：调用方只看到少了几个结果。
            Expr::ListLiteral(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.eval_expr_value(item, ctx)?);
                }
                Some(Value::List(out))
            }
            Expr::PropertyAccess { var, prop } => self.render_property(var, prop, ctx).ok(),
            Expr::Variable(var) => match ctx.get(var) {
                Some(Binding::Node(id)) => Some(Value::Int(*id as i64)),
                Some(Binding::Edge(id)) => Some(Value::Int(*id as i64)),
                // `UNWIND [1,2,3] AS x ... RETURN x` 走这条：x 绑定的是值本身
                Some(Binding::Value(v)) => Some(v.clone()),
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
            Expr::FunctionCall { name, args } => self.eval_scalar_function(name, args, ctx),
        }
    }

    /// 求值标量函数：`id(x)`、`labels(n)`、`type(r)`。
    ///
    /// 函数名已在解析期校验过（未知函数直接报错），因此这里的 `_` 分支不可达；
    /// 返回 `None` 而非 panic，是为了不给「解析器与求值器不一致」留下崩溃点。
    fn eval_scalar_function(&self, name: &str, args: &[Expr], ctx: &RowCtx) -> Option<Value> {
        use crate::cypher::ast::ScalarFunc;

        let func = ScalarFunc::from_name(name)?;
        if args.len() != func.arity() {
            return None;
        }

        // 参数求值为一个绑定；三种函数都只接受节点或边
        let binding = match &args[0] {
            Expr::Variable(var) => ctx.get(var)?.clone(),
            // `id(r)` 之外的嵌套（如 `id(other.x)`）没有意义：ID 不是属性
            _ => return None,
        };

        match func {
            ScalarFunc::Id => match binding {
                Binding::Node(id) | Binding::Edge(id) => Some(Value::Int(id as i64)),
                // 标量没有 ID：`id('a')` 是 NULL，不是 0
                Binding::Value(_) => None,
            },
            ScalarFunc::Labels => match binding {
                Binding::Node(id) => {
                    let node = self.graph.get_node(id).ok().flatten()?;
                    // 标签列表按字典序输出，保证同一节点每次渲染一致
                    let mut labels: Vec<String> = node.labels.iter().cloned().collect();
                    labels.sort_unstable();
                    Some(Value::String(labels.join(",")))
                }
                Binding::Edge(_) | Binding::Value(_) => None,
            },
            ScalarFunc::Type => match binding {
                Binding::Edge(id) => {
                    let edge = self.graph.get_edge(id).ok().flatten()?;
                    Some(Value::String(edge.edge_type.clone()))
                }
                Binding::Node(_) | Binding::Value(_) => None,
            },
        }
    }
}

/// 把变量绑定进上下文；若同名变量已绑定到不同实体则判定连接失败。
///
/// 这保证 `MATCH (a)-[:R]->(a)` 这类**同名变量**被解释为连接约束
/// （起止必须是同一节点），而不是把先前绑定静默覆盖掉。
fn bind_or_reject(ctx: &mut RowCtx, var: &str, binding: Binding) -> bool {
    match ctx.get(var) {
        Some(existing) if existing != &binding => false,
        Some(_) => true,
        None => {
            ctx.insert(var.to_string(), binding);
            true
        }
    }
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

/// 构造 Cypher 的 null。
///
/// 这里曾返回**字符串** `"null"`，于是「值是字符串 "null"」与「值是空」无法区分。
/// 实测后果：一个 `name = 'null'` 的节点在 `count(c.name)` 里被静默忽略，任何
/// `count`/`sum`/`avg` 都会漏掉这类数据。现在返回真正的 `Value::Null`。
fn null_value() -> Value {
    Value::Null
}

/// 是否为 null。
///
/// 委托给 `Value::is_null`，使判定只有一处实现——判定逻辑分散正是这个缺陷的成因。
fn is_null(v: &Value) -> bool {
    v.is_null()
}

/// 构造 CREATE 语句返回的状态结果集。
///
/// `execute_create` 与 `execute_unwind` 都要用它：两条路径各自拼一遍
/// message 格式，迟早会出现「一条说 3 nodes，另一条说 3 个节点」的分歧。
fn create_result_set(nodes_created: usize, edges_created: usize) -> CypherResultSet {
    let stats = ExecuteResult {
        nodes_created,
        edges_created,
        nodes_deleted: 0,
        edges_deleted: 0,
        properties_set: 0,
        message: format!("Created {nodes_created} nodes, {edges_created} relationships."),
    };
    CypherResultSet {
        columns: vec!["Status".to_string()],
        rows: vec![Row {
            values: vec![Value::from(stats.message.clone())],
        }],
        stats,
    }
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

    /// 属性表达式求值（写路径）。
    ///
    /// 委托给只读执行器：属性求值不区分读写，两处各写一份必然漂移。
    /// 只读执行器只借用 `&DiskGraph`，所以这里的可变借用需要在调用点收窄——
    /// Rust 的重新借用（reborrow）在方法调用里自动完成。
    fn resolve_storable_properties(
        &self,
        props: &HashMap<String, Expr>,
        ctx: &RowCtx,
    ) -> Result<HashMap<String, Value>, GraphError> {
        CypherReadOnlyExecutor::new(self.graph, self.index_mgr)
            .resolve_storable_properties(props, ctx)
    }

    /// 生成执行计划文本（`EXPLAIN`）。
    ///
    /// ## 为什么需要它
    ///
    /// 查询慢的时候，用户能看到的只有「慢」。这个项目曾经在 `MATCH` 的起点选择上
    /// 有过 O(N²) 的展开（见 CHANGELOG），当时没有任何办法从外部看出走了哪条路径——
    /// 只能读源码。EXPLAIN 把「引擎实际打算怎么做」变成可观察的。
    ///
    /// ## 它描述的是真实决策，不是理想化的决策
    ///
    /// 每一行都对应 `find_initial_candidates` / `limit_pushdown` 里真实执行的分支。
    /// 如果计划与实际行为不符，那这个功能比没有更糟——因此渲染逻辑刻意与那些函数
    /// 保持一致的判断顺序。
    ///
    /// ## 只读
    ///
    /// 计划由 AST 与索引元数据推导，**不触碰磁盘**，也不执行内层语句。
    /// 因此在空库上同样可用，且没有副作用。
    fn explain_statement(
        stmt: &CypherStatement,
        graph: &DiskGraph,
        index_mgr: &IndexManager,
    ) -> CypherResultSet {
        let mut lines: Vec<String> = Vec::new();

        match stmt {
            CypherStatement::Explain(_) => {
                lines.push("EXPLAIN 不能嵌套".to_string());
            }
            CypherStatement::Create { pattern } => {
                lines.push("Create".to_string());
                lines.push("  └─ AllocateNodeOrEdge (写路径)".to_string());
                lines.push(format!("     pattern nodes: {}", pattern.nodes.len()));
                lines.push(format!("     pattern edges: {}", pattern.edges.len()));
            }
            CypherStatement::Merge {
                pattern,
                on_create,
                on_match,
                return_clause,
                ..
            } => {
                lines.push("WriteQuery (需要排他写锁)".to_string());
                lines.push("  ├─ Merge (整体匹配优先，不匹配才创建)".to_string());
                lines.push(format!("  │    pattern nodes: {}", pattern.nodes.len()));
                lines.push(format!("  │    pattern edges: {}", pattern.edges.len()));
                if !on_match.is_empty() {
                    lines.push(format!("  ├─ OnMatchSet ({} 项)", on_match.len()));
                }
                if !on_create.is_empty() {
                    lines.push(format!("  ├─ OnCreateSet ({} 项)", on_create.len()));
                }
                if return_clause.is_some() {
                    lines.push("  └─ Project".to_string());
                } else {
                    lines.push("  └─ (无 RETURN)".to_string());
                }
            }
            CypherStatement::Unwind {
                variable,
                create_clause,
                return_clause,
                ..
            } => {
                lines.push(if create_clause.is_some() {
                    "WriteQuery (需要排他写锁)".to_string()
                } else {
                    "ReadQuery (共享读锁即可)".to_string()
                });
                lines.push("  ├─ Unwind (逐元素展开为行)".to_string());
                lines.push(format!("  │    AS {}", variable));
                if create_clause.is_some() {
                    lines.push("  ├─ Create (每个元素建一个模式)".to_string());
                }
                if return_clause.is_some() {
                    lines.push("  └─ Project".to_string());
                } else {
                    lines.push("  └─ (无 RETURN)".to_string());
                }
            }
            CypherStatement::Match(clause) => {
                // 与 lib.rs 的路由判断同源：变更子句决定走写锁还是读锁
                let mutating = !clause.set_clause.is_empty()
                    || clause.delete_clause.is_some()
                    || clause.create_clause.is_some();
                lines.push(if mutating {
                    "WriteQuery (需要排他写锁)".to_string()
                } else {
                    "ReadQuery (共享读锁即可)".to_string()
                });

                // 连接定序：与 `find_matches` 同一份规划器输出，且**必须**同源。
                // 计划里报一个顺序、执行时用另一个，比没有计划更糟。
                let stats = PlanStats::new(graph, index_mgr);
                let order = plan_pattern_order(&clause.patterns, &stats);
                let multi = clause.patterns.len() > 1;

                if multi {
                    lines.push(format!(
                        "  ├─ JoinOrder ({} 个模式，按估计基数升序；共享变量优先)",
                        order.len()
                    ));
                }

                for (rank, planned) in order.iter().enumerate() {
                    let pattern = &clause.patterns[planned.index];
                    let label = if multi {
                        format!("模式 {} (原第 {} 位)", rank, planned.index)
                    } else {
                        format!("模式 {}", planned.index)
                    };
                    lines.push(format!("  ├─ Expand ({})", label));
                    if let Some(start) = pattern.nodes.first() {
                        // 起点依据的措辞与 `find_initial_candidates` 的判断顺序一致
                        let how =
                            Self::explain_start_selection(start, &clause.where_clause, index_mgr);
                        lines.push(format!("  │    StartNode: {}", how));
                        lines.push(format!(
                            "  │    Steps: {} node(s), {} edge(s)",
                            pattern.nodes.len(),
                            pattern.edges.len()
                        ));
                        for e in &pattern.edges {
                            let dir = match e.direction {
                                Direction::Outgoing => "->",
                                Direction::Incoming => "<-",
                                Direction::Both => "--",
                            };
                            let ty = e.rel_type.as_deref().unwrap_or("(any)");
                            let hops = match e.hops {
                                Some((lo, hi)) => format!(" *{}..{}", lo, hi),
                                None => String::new(),
                            };
                            lines.push(format!("  │      {} [{}]{}", dir, ty, hops));
                        }
                    }
                    if multi {
                        // 估计值必须连同**依据**一起打印：只说 0.02 行不变真假，
                        // 说清它是标签索引数出来的，才可核对。
                        let basis = if planned.estimate.start_is_measured {
                            "起点实测"
                        } else {
                            "起点为估计上界"
                        };
                        let driven = if planned.driven_by_binding {
                            "；起点变量已绑定 → 索引嵌套循环"
                        } else {
                            ""
                        };
                        lines.push(format!(
                            "  │    Est. rows: {:.0} ({}{})",
                            planned.estimate.rows, basis, driven
                        ));
                    }
                }

                if clause.where_clause.is_some() {
                    lines.push("  ├─ Filter (WHERE)".to_string());
                }

                // LIMIT 下推：与 `limit_pushdown` 同一组条件
                let pushback = CypherReadOnlyExecutor::limit_pushdown(
                    clause.return_clause.as_deref(),
                    &clause.order_by,
                    clause.skip,
                    clause.limit,
                );
                if clause.limit.is_some() {
                    match pushback {
                        Some(_) => lines.push(
                            "  ├─ Limit (已下推到匹配阶段：凑够即停，不再展开全部)".to_string(),
                        ),
                        None => {
                            let why = if !clause.order_by.is_empty() {
                                "ORDER BY 需要全部行"
                            } else if clause.skip.is_some() {
                                "SKIP 需要先丢弃前缀"
                            } else {
                                "聚合需要全部行"
                            };
                            lines.push(format!("  ├─ Limit (无法下推：{})", why));
                        }
                    }
                }

                let has_agg = clause.return_clause.as_ref().is_some_and(|items| {
                    items
                        .iter()
                        .any(|i| matches!(i, ReturnItem::Aggregate { .. }))
                });
                if has_agg {
                    lines.push("  ├─ Aggregate".to_string());
                }
                if !clause.order_by.is_empty() {
                    lines.push("  └─ Sort (ORDER BY)".to_string());
                } else {
                    lines.push("  └─ Project".to_string());
                }
            }
        }

        CypherResultSet {
            columns: vec!["plan".to_string()],
            rows: lines
                .into_iter()
                .map(|l| Row {
                    values: vec![Value::String(l)],
                })
                .collect(),
            stats: ExecuteResult {
                message: "Execution plan (query NOT executed).".to_string(),
                ..Default::default()
            },
        }
    }

    /// 起点选择的描述，判断顺序与 `find_initial_candidates` 一致。
    fn explain_start_selection(
        start: &NodePattern,
        where_clause: &Option<Expr>,
        index_mgr: &IndexManager,
    ) -> String {
        if let Some(lbl) = start.labels.first() {
            if index_mgr.is_label_complete(lbl) {
                for (key, val_expr) in &start.properties {
                    // 与 find_initial_candidates 同步：只有字面量能查索引
                    if let Expr::Literal(val) = val_expr {
                        if index_mgr.find_by_property_exact(lbl, key, val).is_some() {
                            return format!(
                                "property index (:{lbl} {{{key}: {val:?}}})",
                                lbl = lbl,
                                key = key,
                                val = val
                            );
                        }
                    }
                }
                if let (Some(_), Some(v)) = (where_clause, &start.variable) {
                    let _ = v;
                    return format!(
                        "label index (:{})  [WHERE 里的属性等值条件可进一步收窄]",
                        lbl
                    );
                }
                return format!("label index (:{})", lbl);
            }
            return format!("full scan (label :{} 的索引尚未建立；首次查询会构建)", lbl);
        }
        "full scan (起点无标签约束，将遍历全部节点)".to_string()
    }

    pub fn execute_statement(
        &mut self,
        stmt: CypherStatement,
    ) -> Result<CypherResultSet, GraphError> {
        match stmt {
            CypherStatement::Explain(inner) => {
                Ok(Self::explain_statement(&inner, self.graph, self.index_mgr))
            }
            CypherStatement::Create { pattern } => self.execute_create(&[pattern]),
            CypherStatement::Unwind {
                expr,
                variable,
                return_clause,
                order_by,
                skip,
                limit,
                create_clause,
            } => self.execute_unwind(
                &expr,
                &variable,
                return_clause,
                order_by,
                skip,
                limit,
                create_clause,
            ),
            CypherStatement::Merge {
                pattern,
                on_create,
                on_match,
                return_clause,
                order_by,
                skip,
                limit,
            } => self.execute_merge(
                &pattern,
                &on_create,
                &on_match,
                return_clause,
                order_by,
                skip,
                limit,
            ),
            CypherStatement::Match(clause) => {
                let MatchClause {
                    patterns,
                    where_clause,
                    set_clause,
                    delete_clause,
                    create_clause,
                    return_clause,
                    order_by,
                    skip,
                    limit,
                } = *clause;

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
        // 无行上下文的 CREATE：属性里不能引用变量
        // （`CREATE (n {k: x})` 只会由 `execute_unwind` 逐行构造上下文后调用）。
        // 返回值里的变量绑定在这里没有用处：纯 CREATE 的返回值是创建摘要。
        let (nodes_created, edges_created, _) = self.create_patterns(patterns, &RowCtx::new())?;
        Ok(create_result_set(nodes_created, edges_created))
    }

    /// 按给定行上下文创建一批模式。
    ///
    /// 返回 `(节点数, 边数, 新建节点的变量绑定)`。
    ///
    /// 第三个返回值是 `UNWIND ... CREATE (n ...) RETURN n.x` 能工作的原因：
    /// 新建节点的变量必须回填进行上下文，否则 `RETURN n.x` 会渲染成 NULL——
    /// 刚创建并写入了 `x` 的节点却报告「没有这个属性」，是静默的错误答案。
    ///
    /// `execute_create`（无上下文）与 `execute_unwind`（每行一个上下文）共用这一份
    /// 实现，避免两条写路径在唯一约束、索引维护上出现分歧。
    fn create_patterns(
        &mut self,
        patterns: &[PathPattern],
        ctx: &RowCtx,
    ) -> Result<CreateOutcome, GraphError> {
        let mut nodes_created = 0;
        let mut edges_created = 0;
        let mut bindings: Vec<(String, u64)> = Vec::new();

        for pattern in patterns {
            let mut node_ids = Vec::new();

            for node_pat in &pattern.nodes {
                let labels: HashSet<String> = node_pat.labels.iter().cloned().collect();
                let props = self.resolve_storable_properties(&node_pat.properties, ctx)?;

                // 唯一约束必须在这里也过一遍：Cypher 直接调 `DiskGraph::add_node`，
                // 绕过了 `NervusDb::add_node` 上的检查。
                self.index_mgr
                    .guard_unique_constraints(self.graph, &labels, &props, None)?;
                let node_id = self.graph.add_node(labels.clone(), props.clone())?;
                nodes_created += 1;
                node_ids.push(node_id);
                if let Some(ref var) = node_pat.variable {
                    bindings.push((var.clone(), node_id));
                }

                for l in &labels {
                    self.index_mgr.insert_label(l, node_id);
                    self.graph.index_catalog.labels.insert(l.clone());
                    for (k, v) in &props {
                        self.index_mgr.insert_property(l, k, v.clone(), node_id);
                        self.graph
                            .index_catalog
                            .properties
                            .insert((l.clone(), (*k).to_string()));
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
                let edge_props = self.resolve_storable_properties(&edge_pat.properties, ctx)?;

                self.graph
                    .add_edge(src_id, dst_id, &rel_type, edge_props, weight)?;
                edges_created += 1;
            }
        }

        Ok((nodes_created, edges_created, bindings))
    }

    /// 执行 `MERGE <pattern> [ON CREATE SET ...] [ON MATCH SET ...] [RETURN ...]`。
    ///
    /// 语义是**幂等写**：模式整体匹配到就复用，匹配不到才创建。这让「先查再写」
    /// 不必由调用方实现——调用方实现时「检查」与「写入」之间存在空隙，而那条
    /// 路径产生的重复数据是静默的，只在后续统计时表现为计数偏高。
    ///
    /// ## 为什么是整体匹配
    ///
    /// 与 Cypher 一致：`MERGE (a:X {k: 1})-[:R]->(b:Y {k: 2})` 只有在**整条路径**
    /// 存在时才复用。若允许部分复用，同一个查询在不同初始状态下会拼接出
    /// 「半新半旧」的路径，且没有明确规则可循。整体匹配让结果只取决于模式是否
    /// 完整存在，是唯一可预测的语义。
    ///
    /// ## 为什么必须走排他写锁
    ///
    /// `is_mutating()` 对 MERGE 恒返回 true，即使这次命中、一个字节都没写。
    /// 因为「是否写入」要匹配完才知道，而用共享读锁执行会让两个并发的 MERGE
    /// 同时判定「不存在」然后各创建一个——正是这个子句要消除的重复。
    #[allow(clippy::too_many_arguments)]
    fn execute_merge(
        &mut self,
        pattern: &PathPattern,
        on_create: &[SetItem],
        on_match: &[SetItem],
        return_clause: Option<Vec<ReturnItem>>,
        order_by: Vec<OrderItem>,
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<CypherResultSet, GraphError> {
        // 1) 先匹配。MERGE 的模式属性已被解析期限定为字面量，与 MATCH 同一套索引路径。
        let matched = {
            let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
            ro.find_matches(std::slice::from_ref(pattern), &None)?
        };

        let mut nodes_created = 0;
        let mut edges_created = 0;
        let mut properties_set = 0;

        if matched.is_empty() {
            // 2a) 不存在：创建整个模式，并把 ON CREATE SET 施加到新节点/边上
            let (nc, ec, _bindings) =
                self.create_patterns(std::slice::from_ref(pattern), &RowCtx::new())?;
            nodes_created = nc;
            edges_created = ec;

            if !on_create.is_empty() {
                // 重新匹配以拿到刚创建实体的变量绑定，再施加 SET。
                // 直接用 _bindings 也可以，但那样必须把「模式里哪个变量对应哪一行」
                // 再推导一遍；重新匹配复用同一条已测试的路径，少一份实现。
                let created = {
                    let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
                    ro.find_matches(std::slice::from_ref(pattern), &None)?
                };
                properties_set += self.apply_set(on_create, &created)?;
            }
        } else {
            // 2b) 已存在：只施加 ON MATCH SET。
            // 这一步可能修改属性，因此必须让二级索引跟上（apply_set 内部负责），
            // 否则索引会与磁盘数据不一致，后续按属性查询会漏结果。
            if !on_match.is_empty() {
                properties_set += self.apply_set(on_match, &matched)?;
            }
        }

        // 3) 投影：与 MATCH 的写路径相同——按变更后的图状态重新匹配再投影。
        // 用变更前的 matched 投影会让 RETURN 看到旧属性（甚至看到还没创建的节点）。
        let items = match return_clause {
            Some(items) => items,
            None => {
                let mut stats = create_result_set(nodes_created, edges_created).stats;
                stats.properties_set = properties_set;
                stats.message = format!(
                    "Merged: created {} nodes, {} relationships, set {} properties.",
                    nodes_created, edges_created, properties_set
                );
                return Ok(CypherResultSet {
                    columns: vec!["Status".to_string()],
                    rows: vec![Row {
                        values: vec![Value::from(stats.message.clone())],
                    }],
                    stats,
                });
            }
        };

        let refreshed = {
            let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
            ro.find_matches(std::slice::from_ref(pattern), &None)?
        };
        let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
        let mut result = ro.project_results(
            std::slice::from_ref(pattern),
            refreshed,
            Some(items),
            &order_by,
            skip,
            limit,
        )?;

        result.stats.nodes_created = nodes_created;
        result.stats.edges_created = edges_created;
        result.stats.properties_set = properties_set;
        Ok(result)
    }

    /// 执行 `UNWIND <expr> AS <var> [CREATE ...] [RETURN ...]`。
    ///
    /// 语义：把 `expr` 求值为列表，为**每个元素**产生一行并绑定到 `var`，
    /// 然后把可选子句施加到这些行上。这是把「批量数据」表达进查询的唯一方式：
    ///
    /// ```text
    /// UNWIND [1, 2, 3] AS x CREATE (n:Num {v: x})
    /// ```
    ///
    /// 一次调用创建 3 个节点，而不是需要 3 条语句。
    ///
    /// 行是**顺序**产生的，因此 `CREATE` 的结果顺序与列表顺序一致。
    #[allow(clippy::too_many_arguments)]
    fn execute_unwind(
        &mut self,
        expr: &Expr,
        variable: &str,
        return_clause: Option<Vec<ReturnItem>>,
        order_by: Vec<OrderItem>,
        skip: Option<usize>,
        limit: Option<usize>,
        create_clause: Option<PathPattern>,
    ) -> Result<CypherResultSet, GraphError> {
        // 行构造复用只读执行器：展开语义只有一处实现
        let (mut rows, synthetic) =
            CypherReadOnlyExecutor::new(self.graph, self.index_mgr).unwind_rows(expr, variable)?;

        let mut nodes_created = 0;
        let mut edges_created = 0;

        if let Some(pattern) = create_clause {
            // 逐行创建：每行一个模式实例。行数即创建个数。
            for ctx in rows.iter_mut() {
                let (n, e, bindings) = self.create_patterns(std::slice::from_ref(&pattern), ctx)?;
                nodes_created += n;
                edges_created += e;
                // 新建节点的变量回填进**这一行**，使 `RETURN n.x` 能拿到刚写入的值。
                // 不回填的话它会渲染成 NULL——刚创建成功的节点报告「没有该属性」。
                for (var, id) in bindings {
                    // 与 UNWIND 变量重名时保留 UNWIND 的绑定：那是本行的输入，
                    // 覆盖它会让后续表达式引用到意料之外的东西。
                    ctx.entry(var).or_insert(Binding::Node(id));
                }
            }
        }

        // 无 RETURN：返回创建摘要（与 CREATE 一致）
        let Some(items) = return_clause else {
            return Ok(create_result_set(nodes_created, edges_created));
        };

        // 有 RETURN：走与 MATCH 相同的投影管线（聚合 / ORDER BY / SKIP / LIMIT）。
        //
        // 复用只读执行器而不是复制一份投影实现——聚合与分页的语义在这里必须
        // 与 MATCH 路径完全一致，复制出来的第二份实现迟早会漂移。
        let ro = CypherReadOnlyExecutor::new(self.graph, self.index_mgr);
        let mut result =
            ro.project_results(&[synthetic], rows, Some(items), &order_by, skip, limit)?;

        // 投影管线只报行数；创建数必须叠加回来，否则调用方看不到写入了多少
        result.stats.nodes_created = nodes_created;
        result.stats.edges_created = edges_created;
        if nodes_created > 0 || edges_created > 0 {
            result.stats.message = format!(
                "Created {} nodes, {} relationships, returned {} rows.",
                nodes_created,
                edges_created,
                result.rows.len()
            );
        }
        Ok(result)
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
                            // 标量没有可写属性。静默跳过会让 `UNWIND [1] AS x SET x.k = 1`
                            // 报告成功却什么都没做，所以这里显式报错。
                            Some(Binding::Value(_)) => {
                                return Err(GraphError::General(format!(
                                    "SET requires a node or relationship, but `{var}` is a value"
                                )))
                            }
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
                // 属性可以引用已绑定的变量：`MATCH (a) CREATE (b {name: a.name})`
                let props = self.resolve_storable_properties(&node_pat.properties, ctx)?;
                // 同上：约束闸门对 `MATCH ... CREATE` 路径同样必须生效
                self.index_mgr
                    .guard_unique_constraints(self.graph, &labels, &props, None)?;
                let node_id = self.graph.add_node(labels.clone(), props.clone())?;
                nodes_created += 1;
                node_ids.push(node_id);

                for l in &labels {
                    self.index_mgr.insert_label(l, node_id);
                    self.graph.index_catalog.labels.insert(l.clone());
                    for (k, v) in &props {
                        self.index_mgr.insert_property(l, k, v.clone(), node_id);
                        self.graph
                            .index_catalog
                            .properties
                            .insert((l.clone(), (*k).to_string()));
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
                let edge_props = self.resolve_storable_properties(&edge_pat.properties, ctx)?;

                self.graph
                    .add_edge(src_id, dst_id, &rel_type, edge_props, weight)?;
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
                    Some(b) => b.clone(),
                    None => continue,
                };

                match binding {
                    Binding::Value(_) => {
                        return Err(GraphError::General(format!(
                            "DELETE requires a node or relationship, but `{target_var}` is a value"
                        )))
                    }
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
                        if deleted_edges.insert(id) && self.graph.remove_edge(id).is_ok() {
                            edges_deleted += 1;
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

    // EXPLAIN 优先：它只描述计划，既不读盘也不写入，因此在只读入口同样合法
    // ——包括 `EXPLAIN CREATE ...` 与 `EXPLAIN ... SET ...`。
    // 位置必须在 is_mutating 守卫**之前**，否则这些查询会被误判为写操作而拒绝。
    if let CypherStatement::Explain(inner) = &statement {
        return Ok(CypherExecutor::explain_statement(inner, graph, index_mgr));
    }

    if statement.is_mutating() {
        return Err(GraphError::General(
            "Read-only executor cannot execute mutating Cypher statement".into(),
        ));
    }
    match statement {
        CypherStatement::Match(clause) => {
            let executor = CypherReadOnlyExecutor::new(graph, index_mgr);
            executor.execute_match(
                clause.patterns,
                clause.where_clause,
                clause.return_clause,
                &clause.order_by,
                clause.skip,
                clause.limit,
            )
        }
        // 只读的 UNWIND（不带 CREATE）在只读句柄上同样合法：
        // `UNWIND [1,2,3] AS x RETURN x` 不写任何东西。
        // 带 CREATE 的版本已被上面的 `is_mutating` 守卫挡掉。
        CypherStatement::Unwind {
            expr,
            variable,
            return_clause,
            order_by,
            skip,
            limit,
            create_clause: None,
        } => {
            let executor = CypherReadOnlyExecutor::new(graph, index_mgr);
            executor.execute_unwind(&expr, &variable, return_clause, &order_by, skip, limit)
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
