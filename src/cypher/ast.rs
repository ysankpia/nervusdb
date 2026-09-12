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
    /// 列表字面量：`[1, 2, 3]`、`['a', 'b']`。
    ///
    /// 只作为**求值期的中间值**存在（`UNWIND [1,2,3] AS x`）。列表不能被写入属性
    /// ——见 `Value::List` 的说明——因此这个变体不需要落盘支持。
    ListLiteral(Vec<Expr>),

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
    /// 属性约束/赋值。
    ///
    /// 值是**表达式**而非字面量：`CREATE (n {name: x})` 里的 `x` 可以来自
    /// `UNWIND ... AS x`。这是批量入库的前提——没有它，`UNWIND` 只能用来产生行，
    /// 无法把行写进图里。
    ///
    /// 匹配场景（`MATCH (n {k: v})`）要求这些表达式求值为字面量，由执行器判定。
    pub properties: HashMap<String, Expr>,
}

/// 关系边模式描述
#[derive(Debug, Clone, PartialEq)]
pub struct RelPattern {
    pub variable: Option<String>,
    pub rel_type: Option<String>,
    /// 同 `NodePattern::properties`：表达式，以支持 `UNWIND` 变量
    pub properties: HashMap<String, Expr>,
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
    Create {
        pattern: PathPattern,
    },
    Match(Box<MatchClause>),
    /// `UNWIND <expr> AS <var>`，后面可选跟 `CREATE` / `MATCH` 子句。
    ///
    /// 语义：把 `<expr>` 求值为列表，为**每个元素**产生一行、把元素绑定到 `<var>`，
    /// 再把后续子句施加到这些行上。这是把「批量数据」表达进查询的唯一方式，
    /// 也是 `MERGE` 得以便捷的前提。
    Unwind {
        /// 求值为列表的表达式（通常是列表字面量或变量）
        expr: Expr,
        /// 每个元素绑定的变量名
        variable: String,
        /// 后续子句（可为空：`UNWIND [1,2,3] AS x RETURN x` 里的 RETURN）
        return_clause: Option<Vec<ReturnItem>>,
        order_by: Vec<OrderItem>,
        skip: Option<usize>,
        limit: Option<usize>,
        /// `UNWIND ... AS x CREATE ...`
        create_clause: Option<PathPattern>,
    },

    /// `MERGE <pattern>`：匹配则复用，不匹配则创建。
    ///
    /// 这是幂等写：同一个模式重复执行不会产生重复数据，这使得「先查再写」这一
    /// 常见模式不必由调用方自己实现——调用方实现时会出现「检查与写入之间有空隙」
    /// 的竞态，而那条路径的失败是静默的重复数据。
    ///
    /// 语义要点：模式**整体**匹配。`MERGE (a)-[:R]->(b)` 若整体不存在，则整个模式
    /// 连同两端节点一起创建（与 Cypher 一致，而不是复用部分匹配的节点）。
    Merge {
        pattern: PathPattern,
        /// `ON CREATE SET ...`：本次创建时额外施加的写操作
        on_create: Vec<SetItem>,
        /// `ON MATCH SET ...`：命中既有数据时额外施加的写操作
        on_match: Vec<SetItem>,
        return_clause: Option<Vec<ReturnItem>>,
        order_by: Vec<OrderItem>,
        skip: Option<usize>,
        limit: Option<usize>,
    },

    /// `EXPLAIN <query>`：只输出执行计划，**不执行**查询。
    ///
    /// 计划由 `Image` 的静态结构推导，不需要触碰磁盘——因此 EXPLAIN 在空库上
    /// 同样可用，也不会产生任何副作用。
    Explain(Box<CypherStatement>),
}

impl CypherStatement {
    /// 该语句是否会产生任何物理写入（决定查询走共享读锁还是排他写锁）
    pub fn is_mutating(&self) -> bool {
        match self {
            CypherStatement::Create { .. } => true,
            // MERGE 命中时虽不写数据，但它**可能**写，因此必须走排他写锁：
            // 用读锁执行会让「检查—创建」之间的空隙变成重复数据的来源。
            CypherStatement::Merge { .. } => true,
            // EXPLAIN 只描述计划，从不写入
            CypherStatement::Explain(_) => false,
            CypherStatement::Match(clause) => {
                !clause.set_clause.is_empty()
                    || clause.delete_clause.is_some()
                    || clause.create_clause.is_some()
            }
            // `UNWIND [..] AS x RETURN x` 是只读的；只有带 `CREATE` 才写入。
            // 这条判定决定走共享读锁还是排他写锁，判错会让读查询被写锁串行化，
            // 或者更糟——让写操作走只读路径并被拒绝。
            CypherStatement::Unwind { create_clause, .. } => create_clause.is_some(),
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
