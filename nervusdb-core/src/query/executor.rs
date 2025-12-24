//! Query Executor v2 - Index-Aware Execution Engine
//!
//! 解决 v1 Executor 的性能问题：
//! - v1: ScanNode 全表扫描构建节点集合 -> O(N)
//! - v2: 智能索引扫描 + Index Nested Loop Join -> O(log N)
//!
//! 关键改进：
//! 1. NodeScan 使用 Hexastore 索引而不是全表扫描
//! 2. Index Nested Loop Join 避免内存爆炸
//! 3. 基础统计信息支持 Join 顺序优化

use crate::error::Error;
use crate::query::ast::{
    BinaryOperator, Clause, Direction, ExistsExpression, Expression, FunctionCall,
    ListComprehension, Literal, PathElement, Pattern, PropertyAccess, PropertyMap,
    RelationshipDirection, RelationshipPattern,
};
use crate::query::planner::{
    AggregateFunction, AggregateNode, DistinctNode, ExpandNode, ExpandVarLengthNode, FilterNode,
    FtsCandidateScanNode, LeftOuterJoinNode, LimitNode, NestedLoopJoinNode, PhysicalPlan,
    ProjectNode, ScanNode, SingleRowNode, SkipNode, SortNode, UnwindNode, VectorTopKScanNode,
};
use crate::{Database, QueryCriteria, Triple};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Float(f64),
    Boolean(bool),
    Null,
    Vector(Vec<f32>),
    Node(u64),
    Relationship(Triple),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Vector(a), Value::Vector(b)) => a == b,
            (Value::Node(a), Value::Node(b)) => a == b,
            (Value::Relationship(a), Value::Relationship(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    pub values: HashMap<String, Value>,
}

impl Record {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn insert(&mut self, key: String, value: Value) {
        self.values.insert(key, value);
    }

    pub fn merge(&mut self, other: &Record) {
        for (k, v) in &other.values {
            self.values.insert(k.clone(), v.clone());
        }
    }
}

fn record_distinct_key(record: &Record) -> String {
    let mut keys: Vec<&String> = record.values.keys().collect();
    keys.sort();
    let mut out = String::new();
    for key in keys {
        if let Some(value) = record.values.get(key) {
            out.push_str(key);
            out.push('=');
            out.push_str(&format!("{:?};", value));
        }
    }
    out
}

impl Default for Record {
    fn default() -> Self {
        Self::new()
    }
}

fn try_merge_records(mut left: Record, right: Record) -> Option<Record> {
    for (k, v) in right.values {
        if let Some(existing) = left.values.get(&k) {
            if existing != &v {
                return None;
            }
        } else {
            left.values.insert(k, v);
        }
    }
    Some(left)
}

pub struct ExecutionContext<'a> {
    pub db: &'a Database,
    pub params: &'a HashMap<String, Value>,
}

/// Arc-based execution context for true streaming across FFI boundary
/// This allows iterators to hold a reference to the database without lifetime issues
pub struct ArcExecutionContext {
    pub db: std::sync::Arc<Database>,
    pub params: std::sync::Arc<HashMap<String, Value>>,
}

impl ArcExecutionContext {
    pub fn new(db: std::sync::Arc<Database>, params: HashMap<String, Value>) -> Self {
        Self {
            db,
            params: std::sync::Arc::new(params),
        }
    }
}

/// Owned execution context for FFI - uses raw pointer to avoid lifetime issues
/// SAFETY: The caller must ensure db_ptr remains valid for the lifetime of this context
pub struct OwnedExecutionContext {
    pub db_ptr: *const Database,
    pub params: HashMap<String, Value>,
}

impl OwnedExecutionContext {
    /// Returns a reference to the database.
    ///
    /// # Safety
    /// The caller must ensure `db_ptr` is valid and points to a live `Database` instance.
    pub unsafe fn db(&self) -> &Database {
        unsafe { &*self.db_ptr }
    }
}

/// 节点扫描统计信息
#[derive(Debug)]
pub struct ScanStats {
    pub estimated_cardinality: usize,
    pub has_labels: bool,
}

impl ScanStats {
    /// 计算 ScanNode 的预估基数
    pub fn estimate_scan_cardinality(db: &Database, labels: &[String]) -> Self {
        if labels.is_empty() {
            // 无标签过滤：需要获取所有唯一节点
            // 使用实际采样来估算：扫描少量数据并估算唯一节点数
            let mut unique_nodes = std::collections::HashSet::new();
            let sample_criteria = crate::QueryCriteria::default();
            let sample_count = 100; // 采样前100个三元组

            for triple in db.query(sample_criteria).take(sample_count) {
                unique_nodes.insert(triple.subject_id);
                unique_nodes.insert(triple.object_id);
            }

            // 如果采样了全部数据，返回精确值；否则按比例估算
            let estimated_nodes = if unique_nodes.len() < sample_count / 2 {
                unique_nodes.len() // 数据较少，返回精确值
            } else {
                unique_nodes.len() * 2 // 估算还有更多节点
            };

            Self {
                estimated_cardinality: estimated_nodes.max(1),
                has_labels: false,
            }
        } else {
            // 有标签过滤：估算有该标签的节点数
            let mut total_labeled_nodes = 0;

            // 解析 "type" 谓词 ID
            if let Ok(Some(type_id)) = db.resolve_id("type") {
                for label in labels {
                    if let Ok(Some(label_id)) = db.resolve_id(label) {
                        // 计算 (?, type, label) 的三元组数量
                        let criteria = QueryCriteria {
                            subject_id: None,
                            predicate_id: Some(type_id),
                            object_id: Some(label_id),
                        };

                        let labeled_count = db.query(criteria).count();
                        if total_labeled_nodes == 0 {
                            total_labeled_nodes = labeled_count;
                        } else {
                            // 多标签：取交集（假设独立性，实际会更小）
                            total_labeled_nodes = (total_labeled_nodes * labeled_count / 10).max(1);
                        }
                    }
                }
            }

            Self {
                estimated_cardinality: total_labeled_nodes.max(1),
                has_labels: true,
            }
        }
    }
}

// ============================================================================
// Enhanced Execution Plans
// ============================================================================

pub trait ExecutionPlan {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error>;

    /// 估算该操作的输出基数
    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize;
}

impl ExecutionPlan for PhysicalPlan {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        match self {
            PhysicalPlan::SingleRow(node) => node.execute(ctx),
            PhysicalPlan::Scan(node) => node.execute(ctx),
            PhysicalPlan::FtsCandidateScan(node) => node.execute(ctx),
            PhysicalPlan::VectorTopKScan(node) => node.execute(ctx),
            PhysicalPlan::Filter(node) => node.execute(ctx),
            PhysicalPlan::Project(node) => node.execute(ctx),
            PhysicalPlan::Limit(node) => node.execute(ctx),
            PhysicalPlan::Skip(node) => node.execute(ctx),
            PhysicalPlan::Sort(node) => node.execute(ctx),
            PhysicalPlan::Distinct(node) => node.execute(ctx),
            PhysicalPlan::Aggregate(node) => node.execute(ctx),
            PhysicalPlan::NestedLoopJoin(node) => node.execute(ctx),
            PhysicalPlan::LeftOuterJoin(node) => node.execute(ctx),
            PhysicalPlan::Expand(node) => node.execute(ctx),
            PhysicalPlan::ExpandVarLength(node) => node.execute(ctx),
            PhysicalPlan::Unwind(node) => node.execute(ctx),
            _ => Err(Error::Other("Unsupported physical plan type".to_string())),
        }
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        match self {
            PhysicalPlan::SingleRow(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Scan(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::FtsCandidateScan(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::VectorTopKScan(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Filter(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Project(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Limit(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Skip(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Sort(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Distinct(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Aggregate(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::NestedLoopJoin(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::LeftOuterJoin(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Expand(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::ExpandVarLength(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Unwind(node) => node.estimate_cardinality(ctx),
            _ => 1,
        }
    }
}

use std::sync::Arc;

impl PhysicalPlan {
    /// Execute the plan with Arc-based context for true streaming across FFI boundary.
    /// Returns a 'static iterator that owns its database reference.
    ///
    /// Note: The iterator is NOT Send because Database contains non-Send fields.
    /// This is fine for FFI use where calls are typically single-threaded.
    pub fn execute_streaming(
        self,
        ctx: Arc<ArcExecutionContext>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'static>, Error> {
        match self {
            PhysicalPlan::SingleRow(_) => Ok(Box::new(std::iter::once(Ok(Record::new())))),
            PhysicalPlan::Scan(node) => {
                let alias = node.alias;
                let labels = node.labels;
                let db = Arc::clone(&ctx.db);

                if labels.is_empty() {
                    Ok(Box::new(scan_all_nodes_streaming(db, alias)))
                } else {
                    Ok(Box::new(scan_labeled_nodes_streaming(db, labels, alias)))
                }
            }
            PhysicalPlan::FtsCandidateScan(node) => {
                #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
                {
                    let alias = node.alias;
                    let labels = node.labels;
                    let property = node.property;
                    let query_expr = node.query;
                    let db = Arc::clone(&ctx.db);

                    let query = match resolve_query_string_streaming(&query_expr, &ctx) {
                        Some(q) => q,
                        None => {
                            return Ok(Box::new(std::iter::empty())
                                as Box<dyn Iterator<Item = Result<Record, Error>> + 'static>);
                        }
                    };

                    let Some(scores) = db.fts_scores_for_query(property.as_str(), query.as_str())
                    else {
                        return Ok(Box::new(std::iter::empty())
                            as Box<dyn Iterator<Item = Result<Record, Error>> + 'static>);
                    };

                    let candidate_ids: Vec<u64> = scores.keys().copied().collect();
                    let type_and_labels = resolve_label_ids_streaming(&db, &labels);

                    return Ok(Box::new(candidate_ids.into_iter().filter_map(
                        move |node_id| {
                            if let Some((type_id, label_ids)) = type_and_labels.as_ref()
                                && !node_has_labels_streaming(&db, node_id, *type_id, label_ids)
                            {
                                return None;
                            }

                            let mut record = Record::new();
                            record.insert(alias.clone(), Value::Node(node_id));
                            Some(Ok(record))
                        },
                    )));
                }

                #[cfg(not(all(feature = "fts", not(target_arch = "wasm32"))))]
                {
                    let _ = node;
                    Ok(Box::new(std::iter::empty())
                        as Box<
                            dyn Iterator<Item = Result<Record, Error>> + 'static,
                        >)
                }
            }
            PhysicalPlan::VectorTopKScan(node) => {
                // Delegate to the non-streaming implementation and materialize the limited result
                // set, then hand out a 'static iterator for FFI.
                let exec_ctx = ExecutionContext {
                    db: ctx.db.as_ref(),
                    params: ctx.params.as_ref(),
                };
                let iter = node.execute(&exec_ctx)?;
                let mut records = Vec::new();
                for item in iter {
                    records.push(item?);
                }
                Ok(Box::new(records.into_iter().map(Ok)))
            }
            PhysicalPlan::Filter(node) => {
                let input_iter = node.input.execute_streaming(Arc::clone(&ctx))?;
                let predicate = node.predicate;
                let ctx_clone = Arc::clone(&ctx);
                Ok(Box::new(input_iter.filter(move |result| {
                    match result {
                        Ok(record) => evaluate_expression_streaming(&predicate, record, &ctx_clone),
                        Err(_) => true, // Pass through errors
                    }
                })))
            }
            PhysicalPlan::Project(node) => {
                let input_iter = node.input.execute_streaming(Arc::clone(&ctx))?;
                let projections = node.projections;
                let ctx_clone = Arc::clone(&ctx);
                Ok(Box::new(input_iter.map(move |result| {
                    result.map(|record| {
                        let mut new_record = Record::new();
                        for (expr, alias) in &projections {
                            let value =
                                evaluate_expression_value_streaming(expr, &record, &ctx_clone);
                            new_record.insert(alias.clone(), value);
                        }
                        new_record
                    })
                })))
            }
            PhysicalPlan::Limit(node) => {
                let limit = usize::try_from(node.limit).unwrap_or(usize::MAX);
                let input_iter = node.input.execute_streaming(ctx)?;
                Ok(Box::new(input_iter.take(limit)))
            }
            PhysicalPlan::Skip(node) => {
                let skip = usize::try_from(node.skip).unwrap_or(0);
                let input_iter = node.input.execute_streaming(ctx)?;
                Ok(Box::new(input_iter.skip(skip)))
            }
            PhysicalPlan::Sort(node) => {
                let input_iter = node.input.execute_streaming(Arc::clone(&ctx))?;
                let order_by = node.order_by;
                let ctx_clone = Arc::clone(&ctx);
                // Sort requires materialization - collect all records, sort, then iterate
                let mut records: Vec<Record> = input_iter.filter_map(|r| r.ok()).collect();
                records.sort_by(|a, b| {
                    for (expr, direction) in &order_by {
                        let val_a = evaluate_expression_value_streaming(expr, a, &ctx_clone);
                        let val_b = evaluate_expression_value_streaming(expr, b, &ctx_clone);
                        let cmp = compare_values_for_sort(&val_a, &val_b, direction);
                        if cmp != std::cmp::Ordering::Equal {
                            return cmp;
                        }
                    }
                    std::cmp::Ordering::Equal
                });
                Ok(Box::new(records.into_iter().map(Ok)))
            }
            PhysicalPlan::Distinct(node) => {
                let input_iter = node.input.execute_streaming(Arc::clone(&ctx))?;
                let mut seen: HashSet<String> = HashSet::new();
                Ok(Box::new(input_iter.filter_map(
                    move |result| match result {
                        Ok(record) => {
                            let key = record_distinct_key(&record);
                            if seen.insert(key) {
                                Some(Ok(record))
                            } else {
                                None
                            }
                        }
                        Err(err) => Some(Err(err)),
                    },
                )))
            }
            PhysicalPlan::Expand(node) => {
                let input_iter = node.input.execute_streaming(Arc::clone(&ctx))?;
                Ok(Box::new(StreamingExpandIterator::new(
                    input_iter,
                    node.start_node_alias,
                    node.rel_alias,
                    node.end_node_alias,
                    node.direction,
                    node.rel_type,
                    ctx,
                )))
            }
            PhysicalPlan::Unwind(node) => {
                let input_iter = node.input.execute_streaming(Arc::clone(&ctx))?;
                let expression = node.expression;
                let alias = node.alias;
                let ctx_clone = Arc::clone(&ctx);
                Ok(Box::new(input_iter.flat_map(move |result| {
                    match result {
                        Ok(record) => {
                            match unwind_values_streaming(&expression, &record, &ctx_clone) {
                                Ok(values) => values
                                    .into_iter()
                                    .map(|value| {
                                        let mut new_record = record.clone();
                                        new_record.insert(alias.clone(), value);
                                        Ok(new_record)
                                    })
                                    .collect::<Vec<_>>(),
                                Err(err) => vec![Err(err)],
                            }
                        }
                        Err(err) => vec![Err(err)],
                    }
                })))
            }
            PhysicalPlan::ExpandVarLength(node) => {
                let input_iter = node.input.execute_streaming(Arc::clone(&ctx))?;
                Ok(Box::new(StreamingExpandVarLengthIterator {
                    input_iter,
                    start_node_alias: node.start_node_alias,
                    end_node_alias: node.end_node_alias,
                    direction: node.direction,
                    rel_type: node.rel_type,
                    min_hops: node.min_hops,
                    max_hops: node.max_hops,
                    ctx,
                    current_record: None,
                    current_expansions: None,
                }))
            }
            PhysicalPlan::NestedLoopJoin(node) => {
                let left_iter = node.left.execute_streaming(Arc::clone(&ctx))?;
                let right_plan = *node.right;
                let predicate = node.predicate;
                Ok(Box::new(StreamingNestedLoopJoin::new(
                    left_iter, right_plan, predicate, ctx,
                )))
            }
            PhysicalPlan::LeftOuterJoin(node) => {
                let left_iter = node.left.execute_streaming(Arc::clone(&ctx))?;
                let right_plan = *node.right;
                Ok(Box::new(StreamingLeftOuterJoin::new(
                    left_iter,
                    right_plan,
                    node.right_aliases,
                    ctx,
                )))
            }
            _ => Err(Error::Other(
                "Unsupported physical plan type for streaming".to_string(),
            )),
        }
    }
}

// ============================================================================
// Streaming Iterator Implementations
// ============================================================================

/// Streaming version of scan_all_nodes_optimized
fn scan_all_nodes_streaming(
    db: Arc<Database>,
    alias: String,
) -> impl Iterator<Item = Result<Record, Error>> + Send + 'static {
    let mut unique_nodes = HashSet::new();
    let subject_criteria = crate::QueryCriteria::default();

    for triple in db.query(subject_criteria).take(10000) {
        unique_nodes.insert(triple.subject_id);
        unique_nodes.insert(triple.object_id);
    }

    unique_nodes.into_iter().map(move |node_id| {
        let mut record = Record::new();
        record.insert(alias.clone(), Value::Node(node_id));
        Ok(record)
    })
}

/// Streaming version of scan_labeled_nodes_optimized
fn scan_labeled_nodes_streaming(
    db: Arc<Database>,
    labels: Vec<String>,
    alias: String,
) -> impl Iterator<Item = Result<Record, Error>> + Send + 'static {
    let type_id = match db.resolve_id("type") {
        Ok(Some(id)) => Some(id),
        _ => None,
    };

    let node_ids: Vec<u64> = if let Some(type_id) = type_id {
        let mut nodes = HashSet::new();
        for label in &labels {
            if let Ok(Some(label_id)) = db.resolve_id(label) {
                let criteria = crate::QueryCriteria {
                    subject_id: None,
                    predicate_id: Some(type_id),
                    object_id: Some(label_id),
                };
                for triple in db.query(criteria) {
                    nodes.insert(triple.subject_id);
                }
            }
        }
        nodes.into_iter().collect()
    } else {
        Vec::new()
    };

    node_ids.into_iter().map(move |node_id| {
        let mut record = Record::new();
        record.insert(alias.clone(), Value::Node(node_id));
        Ok(record)
    })
}

/// Streaming nested loop join iterator
struct StreamingNestedLoopJoin {
    left_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'static>,
    right_plan: PhysicalPlan,
    predicate: Option<Expression>,
    ctx: Arc<ArcExecutionContext>,
    current_left: Option<Record>,
    current_right: Option<Box<dyn Iterator<Item = Result<Record, Error>> + 'static>>,
}

impl StreamingNestedLoopJoin {
    fn new(
        left_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'static>,
        right_plan: PhysicalPlan,
        predicate: Option<Expression>,
        ctx: Arc<ArcExecutionContext>,
    ) -> Self {
        Self {
            left_iter,
            right_plan,
            predicate,
            ctx,
            current_left: None,
            current_right: None,
        }
    }
}

impl Iterator for StreamingNestedLoopJoin {
    type Item = Result<Record, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Try to get next from current right iterator
            if let Some(ref mut right_iter) = self.current_right
                && let Some(right_result) = right_iter.next()
            {
                match right_result {
                    Ok(right_record) => {
                        let left_record = self.current_left.as_ref().unwrap();
                        let mut merged = left_record.clone();
                        for (k, v) in right_record.values {
                            merged.insert(k, v);
                        }

                        // Apply predicate if any
                        if let Some(ref pred) = self.predicate
                            && !evaluate_expression_streaming(pred, &merged, &self.ctx)
                        {
                            continue;
                        }
                        return Some(Ok(merged));
                    }
                    Err(e) => return Some(Err(e)),
                }
            }

            // Get next left record
            match self.left_iter.next()? {
                Ok(left_record) => {
                    self.current_left = Some(left_record);
                    // Clone the right plan and execute it
                    match self
                        .right_plan
                        .clone()
                        .execute_streaming(Arc::clone(&self.ctx))
                    {
                        Ok(right_iter) => {
                            self.current_right = Some(right_iter);
                        }
                        Err(e) => return Some(Err(e)),
                    }
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

// SAFETY: StreamingNestedLoopJoin is not Send - FFI calls are single-threaded

/// Streaming left outer join iterator (for OPTIONAL MATCH)
struct StreamingLeftOuterJoin {
    left_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'static>,
    right_plan: PhysicalPlan,
    right_aliases: Vec<String>,
    ctx: Arc<ArcExecutionContext>,
    current_left: Option<Record>,
    current_right: Option<Box<dyn Iterator<Item = Result<Record, Error>> + 'static>>,
    matched_current_left: bool,
    emitted_null_current_left: bool,
}

impl StreamingLeftOuterJoin {
    fn new(
        left_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'static>,
        right_plan: PhysicalPlan,
        right_aliases: Vec<String>,
        ctx: Arc<ArcExecutionContext>,
    ) -> Self {
        Self {
            left_iter,
            right_plan,
            right_aliases,
            ctx,
            current_left: None,
            current_right: None,
            matched_current_left: false,
            emitted_null_current_left: false,
        }
    }

    fn emit_null_row(&mut self, mut left_record: Record) -> Record {
        for alias in &self.right_aliases {
            left_record
                .values
                .entry(alias.clone())
                .or_insert(Value::Null);
        }
        left_record
    }
}

impl Iterator for StreamingLeftOuterJoin {
    type Item = Result<Record, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut right_iter) = self.current_right {
                if let Some(right_result) = right_iter.next() {
                    match right_result {
                        Ok(right_record) => {
                            let left_record = self.current_left.as_ref().unwrap().clone();
                            if let Some(merged) = try_merge_records(left_record, right_record) {
                                self.matched_current_left = true;
                                return Some(Ok(merged));
                            }
                            continue;
                        }
                        Err(e) => return Some(Err(e)),
                    }
                }

                // Right exhausted; maybe emit NULL row, then advance.
                self.current_right = None;
                if !self.matched_current_left && !self.emitted_null_current_left {
                    self.emitted_null_current_left = true;
                    let left_record = self.current_left.take().unwrap();
                    return Some(Ok(self.emit_null_row(left_record)));
                }
                self.current_left = None;
                self.matched_current_left = false;
                self.emitted_null_current_left = false;
                continue;
            }

            // Load next left record.
            match self.left_iter.next()? {
                Ok(left_record) => {
                    self.current_left = Some(left_record);
                    self.matched_current_left = false;
                    self.emitted_null_current_left = false;
                    match self
                        .right_plan
                        .clone()
                        .execute_streaming(Arc::clone(&self.ctx))
                    {
                        Ok(right_iter) => self.current_right = Some(right_iter),
                        Err(e) => return Some(Err(e)),
                    }
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

/// Streaming expand iterator
struct StreamingExpandIterator {
    input_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'static>,
    start_node_alias: String,
    rel_alias: String,
    end_node_alias: String,
    direction: RelationshipDirection,
    rel_type: Option<String>,
    ctx: Arc<ArcExecutionContext>,
    current_record: Option<Record>,
    current_expansions: Option<std::vec::IntoIter<(crate::Triple, u64)>>,
}

impl StreamingExpandIterator {
    fn new(
        input_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'static>,
        start_node_alias: String,
        rel_alias: String,
        end_node_alias: String,
        direction: RelationshipDirection,
        rel_type: Option<String>,
        ctx: Arc<ArcExecutionContext>,
    ) -> Self {
        Self {
            input_iter,
            start_node_alias,
            rel_alias,
            end_node_alias,
            direction,
            rel_type,
            ctx,
            current_record: None,
            current_expansions: None,
        }
    }

    fn get_expansions(&self, node_id: u64) -> Vec<(crate::Triple, u64)> {
        let db = &self.ctx.db;
        let mut results = Vec::new();

        let rel_type_id = self
            .rel_type
            .as_ref()
            .and_then(|t| db.resolve_id(t).ok().flatten());

        match self.direction {
            RelationshipDirection::LeftToRight | RelationshipDirection::Undirected => {
                let criteria = crate::QueryCriteria {
                    subject_id: Some(node_id),
                    predicate_id: rel_type_id,
                    object_id: None,
                };
                for triple in db.query(criteria) {
                    results.push((triple, triple.object_id));
                }
            }
            _ => {}
        }

        if self.direction == RelationshipDirection::RightToLeft {
            let criteria = crate::QueryCriteria {
                subject_id: None,
                predicate_id: rel_type_id,
                object_id: Some(node_id),
            };
            for triple in db.query(criteria) {
                results.push((triple, triple.subject_id));
            }
        }

        results
    }
}

impl Iterator for StreamingExpandIterator {
    type Item = Result<Record, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Try to get next expansion from current record
            if let Some(ref mut expansions) = self.current_expansions
                && let Some((triple, end_node_id)) = expansions.next()
            {
                let record = self.current_record.as_ref().unwrap();
                let mut new_record = record.clone();
                new_record.insert(self.rel_alias.clone(), Value::Relationship(triple));
                new_record.insert(self.end_node_alias.clone(), Value::Node(end_node_id));
                return Some(Ok(new_record));
            }

            // Get next input record
            match self.input_iter.next()? {
                Ok(record) => {
                    if let Some(Value::Node(node_id)) = record.values.get(&self.start_node_alias) {
                        let expansions = self.get_expansions(*node_id);
                        self.current_record = Some(record);
                        self.current_expansions = Some(expansions.into_iter());
                    }
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

/// Streaming version of evaluate_expression
fn evaluate_expression_streaming(
    expr: &Expression,
    record: &Record,
    ctx: &ArcExecutionContext,
) -> bool {
    match evaluate_expression_value_streaming(expr, record, ctx) {
        Value::Boolean(b) => b,
        _ => false,
    }
}

/// Streaming version of evaluate_expression_value
fn evaluate_expression_value_streaming(
    expr: &Expression,
    record: &Record,
    ctx: &ArcExecutionContext,
) -> Value {
    match expr {
        Expression::Literal(l) => match l {
            Literal::String(s) => Value::String(s.clone()),
            Literal::Float(f) => Value::Float(*f),
            Literal::Integer(i) => Value::Float(*i as f64),
            Literal::Boolean(b) => Value::Boolean(*b),
            Literal::Null => Value::Null,
        },
        Expression::Variable(name) => record.get(name).cloned().unwrap_or(Value::Null),
        Expression::Parameter(name) => ctx.params.get(name).cloned().unwrap_or(Value::Null),
        Expression::PropertyAccess(pa) => {
            if let Some(Value::Node(node_id)) = record.get(&pa.variable)
                && let Ok(Some(binary)) = ctx.db.get_node_property_binary(*node_id)
                && let Ok(props) = crate::storage::property::deserialize_properties(&binary)
                && let Some(value) = props.get(&pa.property)
            {
                return match value {
                    serde_json::Value::String(s) => Value::String(s.clone()),
                    serde_json::Value::Number(n) => Value::Float(n.as_f64().unwrap_or(0.0)),
                    serde_json::Value::Bool(b) => Value::Boolean(*b),
                    serde_json::Value::Null => Value::Null,
                    serde_json::Value::Array(items) => {
                        let mut out = Vec::with_capacity(items.len());
                        for item in items {
                            let Some(n) = item.as_f64() else {
                                return Value::String(
                                    serde_json::Value::Array(items.clone()).to_string(),
                                );
                            };
                            out.push(n as f32);
                        }
                        Value::Vector(out)
                    }
                    _ => Value::Null,
                };
            }
            Value::Null
        }
        Expression::Binary(b) => {
            let left = evaluate_expression_value_streaming(&b.left, record, ctx);
            let right = evaluate_expression_value_streaming(&b.right, record, ctx);

            match b.operator {
                BinaryOperator::Equal => Value::Boolean(left == right),
                BinaryOperator::NotEqual => Value::Boolean(left != right),
                BinaryOperator::And => match (left, right) {
                    (Value::Boolean(l), Value::Boolean(r)) => Value::Boolean(l && r),
                    _ => Value::Null,
                },
                BinaryOperator::Or => match (left, right) {
                    (Value::Boolean(l), Value::Boolean(r)) => Value::Boolean(l || r),
                    _ => Value::Null,
                },
                BinaryOperator::LessThan => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l < r),
                    _ => Value::Null,
                },
                BinaryOperator::LessThanOrEqual => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l <= r),
                    _ => Value::Null,
                },
                BinaryOperator::GreaterThan => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l > r),
                    _ => Value::Null,
                },
                BinaryOperator::GreaterThanOrEqual => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l >= r),
                    _ => Value::Null,
                },
                BinaryOperator::Add => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Float(l + r),
                    (Value::String(l), Value::String(r)) => Value::String(format!("{}{}", l, r)),
                    _ => Value::Null,
                },
                BinaryOperator::Subtract => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Float(l - r),
                    _ => Value::Null,
                },
                BinaryOperator::Multiply => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Float(l * r),
                    _ => Value::Null,
                },
                BinaryOperator::Divide => match (left, right) {
                    (Value::Float(l), Value::Float(r)) if r != 0.0 => Value::Float(l / r),
                    _ => Value::Null,
                },
                BinaryOperator::Modulo => match (left, right) {
                    (Value::Float(l), Value::Float(r)) if r != 0.0 => Value::Float(l % r),
                    _ => Value::Null,
                },
                BinaryOperator::In => value_in_list(&left, &right),
                BinaryOperator::NotIn => match value_in_list(&left, &right) {
                    Value::Boolean(b) => Value::Boolean(!b),
                    other => other,
                },
                BinaryOperator::StartsWith => match (left, right) {
                    (Value::String(l), Value::String(r)) => Value::Boolean(l.starts_with(&r)),
                    _ => Value::Null,
                },
                BinaryOperator::EndsWith => match (left, right) {
                    (Value::String(l), Value::String(r)) => Value::Boolean(l.ends_with(&r)),
                    _ => Value::Null,
                },
                BinaryOperator::Contains => match (left, right) {
                    (Value::String(l), Value::String(r)) => Value::Boolean(l.contains(&r)),
                    _ => Value::Null,
                },
                _ => Value::Null,
            }
        }
        Expression::Unary(u) => {
            let arg = evaluate_expression_value_streaming(&u.argument, record, ctx);
            match u.operator {
                crate::query::ast::UnaryOperator::Not => match arg {
                    Value::Boolean(b) => Value::Boolean(!b),
                    _ => Value::Null,
                },
                crate::query::ast::UnaryOperator::Negate => match arg {
                    Value::Float(f) => Value::Float(-f),
                    _ => Value::Null,
                },
            }
        }
        Expression::Case(case_expr) => {
            for alt in &case_expr.alternatives {
                if evaluate_expression_streaming(&alt.when, record, ctx) {
                    return evaluate_expression_value_streaming(&alt.then, record, ctx);
                }
            }
            match &case_expr.else_expression {
                Some(expr) => evaluate_expression_value_streaming(expr, record, ctx),
                None => Value::Null,
            }
        }
        Expression::Exists(exists_expr) => {
            Value::Boolean(evaluate_exists_streaming(exists_expr.as_ref(), record, ctx))
        }
        Expression::List(elements) => list_literal_value_streaming(elements, record, ctx),
        Expression::ListComprehension(comp) => {
            list_comprehension_value_streaming(comp.as_ref(), record, ctx)
        }
        Expression::FunctionCall(func) => match func.name.to_lowercase().as_str() {
            "vec_similarity" => {
                let Some(left) = func
                    .arguments
                    .first()
                    .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
                else {
                    return Value::Null;
                };
                let Some(right) = func
                    .arguments
                    .get(1)
                    .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
                else {
                    return Value::Null;
                };
                let Some(left_vec) = value_to_vector(&left) else {
                    return Value::Null;
                };
                let Some(right_vec) = value_to_vector(&right) else {
                    return Value::Null;
                };
                let Some(sim) = cosine_similarity(&left_vec, &right_vec) else {
                    return Value::Null;
                };
                Value::Float(sim as f64)
            }
            "txt_score" => {
                #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
                {
                    let Some(Expression::PropertyAccess(pa)) = func.arguments.first() else {
                        return Value::Null;
                    };
                    let Some(Value::Node(node_id)) = record.get(&pa.variable) else {
                        return Value::Null;
                    };
                    let Some(query_expr) = func.arguments.get(1) else {
                        return Value::Null;
                    };
                    let Value::String(query) =
                        evaluate_expression_value_streaming(query_expr, record, ctx)
                    else {
                        return Value::Null;
                    };
                    Value::Float(ctx.db.fts_txt_score(*node_id, &pa.property, &query))
                }
                #[cfg(not(all(feature = "fts", not(target_arch = "wasm32"))))]
                {
                    Value::Float(0.0)
                }
            }
            "id" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
            {
                Some(Value::Node(id)) => Value::Float(id as f64),
                _ => Value::Null,
            },
            "type" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
            {
                Some(Value::Relationship(triple)) => relationship_type_value(&ctx.db, &triple),
                _ => Value::Null,
            },
            "labels" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
            {
                Some(Value::Node(id)) => node_labels_value(&ctx.db, id),
                _ => Value::Null,
            },
            "keys" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
            {
                Some(Value::Node(id)) => node_property_keys_value(&ctx.db, id),
                Some(Value::Relationship(triple)) => edge_property_keys_value(&ctx.db, &triple),
                _ => Value::Null,
            },
            "size" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
            {
                Some(Value::String(s)) => Value::Float(s.len() as f64),
                _ => Value::Null,
            },
            "toupper" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
            {
                Some(Value::String(s)) => Value::String(s.to_uppercase()),
                _ => Value::Null,
            },
            "tolower" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
            {
                Some(Value::String(s)) => Value::String(s.to_lowercase()),
                _ => Value::Null,
            },
            "trim" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value_streaming(arg, record, ctx))
            {
                Some(Value::String(s)) => Value::String(s.trim().to_string()),
                _ => Value::Null,
            },
            "coalesce" => {
                for arg in &func.arguments {
                    let v = evaluate_expression_value_streaming(arg, record, ctx);
                    if !matches!(v, Value::Null) {
                        return v;
                    }
                }
                Value::Null
            }
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

fn list_literal_value_streaming(
    elements: &[Expression],
    record: &Record,
    ctx: &ArcExecutionContext,
) -> Value {
    let json = serde_json::Value::Array(
        elements
            .iter()
            .map(|e| executor_value_to_json(&evaluate_expression_value_streaming(e, record, ctx)))
            .collect(),
    );
    Value::String(json.to_string())
}

fn list_comprehension_value_streaming(
    comp: &ListComprehension,
    record: &Record,
    ctx: &ArcExecutionContext,
) -> Value {
    let Some(items) = evaluate_list_source_streaming(&comp.list, record, ctx) else {
        return Value::Null;
    };

    let mut out = Vec::new();
    for item in items {
        let mut scoped = record.clone();
        scoped.insert(comp.variable.clone(), item);

        if let Some(where_expr) = &comp.where_expression
            && !evaluate_expression_streaming(where_expr, &scoped, ctx)
        {
            continue;
        }

        let mapped = match &comp.map_expression {
            Some(expr) => evaluate_expression_value_streaming(expr, &scoped, ctx),
            None => scoped.get(&comp.variable).cloned().unwrap_or(Value::Null),
        };
        out.push(executor_value_to_json(&mapped));
    }

    Value::String(serde_json::Value::Array(out).to_string())
}

fn evaluate_list_source_streaming(
    expr: &Expression,
    record: &Record,
    ctx: &ArcExecutionContext,
) -> Option<Vec<Value>> {
    match expr {
        Expression::List(elements) => Some(
            elements
                .iter()
                .map(|e| evaluate_expression_value_streaming(e, record, ctx))
                .collect(),
        ),
        _ => match evaluate_expression_value_streaming(expr, record, ctx) {
            Value::String(s) => parse_executor_list_string(&s),
            Value::Vector(v) => Some(v.iter().map(|f| Value::Float(*f as f64)).collect()),
            _ => None,
        },
    }
}

fn executor_value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Vector(v) => serde_json::Value::Array(
            v.iter()
                .map(|f| {
                    serde_json::Number::from_f64(*f as f64)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                })
                .collect(),
        ),
        Value::Node(id) => serde_json::Value::Number(serde_json::Number::from(*id)),
        Value::Relationship(triple) => serde_json::Value::String(format!("{triple:?}")),
    }
}

fn json_to_executor_value(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Boolean(*b),
        serde_json::Value::Number(n) => Value::Float(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::String(s.clone()),
        _ => Value::Null,
    }
}

fn parse_executor_list_string(input: &str) -> Option<Vec<Value>> {
    let json = serde_json::from_str::<serde_json::Value>(input).ok()?;
    let serde_json::Value::Array(items) = json else {
        return None;
    };
    Some(items.iter().map(json_to_executor_value).collect())
}

fn parse_executor_vector_string(input: &str) -> Option<Vec<f32>> {
    let json = serde_json::from_str::<serde_json::Value>(input).ok()?;
    let serde_json::Value::Array(items) = json else {
        return None;
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(item.as_f64()? as f32);
    }
    Some(out)
}

fn value_to_vector(value: &Value) -> Option<Vec<f32>> {
    match value {
        Value::Vector(v) => Some(v.clone()),
        Value::String(s) => parse_executor_vector_string(s),
        _ => None,
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        return None;
    }
    Some(dot / denom)
}

fn value_in_list(needle: &Value, haystack: &Value) -> Value {
    match haystack {
        Value::String(input) => {
            let Some(items) = parse_executor_list_string(input) else {
                return Value::Null;
            };
            Value::Boolean(items.iter().any(|v| v == needle))
        }
        Value::Vector(items) => match needle {
            Value::Float(f) => Value::Boolean(items.iter().any(|v| (*v as f64) == *f)),
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

fn unwind_value_to_list(value: Value) -> Result<Vec<Value>, Error> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::String(input) => parse_executor_list_string(&input)
            .ok_or_else(|| Error::Other("UNWIND expects a list expression".to_string())),
        Value::Vector(items) => Ok(items.into_iter().map(|f| Value::Float(f as f64)).collect()),
        _ => Err(Error::Other("UNWIND expects a list expression".to_string())),
    }
}

fn unwind_values_streaming(
    expr: &Expression,
    record: &Record,
    ctx: &ArcExecutionContext,
) -> Result<Vec<Value>, Error> {
    match expr {
        Expression::List(elements) => Ok(elements
            .iter()
            .map(|e| evaluate_expression_value_streaming(e, record, ctx))
            .collect()),
        _ => unwind_value_to_list(evaluate_expression_value_streaming(expr, record, ctx)),
    }
}

fn unwind_values(
    expr: &Expression,
    record: &Record,
    ctx: &ExecutionContext,
) -> Result<Vec<Value>, Error> {
    match expr {
        Expression::List(elements) => Ok(elements
            .iter()
            .map(|e| evaluate_expression_value(e, record, ctx))
            .collect()),
        _ => unwind_value_to_list(evaluate_expression_value(expr, record, ctx)),
    }
}

fn evaluate_exists_streaming(
    exists_expr: &ExistsExpression,
    record: &Record,
    ctx: &ArcExecutionContext,
) -> bool {
    match exists_expr {
        ExistsExpression::Pattern(pattern) => {
            exists_match_pattern_streaming(pattern, None, record, ctx)
        }
        ExistsExpression::Subquery(query) => {
            let (pattern, where_expr) = match extract_exists_match_query(query) {
                Some(v) => v,
                None => return false,
            };
            exists_match_pattern_streaming(pattern, where_expr, record, ctx)
        }
    }
}

fn exists_match_pattern_streaming(
    pattern: &Pattern,
    where_expr: Option<&Expression>,
    outer_record: &Record,
    ctx: &ArcExecutionContext,
) -> bool {
    let Some(PathElement::Node(start_node)) = pattern.elements.first() else {
        return false;
    };

    if let Some(var) = &start_node.variable
        && let Some(Value::Node(start_id)) = outer_record.get(var)
    {
        if !node_satisfies_streaming(*start_id, start_node, outer_record, ctx) {
            return false;
        }
        return exists_path_from_node_streaming(
            pattern,
            0,
            *start_id,
            outer_record,
            where_expr,
            ctx,
        );
    }

    // Fallback: evaluate as an uncorrelated MATCH query and check if any result exists.
    exists_uncorrelated_match_streaming(pattern, where_expr, ctx)
}

fn exists_uncorrelated_match_streaming(
    pattern: &Pattern,
    where_expr: Option<&Expression>,
    ctx: &ArcExecutionContext,
) -> bool {
    use crate::query::ast::{MatchClause, Query, ReturnClause, ReturnItem, WhereClause};
    use crate::query::planner::QueryPlanner;

    let mut clauses: Vec<Clause> = Vec::new();
    clauses.push(Clause::Match(MatchClause {
        optional: false,
        pattern: pattern.clone(),
    }));
    if let Some(expr) = where_expr.cloned() {
        clauses.push(Clause::Where(WhereClause { expression: expr }));
    }
    clauses.push(Clause::Return(ReturnClause {
        distinct: false,
        items: vec![ReturnItem {
            expression: Expression::Literal(Literal::Integer(1)),
            alias: Some("_exists".to_string()),
        }],
        order_by: None,
        limit: Some(1),
        skip: None,
    }));

    let planner = QueryPlanner::new();
    let plan = match planner.plan(Query { clauses }) {
        Ok(plan) => plan,
        Err(_) => return false,
    };

    let exec_ctx = ExecutionContext {
        db: ctx.db.as_ref(),
        params: ctx.params.as_ref(),
    };
    match plan.execute(&exec_ctx) {
        Ok(mut iter) => iter.next().is_some(),
        Err(_) => false,
    }
}

fn exists_path_from_node_streaming(
    pattern: &Pattern,
    node_index: usize,
    current_node_id: u64,
    bindings: &Record,
    where_expr: Option<&Expression>,
    ctx: &ArcExecutionContext,
) -> bool {
    let next_rel_index = node_index + 1;
    let next_node_index = node_index + 2;

    if next_node_index >= pattern.elements.len() {
        return where_expr.is_none_or(|expr| evaluate_expression_streaming(expr, bindings, ctx));
    }

    let PathElement::Relationship(rel) = &pattern.elements[next_rel_index] else {
        return false;
    };
    let PathElement::Node(next_node) = &pattern.elements[next_node_index] else {
        return false;
    };

    if rel.variable_length.is_some() {
        return exists_var_length_step_streaming(
            pattern,
            next_node_index,
            current_node_id,
            rel,
            bindings,
            where_expr,
            ctx,
        );
    }

    for (triple, end_id) in iter_matching_edges(ctx.db.as_ref(), current_node_id, rel) {
        let mut new_record = bindings.clone();

        if let Some(rel_var) = &rel.variable {
            match new_record.get(rel_var) {
                Some(Value::Relationship(existing)) if existing == &triple => {}
                Some(_) => continue,
                None => new_record.insert(rel_var.clone(), Value::Relationship(triple)),
            }
        }

        if let Some(props) = &rel.properties
            && !edge_satisfies_streaming(&triple, props, &new_record, ctx)
        {
            continue;
        }

        if !node_satisfies_streaming(end_id, next_node, &new_record, ctx) {
            continue;
        }

        if let Some(node_var) = &next_node.variable {
            match new_record.get(node_var) {
                Some(Value::Node(existing)) if *existing == end_id => {}
                Some(_) => continue,
                None => new_record.insert(node_var.clone(), Value::Node(end_id)),
            }
        }

        if exists_path_from_node_streaming(
            pattern,
            next_node_index,
            end_id,
            &new_record,
            where_expr,
            ctx,
        ) {
            return true;
        }
    }

    false
}

fn exists_var_length_step_streaming(
    pattern: &Pattern,
    next_node_index: usize,
    current_node_id: u64,
    rel: &RelationshipPattern,
    bindings: &Record,
    where_expr: Option<&Expression>,
    ctx: &ArcExecutionContext,
) -> bool {
    let PathElement::Node(next_node) = &pattern.elements[next_node_index] else {
        return false;
    };
    if rel.variable.is_some() || rel.properties.is_some() {
        return false;
    }
    let Some(var_len) = &rel.variable_length else {
        return false;
    };
    let min_hops = var_len.min.unwrap_or(1);
    let Some(max_hops) = var_len.max else {
        return false;
    };

    if rel.types.len() > 1 {
        return false;
    }

    let rel_predicate_id = rel
        .types
        .first()
        .and_then(|t| ctx.db.resolve_id(t).ok().flatten());

    let reachable = find_reachable_nodes(
        ctx.db.as_ref(),
        current_node_id,
        rel.direction.clone(),
        rel_predicate_id,
        min_hops,
        max_hops,
    );

    for end_id in reachable {
        let mut new_record = bindings.clone();

        if !node_satisfies_streaming(end_id, next_node, &new_record, ctx) {
            continue;
        }

        if let Some(node_var) = &next_node.variable {
            match new_record.get(node_var) {
                Some(Value::Node(existing)) if *existing == end_id => {}
                Some(_) => continue,
                None => new_record.insert(node_var.clone(), Value::Node(end_id)),
            }
        }

        if exists_path_from_node_streaming(
            pattern,
            next_node_index,
            end_id,
            &new_record,
            where_expr,
            ctx,
        ) {
            return true;
        }
    }

    false
}

fn node_satisfies_streaming(
    node_id: u64,
    node: &crate::query::ast::NodePattern,
    bindings: &Record,
    ctx: &ArcExecutionContext,
) -> bool {
    if let Some(var) = &node.variable
        && let Some(Value::Node(bound)) = bindings.get(var)
        && *bound != node_id
    {
        return false;
    }

    if !node.labels.is_empty() {
        let Some(type_id) = ctx.db.resolve_id("type").ok().flatten() else {
            return false;
        };
        for label in &node.labels {
            let Some(label_id) = ctx.db.resolve_id(label).ok().flatten() else {
                return false;
            };
            let criteria = QueryCriteria {
                subject_id: Some(node_id),
                predicate_id: Some(type_id),
                object_id: Some(label_id),
            };
            if ctx.db.query(criteria).next().is_none() {
                return false;
            }
        }
    }

    if let Some(props) = &node.properties
        && !node_properties_match_streaming(node_id, props, bindings, ctx)
    {
        return false;
    }

    true
}

fn node_properties_match_streaming(
    node_id: u64,
    props: &PropertyMap,
    bindings: &Record,
    ctx: &ArcExecutionContext,
) -> bool {
    if props.properties.is_empty() {
        return true;
    }
    let Ok(Some(binary)) = ctx.db.get_node_property_binary(node_id) else {
        return false;
    };
    let Ok(stored) = crate::storage::property::deserialize_properties(&binary) else {
        return false;
    };

    for pair in &props.properties {
        let expected = evaluate_expression_value_streaming(&pair.value, bindings, ctx);
        let Some(actual) = stored.get(&pair.key) else {
            return false;
        };
        if !json_value_matches_executor_value(actual, &expected) {
            return false;
        }
    }

    true
}

fn edge_satisfies_streaming(
    triple: &Triple,
    props: &PropertyMap,
    bindings: &Record,
    ctx: &ArcExecutionContext,
) -> bool {
    if props.properties.is_empty() {
        return true;
    }
    let Ok(Some(binary)) =
        ctx.db
            .get_edge_property_binary(triple.subject_id, triple.predicate_id, triple.object_id)
    else {
        return false;
    };
    let Ok(stored) = crate::storage::property::deserialize_properties(&binary) else {
        return false;
    };

    for pair in &props.properties {
        let expected = evaluate_expression_value_streaming(&pair.value, bindings, ctx);
        let Some(actual) = stored.get(&pair.key) else {
            return false;
        };
        if !json_value_matches_executor_value(actual, &expected) {
            return false;
        }
    }

    true
}

fn json_value_matches_executor_value(actual: &serde_json::Value, expected: &Value) -> bool {
    match expected {
        Value::Null => actual.is_null(),
        Value::String(s) => actual.as_str() == Some(s.as_str()),
        Value::Boolean(b) => actual.as_bool() == Some(*b),
        Value::Float(f) => actual.as_f64().map(|v| v == *f).unwrap_or(false),
        _ => false,
    }
}

fn iter_matching_edges(
    db: &Database,
    node_id: u64,
    rel: &RelationshipPattern,
) -> Vec<(Triple, u64)> {
    let predicate_ids: Vec<u64> = rel
        .types
        .iter()
        .filter_map(|t| db.resolve_id(t).ok().flatten())
        .collect();

    let predicate_ids: Vec<Option<u64>> = if predicate_ids.is_empty() && !rel.types.is_empty() {
        // Types were specified but none resolved -> no matches.
        return Vec::new();
    } else if predicate_ids.is_empty() {
        vec![None]
    } else {
        predicate_ids.into_iter().map(Some).collect()
    };

    let mut out = Vec::new();

    for pred in predicate_ids {
        match rel.direction {
            RelationshipDirection::LeftToRight => {
                let criteria = QueryCriteria {
                    subject_id: Some(node_id),
                    predicate_id: pred,
                    object_id: None,
                };
                out.extend(db.query(criteria).map(|t| (t, t.object_id)));
            }
            RelationshipDirection::RightToLeft => {
                let criteria = QueryCriteria {
                    subject_id: None,
                    predicate_id: pred,
                    object_id: Some(node_id),
                };
                out.extend(db.query(criteria).map(|t| (t, t.subject_id)));
            }
            RelationshipDirection::Undirected => {
                let out_criteria = QueryCriteria {
                    subject_id: Some(node_id),
                    predicate_id: pred,
                    object_id: None,
                };
                out.extend(db.query(out_criteria).map(|t| (t, t.object_id)));

                let in_criteria = QueryCriteria {
                    subject_id: None,
                    predicate_id: pred,
                    object_id: Some(node_id),
                };
                out.extend(db.query(in_criteria).map(|t| (t, t.subject_id)));
            }
        }
    }

    out
}

fn extract_exists_match_query(
    query: &crate::query::ast::Query,
) -> Option<(&Pattern, Option<&Expression>)> {
    use crate::query::ast::{Clause, WhereClause};

    let mut match_pattern: Option<&Pattern> = None;
    let mut where_expr: Option<&Expression> = None;

    for clause in &query.clauses {
        match clause {
            Clause::Match(m) => match_pattern = Some(&m.pattern),
            Clause::Where(WhereClause { expression }) => where_expr = Some(expression),
            Clause::Return(_) => {}
            _ => return None,
        }
    }

    match_pattern.map(|p| (p, where_expr))
}
fn json_array_string(mut values: Vec<String>) -> Value {
    values.sort();
    let json =
        serde_json::Value::Array(values.into_iter().map(serde_json::Value::String).collect());
    Value::String(json.to_string())
}

fn relationship_type_value(db: &Database, triple: &Triple) -> Value {
    match db.resolve_str(triple.predicate_id).ok().flatten() {
        Some(s) => Value::String(s),
        None => Value::Null,
    }
}

fn node_labels_value(db: &Database, node_id: u64) -> Value {
    let Some(type_id) = db.resolve_id("type").ok().flatten() else {
        return Value::String("[]".to_string());
    };

    let criteria = QueryCriteria {
        subject_id: Some(node_id),
        predicate_id: Some(type_id),
        object_id: None,
    };

    let labels: Vec<String> = db
        .query(criteria)
        .filter_map(|t| db.resolve_str(t.object_id).ok().flatten())
        .collect();

    json_array_string(labels)
}

fn node_property_keys_value(db: &Database, node_id: u64) -> Value {
    let keys: Vec<String> = db
        .get_node_property_binary(node_id)
        .ok()
        .flatten()
        .and_then(|binary| crate::storage::property::deserialize_properties(&binary).ok())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default();

    json_array_string(keys)
}

fn edge_property_keys_value(db: &Database, triple: &Triple) -> Value {
    let keys: Vec<String> = db
        .get_edge_property_binary(triple.subject_id, triple.predicate_id, triple.object_id)
        .ok()
        .flatten()
        .and_then(|binary| crate::storage::property::deserialize_properties(&binary).ok())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default();

    json_array_string(keys)
}

// ============================================================================
// Optimized ScanNode Implementation
// ============================================================================

impl ExecutionPlan for SingleRowNode {
    fn execute<'a>(
        &'a self,
        _ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        Ok(Box::new(std::iter::once(Ok(Record::new()))))
    }

    fn estimate_cardinality(&self, _ctx: &ExecutionContext) -> usize {
        1
    }
}

impl ExecutionPlan for DistinctNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let input_iter = self.input.execute(ctx)?;
        let mut seen: HashSet<String> = HashSet::new();
        Ok(Box::new(input_iter.filter_map(
            move |result| match result {
                Ok(record) => {
                    let key = record_distinct_key(&record);
                    if seen.insert(key) {
                        Some(Ok(record))
                    } else {
                        None
                    }
                }
                Err(err) => Some(Err(err)),
            },
        )))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        self.input.estimate_cardinality(ctx)
    }
}

impl ExecutionPlan for UnwindNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let input_iter = self.input.execute(ctx)?;
        let expression = self.expression.clone();
        let alias = self.alias.clone();
        Ok(Box::new(input_iter.flat_map(move |result| {
            match result {
                Ok(record) => match unwind_values(&expression, &record, ctx) {
                    Ok(values) => values
                        .into_iter()
                        .map(|value| {
                            let mut new_record = record.clone();
                            new_record.insert(alias.clone(), value);
                            Ok(new_record)
                        })
                        .collect::<Vec<_>>(),
                    Err(err) => vec![Err(err)],
                },
                Err(err) => vec![Err(err)],
            }
        })))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        self.input.estimate_cardinality(ctx)
    }
}

impl ExecutionPlan for ScanNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let alias = self.alias.clone();

        if self.labels.is_empty() {
            // 无标签扫描：使用优化的节点发现算法
            Ok(Box::new(scan_all_nodes_optimized(ctx.db, alias)))
        } else {
            // 标签扫描：利用 (?, type, Label) 索引
            Ok(Box::new(scan_labeled_nodes_optimized(
                ctx.db,
                &self.labels,
                alias,
            )))
        }
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        ScanStats::estimate_scan_cardinality(ctx.db, &self.labels).estimated_cardinality
    }
}

impl ExecutionPlan for FtsCandidateScanNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let Some(query) = resolve_query_string(&self.query, ctx) else {
            return Ok(Box::new(std::iter::empty()));
        };
        if query.is_empty() || self.property.is_empty() {
            return Ok(Box::new(std::iter::empty()));
        }

        #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
        let Some(scores) = ctx
            .db
            .fts_scores_for_query(self.property.as_str(), query.as_str())
        else {
            return Ok(Box::new(std::iter::empty()));
        };
        #[cfg(not(all(feature = "fts", not(target_arch = "wasm32"))))]
        let scores: std::sync::Arc<std::collections::HashMap<u64, f32>> =
            std::sync::Arc::new(std::collections::HashMap::new());

        let alias = self.alias.clone();
        let db = ctx.db;
        let type_and_labels = resolve_label_ids(db, &self.labels);
        let candidate_ids: Vec<u64> = scores.keys().copied().collect();

        Ok(Box::new(candidate_ids.into_iter().filter_map(
            move |node_id| {
                if let Some((type_id, label_ids)) = type_and_labels.as_ref()
                    && !node_has_labels(db, node_id, *type_id, label_ids)
                {
                    return None;
                }

                let mut record = Record::new();
                record.insert(alias.clone(), Value::Node(node_id));
                Some(Ok(record))
            },
        )))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        let scan_est = ScanStats::estimate_scan_cardinality(ctx.db, &self.labels)
            .estimated_cardinality
            .max(1);
        scan_est.min(10_000)
    }
}

impl ExecutionPlan for VectorTopKScanNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        fn fallback<'a>(
            node: &VectorTopKScanNode,
            ctx: &'a ExecutionContext<'a>,
            limit: usize,
        ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
            let alias = node.alias.clone();
            let property = node.property.clone();
            let query_expr = node.query.clone();

            let sort_expr = Expression::FunctionCall(FunctionCall {
                name: "vec_similarity".to_string(),
                arguments: vec![
                    Expression::PropertyAccess(PropertyAccess {
                        variable: alias.clone(),
                        property,
                    }),
                    query_expr,
                ],
            });

            let mut records = Vec::new();
            if node.labels.is_empty() {
                for item in scan_all_nodes_optimized(ctx.db, alias.clone()) {
                    records.push(item?);
                }
            } else {
                let labels = node.labels.clone();
                for item in scan_labeled_nodes_optimized(ctx.db, &labels, alias.clone()) {
                    records.push(item?);
                }
            }
            records.sort_by(|a, b| {
                let val_a = evaluate_expression_value(&sort_expr, a, ctx);
                let val_b = evaluate_expression_value(&sort_expr, b, ctx);
                compare_values_for_sort(&val_a, &val_b, &Direction::Descending)
            });
            records.truncate(limit);
            Ok(Box::new(records.into_iter().map(Ok)))
        }

        let limit = usize::try_from(self.limit).unwrap_or(usize::MAX);
        if limit == 0 || self.property.is_empty() {
            return Ok(Box::new(std::iter::empty()));
        }

        // Only optimize global Top-K for now. Label filtering stays on the exact path.
        if !self.labels.is_empty() {
            return fallback(self, ctx, limit);
        }

        #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
        {
            let query_value = evaluate_expression_value(&self.query, &Record::new(), ctx);
            let Some(query_vec) = value_to_vector(&query_value) else {
                return fallback(self, ctx, limit);
            };

            let Some(config) = ctx.db.vector_index_config() else {
                return fallback(self, ctx, limit);
            };

            let metric = config.metric.to_lowercase();
            if config.property != self.property
                || query_vec.len() != config.dim
                || !(metric == "cosine" || metric == "cos")
            {
                return fallback(self, ctx, limit);
            }

            let hits = match ctx.db.vector_search(&query_vec, limit) {
                Ok(h) => h,
                Err(_) => return fallback(self, ctx, limit),
            };

            // Re-score candidates using the exact `vec_similarity` semantics to keep ordering
            // consistent with the executor.
            let alias = self.alias.clone();
            let property_expr = Expression::PropertyAccess(PropertyAccess {
                variable: alias.clone(),
                property: self.property.clone(),
            });
            let mut scored: Vec<(u64, Option<f32>)> = Vec::with_capacity(hits.len());
            for (node_id, _) in hits {
                let mut record = Record::new();
                record.insert(alias.clone(), Value::Node(node_id));

                let value = evaluate_expression_value(&property_expr, &record, ctx);
                let score = value_to_vector(&value).and_then(|v| cosine_similarity(&v, &query_vec));
                scored.push((node_id, score));
            }

            scored.sort_by(|(id_a, s_a), (id_b, s_b)| match (s_a, s_b) {
                (Some(a), Some(b)) => b
                    .partial_cmp(a)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| id_a.cmp(id_b)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => id_a.cmp(id_b),
            });

            let candidate_ids: Vec<u64> = scored.into_iter().map(|(id, _)| id).collect();
            return Ok(Box::new(candidate_ids.into_iter().map(move |node_id| {
                let mut record = Record::new();
                record.insert(alias.clone(), Value::Node(node_id));
                Ok(record)
            })));
        }
        #[cfg(not(all(feature = "vector", not(target_arch = "wasm32"))))]
        {
            fallback(self, ctx, limit)
        }
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        let scan_est = ScanStats::estimate_scan_cardinality(ctx.db, &self.labels)
            .estimated_cardinality
            .max(1);
        scan_est.min(self.limit as usize)
    }
}

/// 优化的全节点扫描 - 避免全表扫描
fn scan_all_nodes_optimized(
    db: &Database,
    alias: String,
) -> impl Iterator<Item = Result<Record, Error>> + '_ {
    // 策略：使用 SPO 索引扫描，提取唯一的 subject 节点
    // 然后使用 OSP 索引扫描，提取唯一的 object 节点
    // 合并去重

    let mut unique_nodes = HashSet::new();

    // 扫描所有三元组的 subjects
    let subject_criteria = QueryCriteria::default();
    for triple in db.query(subject_criteria).take(10000) {
        // 限制扫描量
        unique_nodes.insert(triple.subject_id);
        unique_nodes.insert(triple.object_id);
    }

    unique_nodes.into_iter().map(move |node_id| {
        let mut record = Record::new();
        record.insert(alias.clone(), Value::Node(node_id));
        Ok(record)
    })
}

/// 优化的标签节点扫描 - 使用类型索引
fn scan_labeled_nodes_optimized<'a>(
    db: &'a Database,
    labels: &'a [String],
    alias: String,
) -> impl Iterator<Item = Result<Record, Error>> + 'a {
    // 解析 "type" 谓词 ID
    let type_id = match db.resolve_id("type") {
        Ok(Some(id)) => id,
        _ => {
            return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = Result<Record, Error>>>;
        }
    };

    let labels = labels.to_vec(); // Clone for move

    Box::new(std::iter::once(()).flat_map(move |_| {
        let mut label_node_sets: Vec<HashSet<u64>> = Vec::new();

        // 为每个标签收集节点
        for label in &labels {
            if let Ok(Some(label_id)) = db.resolve_id(label) {
                let criteria = QueryCriteria {
                    subject_id: None,
                    predicate_id: Some(type_id),
                    object_id: Some(label_id),
                };

                let nodes: HashSet<u64> =
                    db.query(criteria).map(|triple| triple.subject_id).collect();
                label_node_sets.push(nodes);
            } else {
                // 标签不存在，返回空集合
                label_node_sets.push(HashSet::new());
            }
        }

        // 计算标签交集（节点必须有所有指定标签）
        let final_nodes = if label_node_sets.is_empty() {
            HashSet::new()
        } else {
            label_node_sets
                .into_iter()
                .reduce(|acc, set| acc.intersection(&set).cloned().collect())
                .unwrap_or_default()
        };

        let alias_clone = alias.clone();
        final_nodes.into_iter().map(move |node_id| {
            let mut record = Record::new();
            record.insert(alias_clone.clone(), Value::Node(node_id));
            Ok(record)
        })
    }))
}

fn resolve_query_string(expr: &Expression, ctx: &ExecutionContext) -> Option<String> {
    match expr {
        Expression::Literal(Literal::String(s)) => Some(s.clone()),
        Expression::Parameter(name) => match ctx.params.get(name) {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn resolve_label_ids(db: &Database, labels: &[String]) -> Option<(u64, Vec<u64>)> {
    if labels.is_empty() {
        return None;
    }
    let type_id = db.resolve_id("type").ok().flatten()?;
    let mut label_ids = Vec::with_capacity(labels.len());
    for label in labels {
        label_ids.push(db.resolve_id(label).ok().flatten()?);
    }
    Some((type_id, label_ids))
}

fn node_has_labels(db: &Database, node_id: u64, type_id: u64, label_ids: &[u64]) -> bool {
    for label_id in label_ids {
        let criteria = QueryCriteria {
            subject_id: Some(node_id),
            predicate_id: Some(type_id),
            object_id: Some(*label_id),
        };
        if db.query(criteria).next().is_none() {
            return false;
        }
    }
    true
}

#[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
fn resolve_query_string_streaming(expr: &Expression, ctx: &ArcExecutionContext) -> Option<String> {
    match expr {
        Expression::Literal(Literal::String(s)) => Some(s.clone()),
        Expression::Parameter(name) => match ctx.params.get(name) {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
fn resolve_label_ids_streaming(db: &Arc<Database>, labels: &[String]) -> Option<(u64, Vec<u64>)> {
    if labels.is_empty() {
        return None;
    }
    let type_id = db.resolve_id("type").ok().flatten()?;
    let mut label_ids = Vec::with_capacity(labels.len());
    for label in labels {
        label_ids.push(db.resolve_id(label).ok().flatten()?);
    }
    Some((type_id, label_ids))
}

#[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
fn node_has_labels_streaming(
    db: &Arc<Database>,
    node_id: u64,
    type_id: u64,
    label_ids: &[u64],
) -> bool {
    for label_id in label_ids {
        let criteria = QueryCriteria {
            subject_id: Some(node_id),
            predicate_id: Some(type_id),
            object_id: Some(*label_id),
        };
        if db.query(criteria).next().is_none() {
            return false;
        }
    }
    true
}

// ============================================================================
// Index Nested Loop Join Implementation
// ============================================================================

impl ExecutionPlan for NestedLoopJoinNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        // 选择更优的 Join 顺序
        let left_card = self.left.estimate_cardinality(ctx);
        let right_card = self.right.estimate_cardinality(ctx);

        if left_card <= right_card {
            // 左侧较小，使用 left -> right 顺序
            Ok(Box::new(IndexNestedLoopJoinIter::new(
                self.left.execute(ctx)?,
                &self.right,
                ctx,
            )))
        } else {
            // 右侧较小，交换顺序
            Ok(Box::new(IndexNestedLoopJoinIter::new(
                self.right.execute(ctx)?,
                &self.left,
                ctx,
            )))
        }
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        // Join 基数 = 左侧基数 * 右侧基数 * 选择性
        // 假设选择性为 0.1（保守估计）
        let left_card = self.left.estimate_cardinality(ctx);
        let right_card = self.right.estimate_cardinality(ctx);
        (left_card * right_card / 10).max(1)
    }
}

/// Index Nested Loop Join 迭代器 - 避免内存爆炸
struct IndexNestedLoopJoinIter<'a> {
    outer_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'a>,
    inner_plan: &'a PhysicalPlan,
    ctx: &'a ExecutionContext<'a>,
    current_outer: Option<Record>,
    current_inner: Option<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>>,
}

impl<'a> IndexNestedLoopJoinIter<'a> {
    fn new(
        outer_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'a>,
        inner_plan: &'a PhysicalPlan,
        ctx: &'a ExecutionContext<'a>,
    ) -> Self {
        Self {
            outer_iter,
            inner_plan,
            ctx,
            current_outer: None,
            current_inner: None,
        }
    }
}

impl<'a> Iterator for IndexNestedLoopJoinIter<'a> {
    type Item = Result<Record, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // 如果有当前内层迭代器，尝试获取下一个内层记录
            if let Some(ref mut inner_iter) = self.current_inner {
                if let Some(inner_result) = inner_iter.next() {
                    match inner_result {
                        Ok(inner_record) => {
                            if let Some(ref outer_record) = self.current_outer {
                                let mut joined = outer_record.clone();
                                joined.merge(&inner_record);
                                return Some(Ok(joined));
                            }
                        }
                        Err(e) => return Some(Err(e)),
                    }
                } else {
                    // 内层迭代器耗尽，清除状态
                    self.current_inner = None;
                    self.current_outer = None;
                }
            }

            // 获取下一个外层记录
            match self.outer_iter.next() {
                Some(Ok(outer_record)) => {
                    // 为该外层记录创建新的内层迭代器
                    match self.inner_plan.execute(self.ctx) {
                        Ok(inner_iter) => {
                            self.current_outer = Some(outer_record);
                            self.current_inner = Some(inner_iter);
                            // 继续循环以获取第一个 join 结果
                        }
                        Err(e) => return Some(Err(e)),
                    }
                }
                Some(Err(e)) => return Some(Err(e)),
                None => return None, // 外层迭代器耗尽
            }
        }
    }
}

impl ExecutionPlan for LeftOuterJoinNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        Ok(Box::new(LeftOuterJoinIter::new(
            self.left.execute(ctx)?,
            &self.right,
            &self.right_aliases,
            ctx,
        )))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        let left_card = self.left.estimate_cardinality(ctx);
        let right_card = self.right.estimate_cardinality(ctx);
        (left_card * right_card / 10).max(left_card).max(1)
    }
}

struct LeftOuterJoinIter<'a> {
    outer_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'a>,
    inner_plan: &'a PhysicalPlan,
    right_aliases: &'a [String],
    ctx: &'a ExecutionContext<'a>,
    current_outer: Option<Record>,
    current_inner: Option<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>>,
    matched_current_outer: bool,
    emitted_null_current_outer: bool,
}

impl<'a> LeftOuterJoinIter<'a> {
    fn new(
        outer_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'a>,
        inner_plan: &'a PhysicalPlan,
        right_aliases: &'a [String],
        ctx: &'a ExecutionContext<'a>,
    ) -> Self {
        Self {
            outer_iter,
            inner_plan,
            right_aliases,
            ctx,
            current_outer: None,
            current_inner: None,
            matched_current_outer: false,
            emitted_null_current_outer: false,
        }
    }

    fn emit_null_row(&self, mut outer: Record) -> Record {
        for alias in self.right_aliases {
            outer.values.entry(alias.clone()).or_insert(Value::Null);
        }
        outer
    }
}

impl<'a> Iterator for LeftOuterJoinIter<'a> {
    type Item = Result<Record, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut inner_iter) = self.current_inner {
                if let Some(inner_result) = inner_iter.next() {
                    match inner_result {
                        Ok(inner_record) => {
                            let outer_record = self.current_outer.as_ref().unwrap().clone();
                            if let Some(joined) = try_merge_records(outer_record, inner_record) {
                                self.matched_current_outer = true;
                                return Some(Ok(joined));
                            }
                            continue;
                        }
                        Err(e) => return Some(Err(e)),
                    }
                }

                // Inner exhausted; maybe emit NULL row, then advance.
                self.current_inner = None;
                if !self.matched_current_outer && !self.emitted_null_current_outer {
                    self.emitted_null_current_outer = true;
                    let outer_record = self.current_outer.take().unwrap();
                    return Some(Ok(self.emit_null_row(outer_record)));
                }
                self.current_outer = None;
                self.matched_current_outer = false;
                self.emitted_null_current_outer = false;
                continue;
            }

            match self.outer_iter.next()? {
                Ok(outer_record) => match self.inner_plan.execute(self.ctx) {
                    Ok(inner_iter) => {
                        self.current_outer = Some(outer_record);
                        self.current_inner = Some(inner_iter);
                        self.matched_current_outer = false;
                        self.emitted_null_current_outer = false;
                    }
                    Err(e) => return Some(Err(e)),
                },
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

// ============================================================================
// Other ExecutionPlan Implementations
// ============================================================================

impl ExecutionPlan for FilterNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let input_iter = self.input.execute(ctx)?;
        let predicate = self.predicate.clone();

        Ok(Box::new(input_iter.filter_map(move |res| match res {
            Ok(record) => {
                if evaluate_expression(&predicate, &record, ctx) {
                    Some(Ok(record))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        })))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        // 过滤选择性假设为 0.1
        (self.input.estimate_cardinality(ctx) / 10).max(1)
    }
}

impl ExecutionPlan for ProjectNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let input_iter = self.input.execute(ctx)?;
        let projections = self.projections.clone();

        Ok(Box::new(input_iter.map(move |res| match res {
            Ok(record) => {
                let mut new_record = Record::new();
                for (expr, alias) in &projections {
                    let val = evaluate_expression_value(expr, &record, ctx);
                    new_record.insert(alias.clone(), val);
                }
                Ok(new_record)
            }
            Err(e) => Err(e),
        })))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        // 投影不改变基数
        self.input.estimate_cardinality(ctx)
    }
}

/// Streaming variable-length expand iterator
struct StreamingExpandVarLengthIterator {
    input_iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'static>,
    start_node_alias: String,
    end_node_alias: String,
    direction: RelationshipDirection,
    rel_type: Option<String>,
    min_hops: u32,
    max_hops: u32,
    ctx: Arc<ArcExecutionContext>,
    current_record: Option<Record>,
    current_expansions: Option<std::vec::IntoIter<u64>>,
}

impl StreamingExpandVarLengthIterator {
    fn compute_expansions(&self, start_id: u64) -> Vec<u64> {
        let rel_predicate_id: Option<u64> = if let Some(ref rel_type) = self.rel_type {
            self.ctx.db.resolve_id(rel_type).ok().flatten()
        } else {
            None
        };

        find_reachable_nodes(
            &self.ctx.db,
            start_id,
            self.direction.clone(),
            rel_predicate_id,
            self.min_hops,
            self.max_hops,
        )
    }
}

impl Iterator for StreamingExpandVarLengthIterator {
    type Item = Result<Record, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut expansions) = self.current_expansions
                && let Some(end_id) = expansions.next()
            {
                let mut record = self.current_record.as_ref().unwrap().clone();
                record.insert(self.end_node_alias.clone(), Value::Node(end_id));
                return Some(Ok(record));
            }

            self.current_record = None;
            self.current_expansions = None;

            match self.input_iter.next()? {
                Ok(record) => {
                    let Some(Value::Node(start_id)) = record.values.get(&self.start_node_alias)
                    else {
                        continue;
                    };
                    let expansions = self.compute_expansions(*start_id);
                    self.current_record = Some(record);
                    self.current_expansions = Some(expansions.into_iter());
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

impl ExecutionPlan for ExpandNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let input_iter = self.input.execute(ctx)?;
        let start_node_alias = self.start_node_alias.clone();
        let rel_alias = self.rel_alias.clone();
        let end_node_alias = self.end_node_alias.clone();
        let direction = self.direction.clone();
        let db = ctx.db;

        // 解析关系类型
        let rel_predicate_id: Option<u64> = if let Some(ref rel_type) = self.rel_type {
            db.resolve_id(rel_type).ok().flatten()
        } else {
            None
        };

        Ok(Box::new(input_iter.flat_map(
            move |res| -> Box<dyn Iterator<Item = Result<Record, Error>>> {
                match res {
                    Ok(record) => {
                        let start_val = record.get(&start_node_alias);
                        let start_id = match start_val {
                            Some(Value::Node(id)) => *id,
                            _ => return Box::new(std::iter::empty()),
                        };

                        let criteria = match direction {
                            RelationshipDirection::LeftToRight => QueryCriteria {
                                subject_id: Some(start_id),
                                predicate_id: rel_predicate_id,
                                object_id: None,
                            },
                            RelationshipDirection::RightToLeft => QueryCriteria {
                                subject_id: None,
                                predicate_id: rel_predicate_id,
                                object_id: Some(start_id),
                            },
                            RelationshipDirection::Undirected => QueryCriteria {
                                subject_id: Some(start_id),
                                predicate_id: rel_predicate_id,
                                object_id: None,
                            },
                        };

                        let triples = db.query(criteria);
                        let rel_alias = rel_alias.clone();
                        let end_node_alias = end_node_alias.clone();
                        let record = record.clone();
                        let direction = direction.clone();

                        Box::new(triples.map(move |triple| {
                            let mut new_record = record.clone();
                            new_record.insert(rel_alias.clone(), Value::Relationship(triple));

                            let end_id = if direction == RelationshipDirection::RightToLeft {
                                triple.subject_id
                            } else {
                                triple.object_id
                            };
                            new_record.insert(end_node_alias.clone(), Value::Node(end_id));

                            Ok(new_record)
                        }))
                    }
                    Err(e) => Box::new(std::iter::once(Err(e))),
                }
            },
        )))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        // Expand 基数 = 输入基数 * 平均出度
        // 假设平均出度为 3
        self.input.estimate_cardinality(ctx) * 3
    }
}

impl ExecutionPlan for ExpandVarLengthNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let input_iter = self.input.execute(ctx)?;
        let start_node_alias = self.start_node_alias.clone();
        let end_node_alias = self.end_node_alias.clone();
        let direction = self.direction.clone();
        let db = ctx.db;

        let rel_predicate_id: Option<u64> = if let Some(ref rel_type) = self.rel_type {
            db.resolve_id(rel_type).ok().flatten()
        } else {
            None
        };

        let min_hops = self.min_hops;
        let max_hops = self.max_hops;

        Ok(Box::new(input_iter.flat_map(
            move |res| -> Box<dyn Iterator<Item = Result<Record, Error>> + 'a> {
                let record = match res {
                    Ok(record) => record,
                    Err(e) => return Box::new(std::iter::once(Err(e))),
                };

                let Some(Value::Node(start_id)) = record.get(&start_node_alias) else {
                    return Box::new(std::iter::empty());
                };

                let expansions = find_reachable_nodes(
                    db,
                    *start_id,
                    direction.clone(),
                    rel_predicate_id,
                    min_hops,
                    max_hops,
                );

                let end_node_alias = end_node_alias.clone();
                Box::new(expansions.into_iter().map(move |end_id| {
                    let mut new_record = record.clone();
                    new_record.insert(end_node_alias.clone(), Value::Node(end_id));
                    Ok(new_record)
                }))
            },
        )))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        // Rough estimate: average branching factor 3 per hop.
        let hops = usize::try_from(self.max_hops).unwrap_or(1).max(1);
        self.input
            .estimate_cardinality(ctx)
            .saturating_mul(3usize.saturating_pow(hops as u32))
            .max(1)
    }
}

fn find_reachable_nodes(
    db: &Database,
    start_id: u64,
    direction: RelationshipDirection,
    rel_predicate_id: Option<u64>,
    min_hops: u32,
    max_hops: u32,
) -> Vec<u64> {
    let mut results = Vec::new();

    if min_hops == 0 {
        results.push(start_id);
    }

    let mut queue: VecDeque<(u64, u32)> = VecDeque::new();
    let mut visited: HashSet<(u64, u32)> = HashSet::new();
    queue.push_back((start_id, 0));
    visited.insert((start_id, 0));

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_hops {
            continue;
        }

        let next_depth = depth + 1;

        let mut neighbors = Vec::new();
        match direction {
            RelationshipDirection::LeftToRight => {
                let criteria = QueryCriteria {
                    subject_id: Some(node_id),
                    predicate_id: rel_predicate_id,
                    object_id: None,
                };
                neighbors.extend(db.query(criteria).map(|t| t.object_id));
            }
            RelationshipDirection::RightToLeft => {
                let criteria = QueryCriteria {
                    subject_id: None,
                    predicate_id: rel_predicate_id,
                    object_id: Some(node_id),
                };
                neighbors.extend(db.query(criteria).map(|t| t.subject_id));
            }
            RelationshipDirection::Undirected => {
                let out = QueryCriteria {
                    subject_id: Some(node_id),
                    predicate_id: rel_predicate_id,
                    object_id: None,
                };
                neighbors.extend(db.query(out).map(|t| t.object_id));

                let inc = QueryCriteria {
                    subject_id: None,
                    predicate_id: rel_predicate_id,
                    object_id: Some(node_id),
                };
                neighbors.extend(db.query(inc).map(|t| t.subject_id));
            }
        }

        for neighbor in neighbors {
            if visited.insert((neighbor, next_depth)) {
                if next_depth >= min_hops {
                    results.push(neighbor);
                }
                queue.push_back((neighbor, next_depth));
            }
        }
    }

    results
}

// ============================================================================
// Limit
// ============================================================================

impl ExecutionPlan for LimitNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let limit = usize::try_from(self.limit).unwrap_or(usize::MAX);
        let inner = self.input.execute(ctx)?;

        struct LimitIter<I> {
            inner: I,
            remaining: usize,
        }

        impl<I> Iterator for LimitIter<I>
        where
            I: Iterator<Item = Result<Record, Error>>,
        {
            type Item = Result<Record, Error>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.remaining == 0 {
                    return None;
                }
                match self.inner.next()? {
                    Ok(v) => {
                        self.remaining -= 1;
                        Some(Ok(v))
                    }
                    Err(e) => Some(Err(e)),
                }
            }
        }

        Ok(Box::new(LimitIter {
            inner,
            remaining: limit,
        }))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        let inner = self.input.estimate_cardinality(ctx);
        inner.min(self.limit as usize)
    }
}

// ============================================================================
// Skip
// ============================================================================

impl ExecutionPlan for SkipNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        let skip = usize::try_from(self.skip).unwrap_or(0);
        let inner = self.input.execute(ctx)?;
        Ok(Box::new(inner.skip(skip)))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        let inner = self.input.estimate_cardinality(ctx);
        inner.saturating_sub(self.skip as usize)
    }
}

// ============================================================================
// Sort
// ============================================================================

impl ExecutionPlan for SortNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        // Sort requires materialization
        let mut records: Vec<Record> = self.input.execute(ctx)?.filter_map(|r| r.ok()).collect();
        let order_by = &self.order_by;

        records.sort_by(|a, b| {
            for (expr, direction) in order_by {
                let val_a = evaluate_expression_value(expr, a, ctx);
                let val_b = evaluate_expression_value(expr, b, ctx);
                let cmp = compare_values_for_sort(&val_a, &val_b, direction);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            std::cmp::Ordering::Equal
        });

        Ok(Box::new(records.into_iter().map(Ok)))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        self.input.estimate_cardinality(ctx)
    }
}

// ============================================================================
// Aggregate
// ============================================================================

impl ExecutionPlan for AggregateNode {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutionContext<'a>,
    ) -> Result<Box<dyn Iterator<Item = Result<Record, Error>> + 'a>, Error> {
        // Collect all input records
        let records: Vec<Record> = self.input.execute(ctx)?.filter_map(|r| r.ok()).collect();

        // Simple case: no GROUP BY, aggregate all records into one result
        if self.group_by.is_empty() {
            let mut result = Record::new();

            for (agg_func, alias) in &self.aggregations {
                let value = compute_aggregate(agg_func, &records, ctx);
                result.insert(alias.clone(), value);
            }

            return Ok(Box::new(std::iter::once(Ok(result))));
        }

        // GROUP BY case: group records and aggregate each group
        // For simplicity, we'll implement basic grouping
        let mut groups: HashMap<String, Vec<Record>> = HashMap::new();

        for record in records {
            // Create group key from group_by expressions
            let key = self
                .group_by
                .iter()
                .map(|expr| format!("{:?}", evaluate_expression_value(expr, &record, ctx)))
                .collect::<Vec<_>>()
                .join("|");
            groups.entry(key).or_default().push(record);
        }

        let results: Vec<Record> = groups
            .into_values()
            .map(|group_records| {
                let mut result = Record::new();

                // Add group by values from first record
                if let Some(first) = group_records.first() {
                    for expr in &self.group_by {
                        if let Expression::Variable(name) = expr
                            && let Some(val) = first.get(name)
                        {
                            result.insert(name.clone(), val.clone());
                        }
                    }
                }

                // Compute aggregates
                for (agg_func, alias) in &self.aggregations {
                    let value = compute_aggregate(agg_func, &group_records, ctx);
                    result.insert(alias.clone(), value);
                }

                result
            })
            .collect();

        Ok(Box::new(results.into_iter().map(Ok)))
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        if self.group_by.is_empty() {
            1 // Single aggregate result
        } else {
            // Estimate: assume 10% of input are unique groups
            (self.input.estimate_cardinality(ctx) / 10).max(1)
        }
    }
}

/// Compute aggregate function over a set of records
fn compute_aggregate(
    func: &AggregateFunction,
    records: &[Record],
    ctx: &ExecutionContext,
) -> Value {
    match func {
        AggregateFunction::Count(expr) => {
            let count = if let Some(e) = expr {
                records
                    .iter()
                    .filter(|r| !matches!(evaluate_expression_value(e, r, ctx), Value::Null))
                    .count()
            } else {
                records.len() // count(*)
            };
            Value::Float(count as f64)
        }
        AggregateFunction::Sum(expr) => {
            let sum: f64 = records
                .iter()
                .filter_map(|r| {
                    if let Value::Float(f) = evaluate_expression_value(expr, r, ctx) {
                        Some(f)
                    } else {
                        None
                    }
                })
                .sum();
            Value::Float(sum)
        }
        AggregateFunction::Avg(expr) => {
            let values: Vec<f64> = records
                .iter()
                .filter_map(|r| {
                    if let Value::Float(f) = evaluate_expression_value(expr, r, ctx) {
                        Some(f)
                    } else {
                        None
                    }
                })
                .collect();
            if values.is_empty() {
                Value::Null
            } else {
                Value::Float(values.iter().sum::<f64>() / values.len() as f64)
            }
        }
        AggregateFunction::Min(expr) => records
            .iter()
            .map(|r| evaluate_expression_value(expr, r, ctx))
            .filter(|v| !matches!(v, Value::Null))
            .min_by(compare_values)
            .unwrap_or(Value::Null),
        AggregateFunction::Max(expr) => records
            .iter()
            .map(|r| evaluate_expression_value(expr, r, ctx))
            .filter(|v| !matches!(v, Value::Null))
            .max_by(compare_values)
            .unwrap_or(Value::Null),
        AggregateFunction::Collect(expr) => {
            // Collect returns a list - for now we'll represent as a string
            let values: Vec<String> = records
                .iter()
                .map(|r| format!("{:?}", evaluate_expression_value(expr, r, ctx)))
                .collect();
            Value::String(format!("[{}]", values.join(", ")))
        }
    }
}

/// Compare two Values for ordering
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Boolean(x), Value::Boolean(y)) => x.cmp(y),
        (Value::Node(x), Value::Node(y)) => x.cmp(y),
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Greater, // NULL sorts last (ASC)
        (_, Value::Null) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    }
}

fn compare_values_for_sort(a: &Value, b: &Value, direction: &Direction) -> std::cmp::Ordering {
    // Keep NULLs last for both ASC and DESC.
    match (a, b) {
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Greater,
        (_, Value::Null) => std::cmp::Ordering::Less,
        _ => {
            let cmp = compare_values(a, b);
            match direction {
                Direction::Ascending => cmp,
                Direction::Descending => cmp.reverse(),
            }
        }
    }
}

// ============================================================================
// Expression Evaluation (从原 executor.rs 复制)
// ============================================================================

fn evaluate_expression(expr: &Expression, record: &Record, ctx: &ExecutionContext) -> bool {
    match evaluate_expression_value(expr, record, ctx) {
        Value::Boolean(b) => b,
        _ => false,
    }
}

pub fn evaluate_expression_value(
    expr: &Expression,
    record: &Record,
    ctx: &ExecutionContext,
) -> Value {
    match expr {
        Expression::Literal(l) => match l {
            Literal::String(s) => Value::String(s.clone()),
            Literal::Float(f) => Value::Float(*f),
            Literal::Integer(i) => Value::Float(*i as f64),
            Literal::Boolean(b) => Value::Boolean(*b),
            Literal::Null => Value::Null,
        },
        Expression::Variable(name) => record.get(name).cloned().unwrap_or(Value::Null),
        Expression::Parameter(name) => ctx.params.get(name).cloned().unwrap_or(Value::Null),
        Expression::PropertyAccess(pa) => {
            if let Some(Value::Node(node_id)) = record.get(&pa.variable)
                && let Ok(Some(binary)) = ctx.db.get_node_property_binary(*node_id)
                && let Ok(props) = crate::storage::property::deserialize_properties(&binary)
                && let Some(value) = props.get(&pa.property)
            {
                return match value {
                    serde_json::Value::String(s) => Value::String(s.clone()),
                    serde_json::Value::Number(n) => Value::Float(n.as_f64().unwrap_or(0.0)),
                    serde_json::Value::Bool(b) => Value::Boolean(*b),
                    serde_json::Value::Null => Value::Null,
                    serde_json::Value::Array(items) => {
                        let mut out = Vec::with_capacity(items.len());
                        for item in items {
                            let Some(n) = item.as_f64() else {
                                return Value::String(
                                    serde_json::Value::Array(items.clone()).to_string(),
                                );
                            };
                            out.push(n as f32);
                        }
                        Value::Vector(out)
                    }
                    _ => Value::Null,
                };
            }
            Value::Null
        }
        Expression::Binary(b) => {
            let left = evaluate_expression_value(&b.left, record, ctx);
            let right = evaluate_expression_value(&b.right, record, ctx);

            match b.operator {
                BinaryOperator::Equal => Value::Boolean(left == right),
                BinaryOperator::NotEqual => Value::Boolean(left != right),
                BinaryOperator::And => match (left, right) {
                    (Value::Boolean(l), Value::Boolean(r)) => Value::Boolean(l && r),
                    _ => Value::Null,
                },
                BinaryOperator::Or => match (left, right) {
                    (Value::Boolean(l), Value::Boolean(r)) => Value::Boolean(l || r),
                    _ => Value::Null,
                },
                BinaryOperator::LessThan => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l < r),
                    _ => Value::Null,
                },
                BinaryOperator::LessThanOrEqual => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l <= r),
                    _ => Value::Null,
                },
                BinaryOperator::GreaterThan => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l > r),
                    _ => Value::Null,
                },
                BinaryOperator::GreaterThanOrEqual => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Boolean(l >= r),
                    _ => Value::Null,
                },
                BinaryOperator::Add => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Float(l + r),
                    (Value::String(l), Value::String(r)) => Value::String(format!("{}{}", l, r)),
                    _ => Value::Null,
                },
                BinaryOperator::Subtract => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Float(l - r),
                    _ => Value::Null,
                },
                BinaryOperator::Multiply => match (left, right) {
                    (Value::Float(l), Value::Float(r)) => Value::Float(l * r),
                    _ => Value::Null,
                },
                BinaryOperator::Divide => match (left, right) {
                    (Value::Float(l), Value::Float(r)) if r != 0.0 => Value::Float(l / r),
                    _ => Value::Null,
                },
                BinaryOperator::Modulo => match (left, right) {
                    (Value::Float(l), Value::Float(r)) if r != 0.0 => Value::Float(l % r),
                    _ => Value::Null,
                },
                BinaryOperator::In => value_in_list(&left, &right),
                BinaryOperator::NotIn => match value_in_list(&left, &right) {
                    Value::Boolean(b) => Value::Boolean(!b),
                    other => other,
                },
                BinaryOperator::StartsWith => match (left, right) {
                    (Value::String(l), Value::String(r)) => Value::Boolean(l.starts_with(&r)),
                    _ => Value::Null,
                },
                BinaryOperator::EndsWith => match (left, right) {
                    (Value::String(l), Value::String(r)) => Value::Boolean(l.ends_with(&r)),
                    _ => Value::Null,
                },
                BinaryOperator::Contains => match (left, right) {
                    (Value::String(l), Value::String(r)) => Value::Boolean(l.contains(&r)),
                    _ => Value::Null,
                },
                _ => Value::Null,
            }
        }
        Expression::Unary(u) => {
            let arg = evaluate_expression_value(&u.argument, record, ctx);
            match u.operator {
                crate::query::ast::UnaryOperator::Not => match arg {
                    Value::Boolean(b) => Value::Boolean(!b),
                    _ => Value::Null,
                },
                crate::query::ast::UnaryOperator::Negate => match arg {
                    Value::Float(f) => Value::Float(-f),
                    _ => Value::Null,
                },
            }
        }
        Expression::Case(case_expr) => {
            for alt in &case_expr.alternatives {
                if evaluate_expression(&alt.when, record, ctx) {
                    return evaluate_expression_value(&alt.then, record, ctx);
                }
            }
            match &case_expr.else_expression {
                Some(expr) => evaluate_expression_value(expr, record, ctx),
                None => Value::Null,
            }
        }
        Expression::Exists(exists_expr) => {
            Value::Boolean(evaluate_exists(exists_expr.as_ref(), record, ctx))
        }
        Expression::List(elements) => list_literal_value(elements, record, ctx),
        Expression::ListComprehension(comp) => list_comprehension_value(comp.as_ref(), record, ctx),
        Expression::FunctionCall(func) => match func.name.to_lowercase().as_str() {
            "vec_similarity" => {
                let Some(left) = func
                    .arguments
                    .first()
                    .map(|arg| evaluate_expression_value(arg, record, ctx))
                else {
                    return Value::Null;
                };
                let Some(right) = func
                    .arguments
                    .get(1)
                    .map(|arg| evaluate_expression_value(arg, record, ctx))
                else {
                    return Value::Null;
                };
                let Some(left_vec) = value_to_vector(&left) else {
                    return Value::Null;
                };
                let Some(right_vec) = value_to_vector(&right) else {
                    return Value::Null;
                };
                let Some(sim) = cosine_similarity(&left_vec, &right_vec) else {
                    return Value::Null;
                };
                Value::Float(sim as f64)
            }
            "txt_score" => {
                #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
                {
                    let Some(Expression::PropertyAccess(pa)) = func.arguments.first() else {
                        return Value::Null;
                    };
                    let Some(Value::Node(node_id)) = record.get(&pa.variable) else {
                        return Value::Null;
                    };
                    let Some(query_expr) = func.arguments.get(1) else {
                        return Value::Null;
                    };
                    let Value::String(query) = evaluate_expression_value(query_expr, record, ctx)
                    else {
                        return Value::Null;
                    };
                    Value::Float(ctx.db.fts_txt_score(*node_id, &pa.property, &query))
                }
                #[cfg(not(all(feature = "fts", not(target_arch = "wasm32"))))]
                {
                    Value::Float(0.0)
                }
            }
            "id" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value(arg, record, ctx))
            {
                Some(Value::Node(id)) => Value::Float(id as f64),
                _ => Value::Null,
            },
            "type" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value(arg, record, ctx))
            {
                Some(Value::Relationship(triple)) => relationship_type_value(ctx.db, &triple),
                _ => Value::Null,
            },
            "labels" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value(arg, record, ctx))
            {
                Some(Value::Node(id)) => node_labels_value(ctx.db, id),
                _ => Value::Null,
            },
            "keys" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value(arg, record, ctx))
            {
                Some(Value::Node(id)) => node_property_keys_value(ctx.db, id),
                Some(Value::Relationship(triple)) => edge_property_keys_value(ctx.db, &triple),
                _ => Value::Null,
            },
            "size" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value(arg, record, ctx))
            {
                Some(Value::String(s)) => Value::Float(s.len() as f64),
                _ => Value::Null,
            },
            "toupper" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value(arg, record, ctx))
            {
                Some(Value::String(s)) => Value::String(s.to_uppercase()),
                _ => Value::Null,
            },
            "tolower" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value(arg, record, ctx))
            {
                Some(Value::String(s)) => Value::String(s.to_lowercase()),
                _ => Value::Null,
            },
            "trim" => match func
                .arguments
                .first()
                .map(|arg| evaluate_expression_value(arg, record, ctx))
            {
                Some(Value::String(s)) => Value::String(s.trim().to_string()),
                _ => Value::Null,
            },
            "coalesce" => {
                for arg in &func.arguments {
                    let v = evaluate_expression_value(arg, record, ctx);
                    if !matches!(v, Value::Null) {
                        return v;
                    }
                }
                Value::Null
            }
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

fn list_literal_value(elements: &[Expression], record: &Record, ctx: &ExecutionContext) -> Value {
    let json = serde_json::Value::Array(
        elements
            .iter()
            .map(|e| executor_value_to_json(&evaluate_expression_value(e, record, ctx)))
            .collect(),
    );
    Value::String(json.to_string())
}

fn list_comprehension_value(
    comp: &ListComprehension,
    record: &Record,
    ctx: &ExecutionContext,
) -> Value {
    let Some(items) = evaluate_list_source(&comp.list, record, ctx) else {
        return Value::Null;
    };

    let mut out = Vec::new();
    for item in items {
        let mut scoped = record.clone();
        scoped.insert(comp.variable.clone(), item);

        if let Some(where_expr) = &comp.where_expression
            && !evaluate_expression(where_expr, &scoped, ctx)
        {
            continue;
        }

        let mapped = match &comp.map_expression {
            Some(expr) => evaluate_expression_value(expr, &scoped, ctx),
            None => scoped.get(&comp.variable).cloned().unwrap_or(Value::Null),
        };
        out.push(executor_value_to_json(&mapped));
    }

    Value::String(serde_json::Value::Array(out).to_string())
}

fn evaluate_list_source(
    expr: &Expression,
    record: &Record,
    ctx: &ExecutionContext,
) -> Option<Vec<Value>> {
    match expr {
        Expression::List(elements) => Some(
            elements
                .iter()
                .map(|e| evaluate_expression_value(e, record, ctx))
                .collect(),
        ),
        _ => match evaluate_expression_value(expr, record, ctx) {
            Value::String(s) => parse_executor_list_string(&s),
            _ => None,
        },
    }
}
fn evaluate_exists(
    exists_expr: &ExistsExpression,
    record: &Record,
    ctx: &ExecutionContext,
) -> bool {
    match exists_expr {
        ExistsExpression::Pattern(pattern) => exists_match_pattern(pattern, None, record, ctx),
        ExistsExpression::Subquery(query) => {
            let (pattern, where_expr) = match extract_exists_match_query(query) {
                Some(v) => v,
                None => return false,
            };
            exists_match_pattern(pattern, where_expr, record, ctx)
        }
    }
}

fn exists_match_pattern(
    pattern: &Pattern,
    where_expr: Option<&Expression>,
    outer_record: &Record,
    ctx: &ExecutionContext,
) -> bool {
    let Some(PathElement::Node(start_node)) = pattern.elements.first() else {
        return false;
    };

    if let Some(var) = &start_node.variable
        && let Some(Value::Node(start_id)) = outer_record.get(var)
    {
        if !node_satisfies(*start_id, start_node, outer_record, ctx) {
            return false;
        }
        return exists_path_from_node(pattern, 0, *start_id, outer_record, where_expr, ctx);
    }

    exists_uncorrelated_match(pattern, where_expr, ctx)
}

fn exists_uncorrelated_match(
    pattern: &Pattern,
    where_expr: Option<&Expression>,
    ctx: &ExecutionContext,
) -> bool {
    use crate::query::ast::{MatchClause, Query, ReturnClause, ReturnItem, WhereClause};
    use crate::query::planner::QueryPlanner;

    let mut clauses: Vec<Clause> = Vec::new();
    clauses.push(Clause::Match(MatchClause {
        optional: false,
        pattern: pattern.clone(),
    }));
    if let Some(expr) = where_expr.cloned() {
        clauses.push(Clause::Where(WhereClause { expression: expr }));
    }
    clauses.push(Clause::Return(ReturnClause {
        distinct: false,
        items: vec![ReturnItem {
            expression: Expression::Literal(Literal::Integer(1)),
            alias: Some("_exists".to_string()),
        }],
        order_by: None,
        limit: Some(1),
        skip: None,
    }));

    let planner = QueryPlanner::new();
    let plan = match planner.plan(Query { clauses }) {
        Ok(plan) => plan,
        Err(_) => return false,
    };

    match plan.execute(ctx) {
        Ok(mut iter) => iter.next().is_some(),
        Err(_) => false,
    }
}

fn exists_path_from_node(
    pattern: &Pattern,
    node_index: usize,
    current_node_id: u64,
    bindings: &Record,
    where_expr: Option<&Expression>,
    ctx: &ExecutionContext,
) -> bool {
    let next_rel_index = node_index + 1;
    let next_node_index = node_index + 2;

    if next_node_index >= pattern.elements.len() {
        return where_expr.is_none_or(|expr| evaluate_expression(expr, bindings, ctx));
    }

    let PathElement::Relationship(rel) = &pattern.elements[next_rel_index] else {
        return false;
    };
    let PathElement::Node(next_node) = &pattern.elements[next_node_index] else {
        return false;
    };

    if rel.variable_length.is_some() {
        return exists_var_length_step(
            pattern,
            next_node_index,
            current_node_id,
            rel,
            bindings,
            where_expr,
            ctx,
        );
    }

    for (triple, end_id) in iter_matching_edges(ctx.db, current_node_id, rel) {
        let mut new_record = bindings.clone();

        if let Some(rel_var) = &rel.variable {
            match new_record.get(rel_var) {
                Some(Value::Relationship(existing)) if existing == &triple => {}
                Some(_) => continue,
                None => new_record.insert(rel_var.clone(), Value::Relationship(triple)),
            }
        }

        if let Some(props) = &rel.properties
            && !edge_satisfies(&triple, props, &new_record, ctx)
        {
            continue;
        }

        if !node_satisfies(end_id, next_node, &new_record, ctx) {
            continue;
        }

        if let Some(node_var) = &next_node.variable {
            match new_record.get(node_var) {
                Some(Value::Node(existing)) if *existing == end_id => {}
                Some(_) => continue,
                None => new_record.insert(node_var.clone(), Value::Node(end_id)),
            }
        }

        if exists_path_from_node(
            pattern,
            next_node_index,
            end_id,
            &new_record,
            where_expr,
            ctx,
        ) {
            return true;
        }
    }

    false
}

fn exists_var_length_step(
    pattern: &Pattern,
    next_node_index: usize,
    current_node_id: u64,
    rel: &RelationshipPattern,
    bindings: &Record,
    where_expr: Option<&Expression>,
    ctx: &ExecutionContext,
) -> bool {
    let PathElement::Node(next_node) = &pattern.elements[next_node_index] else {
        return false;
    };
    if rel.variable.is_some() || rel.properties.is_some() {
        return false;
    }
    let Some(var_len) = &rel.variable_length else {
        return false;
    };
    let min_hops = var_len.min.unwrap_or(1);
    let Some(max_hops) = var_len.max else {
        return false;
    };

    if rel.types.len() > 1 {
        return false;
    }

    let rel_predicate_id = rel
        .types
        .first()
        .and_then(|t| ctx.db.resolve_id(t).ok().flatten());

    let reachable = find_reachable_nodes(
        ctx.db,
        current_node_id,
        rel.direction.clone(),
        rel_predicate_id,
        min_hops,
        max_hops,
    );

    for end_id in reachable {
        let mut new_record = bindings.clone();

        if !node_satisfies(end_id, next_node, &new_record, ctx) {
            continue;
        }

        if let Some(node_var) = &next_node.variable {
            match new_record.get(node_var) {
                Some(Value::Node(existing)) if *existing == end_id => {}
                Some(_) => continue,
                None => new_record.insert(node_var.clone(), Value::Node(end_id)),
            }
        }

        if exists_path_from_node(
            pattern,
            next_node_index,
            end_id,
            &new_record,
            where_expr,
            ctx,
        ) {
            return true;
        }
    }

    false
}

fn node_satisfies(
    node_id: u64,
    node: &crate::query::ast::NodePattern,
    bindings: &Record,
    ctx: &ExecutionContext,
) -> bool {
    if let Some(var) = &node.variable
        && let Some(Value::Node(bound)) = bindings.get(var)
        && *bound != node_id
    {
        return false;
    }

    if !node.labels.is_empty() {
        let Some(type_id) = ctx.db.resolve_id("type").ok().flatten() else {
            return false;
        };
        for label in &node.labels {
            let Some(label_id) = ctx.db.resolve_id(label).ok().flatten() else {
                return false;
            };
            let criteria = QueryCriteria {
                subject_id: Some(node_id),
                predicate_id: Some(type_id),
                object_id: Some(label_id),
            };
            if ctx.db.query(criteria).next().is_none() {
                return false;
            }
        }
    }

    if let Some(props) = &node.properties
        && !node_properties_match(node_id, props, bindings, ctx)
    {
        return false;
    }

    true
}

fn node_properties_match(
    node_id: u64,
    props: &PropertyMap,
    bindings: &Record,
    ctx: &ExecutionContext,
) -> bool {
    if props.properties.is_empty() {
        return true;
    }
    let Ok(Some(binary)) = ctx.db.get_node_property_binary(node_id) else {
        return false;
    };
    let Ok(stored) = crate::storage::property::deserialize_properties(&binary) else {
        return false;
    };

    for pair in &props.properties {
        let expected = evaluate_expression_value(&pair.value, bindings, ctx);
        let Some(actual) = stored.get(&pair.key) else {
            return false;
        };
        if !json_value_matches_executor_value(actual, &expected) {
            return false;
        }
    }

    true
}

fn edge_satisfies(
    triple: &Triple,
    props: &PropertyMap,
    bindings: &Record,
    ctx: &ExecutionContext,
) -> bool {
    if props.properties.is_empty() {
        return true;
    }
    let Ok(Some(binary)) =
        ctx.db
            .get_edge_property_binary(triple.subject_id, triple.predicate_id, triple.object_id)
    else {
        return false;
    };
    let Ok(stored) = crate::storage::property::deserialize_properties(&binary) else {
        return false;
    };

    for pair in &props.properties {
        let expected = evaluate_expression_value(&pair.value, bindings, ctx);
        let Some(actual) = stored.get(&pair.key) else {
            return false;
        };
        if !json_value_matches_executor_value(actual, &expected) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use tempfile::tempdir;

    #[test]
    fn test_optimized_scan_empty_labels() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.nervus");
        let mut db = Database::open(crate::Options::new(&path)).unwrap();

        // 创建测试数据
        db.add_fact(crate::Fact::new("alice", "knows", "bob"))
            .unwrap();
        db.add_fact(crate::Fact::new("bob", "knows", "charlie"))
            .unwrap();

        let ctx = ExecutionContext {
            db: &db,
            params: &HashMap::new(),
        };

        let scan_node = ScanNode {
            alias: "n".to_string(),
            labels: vec![],
        };

        let results: Vec<_> = scan_node.execute(&ctx).unwrap().collect();

        // 应该找到所有唯一节点：alice, bob, charlie
        assert!(results.len() >= 3);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn test_cardinality_estimation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.nervus");
        let mut db = Database::open(crate::Options::new(&path)).unwrap();

        // 添加标签
        db.add_fact(crate::Fact::new("alice", "type", "Person"))
            .unwrap();
        db.add_fact(crate::Fact::new("bob", "type", "Person"))
            .unwrap();
        db.add_fact(crate::Fact::new("charlie", "type", "Robot"))
            .unwrap();

        let ctx = ExecutionContext {
            db: &db,
            params: &HashMap::new(),
        };

        // 测试无标签扫描的基数估算
        let scan_all = ScanNode {
            alias: "n".to_string(),
            labels: vec![],
        };
        let card_all = scan_all.estimate_cardinality(&ctx);
        assert!(card_all > 0);

        // 测试有标签扫描的基数估算
        let scan_person = ScanNode {
            alias: "p".to_string(),
            labels: vec!["Person".to_string()],
        };
        let card_person = scan_person.estimate_cardinality(&ctx);
        assert!(card_person > 0);

        // 有标签的基数应该 <= 无标签的基数
        println!("card_all = {}, card_person = {}", card_all, card_person);
        assert!(card_person <= card_all);
    }
}
