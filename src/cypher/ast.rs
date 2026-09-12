use crate::graph::{Direction, Value};
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

impl BinaryOperator {
    pub fn as_str(&self) -> &'static str {
        match self {
            BinaryOperator::Eq => "=",
            BinaryOperator::Neq => "!=",
            BinaryOperator::Lt => "<",
            BinaryOperator::Lte => "<=",
            BinaryOperator::Gt => ">",
            BinaryOperator::Gte => ">=",
            BinaryOperator::And => "AND",
            BinaryOperator::Or => "OR",
        }
    }
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
    /// 带标签的类型谓词：`n:Person`
    LabelCheck {
        var: String,
        label: String,
    },
    /// 函数调用：`id(n)`、`labels(n)`、`type(r)` 等。
    ///
    /// 在 AST 中显式表示，而不是让解析器把 `id` 当作裸变量再丢弃参数：
    /// 后者曾导致 `WHERE id(a) = 1` 静默退化成恒真（见 parser.rs 的尾部检查）。
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
}

/// 本项目支持的标量函数。
///
/// 集中在这里而不是散落在求值分支里，是为了让「支持哪些函数」成为一处可读的
/// 清单：未知函数名在解析阶段就报错，而不是在执行期静默返回 null。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarFunc {
    /// `id(x)` —— 节点或边的内部 ID（u64，以 Int 返回）
    Id,
    /// `labels(n)` —— 节点的标签列表
    Labels,
    /// `type(r)` —— 边的关系类型名
    Type,
}

impl ScalarFunc {
    /// 按名称解析；`None` 表示不是受支持的标量函数。
    pub fn from_name(name: &str) -> Option<Self> {
        // 大小写不敏感：Cypher 函数名不区分大小写
        match name.to_ascii_lowercase().as_str() {
            "id" => Some(ScalarFunc::Id),
            "labels" => Some(ScalarFunc::Labels),
            "type" => Some(ScalarFunc::Type),
            _ => None,
        }
    }

    /// 参数个数（用于解析期校验）。
    pub fn arity(self) -> usize {
        // 三者都恰好接受一个参数
        1
    }

    pub fn name(self) -> &'static str {
        match self {
            ScalarFunc::Id => "id",
            ScalarFunc::Labels => "labels",
            ScalarFunc::Type => "type",
        }
    }
}

/// 节点模式描述
#[derive(Debug, Clone, PartialEq)]
pub struct NodePattern {
    pub variable: Option<String>,
    /// 节点标签集合（Cypher 允许 `(n:A:B)` 多标签）
    pub labels: Vec<String>,
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
    /// 聚合投影项：count/sum/avg/min/max
    Aggregate {
        func: AggregateFunc,
        arg: AggregateArg,
        alias: Option<String>,
    },
    /// 标量函数投影项：`RETURN id(a)`、`labels(n)`、`type(r)`
    Function {
        name: String,
        arg: Box<Expr>,
        alias: Option<String>,
    },
}

/// 聚合函数族
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateFunc {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl AggregateFunc {
    pub fn as_str(&self) -> &'static str {
        match self {
            AggregateFunc::Count => "count",
            AggregateFunc::Sum => "sum",
            AggregateFunc::Avg => "avg",
            AggregateFunc::Min => "min",
            AggregateFunc::Max => "max",
        }
    }
}

/// 聚合函数入参
#[derive(Debug, Clone, PartialEq)]
pub enum AggregateArg {
    /// count(*)
    Star,
    /// sum(n.age) / min(e.weight)
    Property { var: String, prop: String },
    /// count(n)
    Variable(String),
}

/// ORDER BY 排序项
#[derive(Debug, Clone, PartialEq)]
pub struct OrderItem {
    pub expr: Expr,
    /// true 表示 DESC 降序，false 表示 ASC 升序
    pub desc: bool,
}

/// DELETE 目标描述
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteClause {
    pub detach: bool,
    pub targets: Vec<String>,
}

/// SET 子句条目
#[derive(Debug, Clone, PartialEq)]
pub enum SetItem {
    /// `SET n.key = expr`
    Property {
        var: String,
        key: String,
        value: Expr,
    },
    /// SET n:Label
    Label { var: String, label: String },
}

/// MATCH 语句子句集合。
///
/// 独立成结构体并被 `CypherStatement::Match` 装箱持有：该变体字段数远多于
/// `Create`，装箱后各变体尺寸均衡，避免枚举整体被最大变体撑大（每条语句仅
/// 构造一次，此处的间接寻址开销可忽略）。
#[derive(Debug, Clone, PartialEq)]
pub struct MatchClause {
    pub patterns: Vec<PathPattern>,
    pub where_clause: Option<Expr>,
    pub set_clause: Vec<SetItem>,
    pub delete_clause: Option<DeleteClause>,
    pub create_clause: Option<PathPattern>,
    pub return_clause: Option<Vec<ReturnItem>>,
    pub order_by: Vec<OrderItem>,
    pub skip: Option<usize>,
    pub limit: Option<usize>,
}

/// Cypher 语句抽象语法树
#[derive(Debug, Clone, PartialEq)]
pub enum CypherStatement {
    Create { pattern: PathPattern },
    Match(Box<MatchClause>),
}

impl CypherStatement {
    /// 该语句是否会产生任何物理写入（决定查询走共享读锁还是排他写锁）
    pub fn is_mutating(&self) -> bool {
        match self {
            CypherStatement::Create { .. } => true,
            CypherStatement::Match(clause) => {
                !clause.set_clause.is_empty()
                    || clause.delete_clause.is_some()
                    || clause.create_clause.is_some()
            }
        }
    }
}

/// 变更执行结果摘要
#[derive(Debug, Clone, Default)]
pub struct ExecuteResult {
    pub nodes_created: usize,
    pub edges_created: usize,
    pub nodes_deleted: usize,
    pub edges_deleted: usize,
    pub properties_set: usize,
    pub message: String,
}
