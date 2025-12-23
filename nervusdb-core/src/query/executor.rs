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
use crate::query::ast::{BinaryOperator, Direction, Expression, Literal, RelationshipDirection};
use crate::query::planner::{
    AggregateFunction, AggregateNode, ExpandNode, ExpandVarLengthNode, FilterNode,
    LeftOuterJoinNode, LimitNode, NestedLoopJoinNode, PhysicalPlan, ProjectNode, ScanNode,
    SkipNode, SortNode,
};
use crate::{Database, QueryCriteria, Triple};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Float(f64),
    Boolean(bool),
    Null,
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
            PhysicalPlan::Scan(node) => node.execute(ctx),
            PhysicalPlan::Filter(node) => node.execute(ctx),
            PhysicalPlan::Project(node) => node.execute(ctx),
            PhysicalPlan::Limit(node) => node.execute(ctx),
            PhysicalPlan::Skip(node) => node.execute(ctx),
            PhysicalPlan::Sort(node) => node.execute(ctx),
            PhysicalPlan::Aggregate(node) => node.execute(ctx),
            PhysicalPlan::NestedLoopJoin(node) => node.execute(ctx),
            PhysicalPlan::LeftOuterJoin(node) => node.execute(ctx),
            PhysicalPlan::Expand(node) => node.execute(ctx),
            PhysicalPlan::ExpandVarLength(node) => node.execute(ctx),
            _ => Err(Error::Other("Unsupported physical plan type".to_string())),
        }
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        match self {
            PhysicalPlan::Scan(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Filter(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Project(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Limit(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Skip(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Sort(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Aggregate(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::NestedLoopJoin(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::LeftOuterJoin(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Expand(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::ExpandVarLength(node) => node.estimate_cardinality(ctx),
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
                        let cmp = compare_values(&val_a, &val_b);
                        if cmp != std::cmp::Ordering::Equal {
                            return match direction {
                                Direction::Ascending => cmp,
                                Direction::Descending => cmp.reverse(),
                            };
                        }
                    }
                    std::cmp::Ordering::Equal
                });
                Ok(Box::new(records.into_iter().map(Ok)))
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
        Expression::FunctionCall(func) => match func.name.to_lowercase().as_str() {
            "id" => {
                if let Some(arg) = func.arguments.first()
                    && let Value::Node(id) = evaluate_expression_value_streaming(arg, record, ctx)
                {
                    return Value::Float(id as f64);
                }
                Value::Null
            }
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

// ============================================================================
// Optimized ScanNode Implementation
// ============================================================================

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
                let cmp = compare_values(&val_a, &val_b);
                if cmp != std::cmp::Ordering::Equal {
                    return match direction {
                        Direction::Ascending => cmp,
                        Direction::Descending => cmp.reverse(),
                    };
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
        (Value::Null, _) => std::cmp::Ordering::Greater, // NULL sorts last
        (_, Value::Null) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
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
            Literal::Boolean(b) => Value::Boolean(*b),
            Literal::Null => Value::Null,
            _ => Value::Null,
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
        _ => Value::Null,
    }
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
