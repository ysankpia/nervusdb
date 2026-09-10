use crate::graph::{Direction, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 二元操作符
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOperator {
    Eq,  // =
    Neq, // !=
    Lt,  // <
    Lte, // <=
    Gt,  // >
    Gte, // >=
    And, // AND
    Or,  // OR
}

/// 表达式 AST
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Value),
    Variable(String),
    PropertyAccess {
        var: String,
        prop: String,
    },
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
    },
}

/// 节点模式描述
#[derive(Debug, Clone, PartialEq)]
pub struct NodePattern {
    pub variable: Option<String>,
    pub label: Option<String>,
    pub properties: HashMap<String, Value>,
}

/// 关系边模式描述
#[derive(Debug, Clone, PartialEq)]
pub struct RelPattern {
    pub variable: Option<String>,
    pub rel_type: Option<String>,
    pub properties: HashMap<String, Value>,
    pub weight: Option<f64>,
    pub hops: Option<(usize, usize)>, // (min_hops, max_hops) 如 *1..3
    pub direction: Direction,
}

/// 路径模式：交替出现的 NodePattern 和 RelPattern
#[derive(Debug, Clone, PartialEq)]
pub struct PathPattern {
    pub nodes: Vec<NodePattern>,
    pub edges: Vec<RelPattern>,
}

/// RETURN 投影项
#[derive(Debug, Clone, PartialEq)]
pub enum ReturnItem {
    All,
    Variable {
        var: String,
        alias: Option<String>,
    },
    Property {
        var: String,
        prop: String,
        alias: Option<String>,
    },
}

/// DELETE 目标描述
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteClause {
    pub detach: bool,
    pub targets: Vec<String>,
}

/// Cypher 语句抽象语法树
#[derive(Debug, Clone, PartialEq)]
pub enum CypherStatement {
    Create {
        pattern: PathPattern,
    },
    Match {
        pattern: PathPattern,
        where_clause: Option<Expr>,
        return_clause: Option<Vec<ReturnItem>>,
        delete_clause: Option<DeleteClause>,
        limit: Option<usize>,
    },
}

/// 变更执行结果摘要
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecuteResult {
    pub nodes_created: usize,
    pub edges_created: usize,
    pub nodes_deleted: usize,
    pub edges_deleted: usize,
    pub properties_set: usize,
    pub message: String,
}
