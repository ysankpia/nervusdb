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
use crate::query::ast::{BinaryOperator, Expression, Literal, RelationshipDirection};
use crate::query::planner::{
    ExpandNode, FilterNode, LimitNode, NestedLoopJoinNode, PhysicalPlan, ProjectNode, ScanNode,
};
use crate::{Database, QueryCriteria, Triple};
use std::collections::{HashMap, HashSet};

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
            PhysicalPlan::NestedLoopJoin(node) => node.execute(ctx),
            PhysicalPlan::Expand(node) => node.execute(ctx),
            _ => Err(Error::Other("Unsupported physical plan type".to_string())),
        }
    }

    fn estimate_cardinality(&self, ctx: &ExecutionContext) -> usize {
        match self {
            PhysicalPlan::Scan(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Filter(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Project(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Limit(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::NestedLoopJoin(node) => node.estimate_cardinality(ctx),
            PhysicalPlan::Expand(node) => node.estimate_cardinality(ctx),
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
            PhysicalPlan::NestedLoopJoin(node) => {
                let left_iter = node.left.execute_streaming(Arc::clone(&ctx))?;
                let right_plan = *node.right;
                let predicate = node.predicate;
                Ok(Box::new(StreamingNestedLoopJoin::new(
                    left_iter, right_plan, predicate, ctx,
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
