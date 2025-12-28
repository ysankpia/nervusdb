use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Query {
    pub clauses: Vec<Clause>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Clause {
    Match(MatchClause),
    Create(CreateClause),
    Merge(MergeClause),
    Unwind(UnwindClause),
    Call(CallClause),
    Return(ReturnClause),
    Where(WhereClause),
    With(WithClause),
    Set(SetClause),
    Delete(DeleteClause),
    Union(UnionClause),
    // Remove can be added later
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MatchClause {
    pub optional: bool,
    pub pattern: Pattern,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateClause {
    pub pattern: Pattern,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeClause {
    pub pattern: Pattern,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnwindClause {
    pub expression: Expression,
    pub alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallClause {
    pub query: Query,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnionClause {
    pub all: bool,
    pub query: Query,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReturnClause {
    pub distinct: bool,
    pub items: Vec<ReturnItem>,
    pub order_by: Option<OrderByClause>,
    pub limit: Option<u32>,
    pub skip: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReturnItem {
    pub expression: Expression,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WhereClause {
    pub expression: Expression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WithClause {
    pub distinct: bool,
    pub items: Vec<ReturnItem>,
    pub where_clause: Option<WhereClause>,
    pub order_by: Option<OrderByClause>,
    pub limit: Option<u32>,
    pub skip: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderByClause {
    pub items: Vec<OrderByItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderByItem {
    pub expression: Expression,
    pub direction: Direction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Direction {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetClause {
    pub items: Vec<SetItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetItem {
    pub property: PropertyAccess,
    pub value: Expression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeleteClause {
    pub detach: bool,
    pub expressions: Vec<Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pattern {
    pub elements: Vec<PathElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PathElement {
    Node(NodePattern),
    Relationship(RelationshipPattern),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodePattern {
    pub variable: Option<String>,
    pub labels: Vec<String>,
    pub properties: Option<PropertyMap>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelationshipPattern {
    pub variable: Option<String>,
    pub types: Vec<String>,
    pub direction: RelationshipDirection,
    pub properties: Option<PropertyMap>,
    pub variable_length: Option<VariableLength>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelationshipDirection {
    LeftToRight,
    RightToLeft,
    Undirected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableLength {
    pub min: Option<u32>,
    pub max: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyMap {
    pub properties: Vec<PropertyPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyPair {
    pub key: String,
    pub value: Expression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Expression {
    Literal(Literal),
    Variable(String),
    PropertyAccess(PropertyAccess),
    Binary(Box<BinaryExpression>),
    Unary(Box<UnaryExpression>),
    FunctionCall(FunctionCall),
    Case(Box<CaseExpression>),
    Exists(Box<ExistsExpression>),
    List(Vec<Expression>),
    ListComprehension(Box<ListComprehension>),
    Map(PropertyMap),
    Parameter(String), // $param
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExistsExpression {
    Pattern(Pattern),
    Subquery(Query),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListComprehension {
    pub variable: String,
    pub list: Expression,
    pub where_expression: Option<Expression>,
    pub map_expression: Option<Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseExpression {
    pub alternatives: Vec<CaseAlternative>,
    pub else_expression: Option<Expression>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseAlternative {
    pub when: Expression,
    pub then: Expression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Literal {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyAccess {
    pub variable: String,
    pub property: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BinaryExpression {
    pub operator: BinaryOperator,
    pub left: Expression,
    pub right: Expression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BinaryOperator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    And,
    Or,
    Xor,
    In,
    NotIn,
    StartsWith,
    EndsWith,
    Contains,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnaryExpression {
    pub operator: UnaryOperator,
    pub argument: Expression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UnaryOperator {
    Not,
    Negate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: Vec<Expression>,
}
