use super::{
    AggregateFunction, ApplyIter, CartesianProductIter, Direction, Expression, FilterIter,
    GraphSnapshot, NodeScanIter, Pattern, ProcedureCallIter, RelationshipDirection, Result, Row,
};

#[derive(Debug, Clone)]
pub enum Plan {
    /// `RETURN 1`
    ReturnOne,
    /// `MATCH (n) RETURN ...`
    NodeScan {
        alias: String,
        label: Option<String>,
        optional: bool,
    },
    /// `MATCH (a)-[:rel]->(b) RETURN ...`
    MatchOut {
        input: Option<Box<Plan>>,
        src_alias: String,
        rels: Vec<String>,
        edge_alias: Option<String>,
        dst_alias: String,
        dst_labels: Vec<String>,
        src_prebound: bool,
        limit: Option<u32>,
        // Note: project is kept for backward compatibility but projection
        // should happen after filtering (see Plan::Project)
        project: Vec<String>,
        project_external: bool,
        optional: bool,
        optional_unbind: Vec<String>,
        path_alias: Option<String>,
    },
    /// `MATCH (a)-[:rel*min..max]->(b) RETURN ...` (variable length)
    MatchOutVarLen {
        input: Option<Box<Plan>>,
        src_alias: String,
        rels: Vec<String>,
        edge_alias: Option<String>,
        dst_alias: String,
        dst_labels: Vec<String>,
        src_prebound: bool,
        direction: RelationshipDirection,
        min_hops: u32,
        max_hops: Option<u32>,
        limit: Option<u32>,
        project: Vec<String>,
        project_external: bool,
        optional: bool,
        optional_unbind: Vec<String>,
        path_alias: Option<String>,
    },
    MatchIn {
        input: Option<Box<Plan>>,
        src_alias: String,
        rels: Vec<String>,
        edge_alias: Option<String>,
        dst_alias: String,
        dst_labels: Vec<String>,
        src_prebound: bool,
        limit: Option<u32>,
        optional: bool,
        optional_unbind: Vec<String>,
        path_alias: Option<String>,
    },
    MatchUndirected {
        input: Option<Box<Plan>>,
        src_alias: String,
        rels: Vec<String>,
        edge_alias: Option<String>,
        dst_alias: String,
        dst_labels: Vec<String>,
        src_prebound: bool,
        limit: Option<u32>,
        optional: bool,
        optional_unbind: Vec<String>,
        path_alias: Option<String>,
    },
    MatchBoundRel {
        input: Box<Plan>,
        rel_alias: String,
        src_alias: String,
        dst_alias: String,
        dst_labels: Vec<String>,
        src_prebound: bool,
        rels: Vec<String>,
        direction: RelationshipDirection,
        optional: bool,
        optional_unbind: Vec<String>,
        path_alias: Option<String>,
    },
    /// `MATCH ... WHERE ... RETURN ...` (with filter)
    Filter {
        input: Box<Plan>,
        predicate: Expression,
    },
    /// 修复 OPTIONAL MATCH + WHERE 语义：按外层行回填 null 行
    OptionalWhereFixup {
        outer: Box<Plan>,
        filtered: Box<Plan>,
        null_aliases: Vec<String>,
    },
    /// Project expressions to new variables
    Project {
        input: Box<Plan>,
        projections: Vec<(String, Expression)>, // (Result/Alias Name, Expression to Eval)
    },
    /// Aggregation: COUNT, SUM, AVG with optional grouping
    Aggregate {
        input: Box<Plan>,
        group_by: Vec<String>,                        // Variables to group by
        aggregates: Vec<(AggregateFunction, String)>, // (Function, Alias)
    },
    /// `ORDER BY` - sort results
    OrderBy {
        input: Box<Plan>,
        items: Vec<(Expression, Direction)>, // (Expression to sort by, ASC|DESC)
    },
    /// `SKIP` - skip first n rows
    Skip {
        input: Box<Plan>,
        skip: Expression,
    },
    /// `LIMIT` - limit result count
    Limit {
        input: Box<Plan>,
        limit: Expression,
    },
    /// `RETURN DISTINCT` - deduplicate results
    Distinct {
        input: Box<Plan>,
    },
    /// `UNWIND` - expand a list into multiple rows
    Unwind {
        input: Box<Plan>,
        expression: Expression,
        alias: String,
    },
    /// `UNION` / `UNION ALL` - combine results from two queries
    Union {
        left: Box<Plan>,
        right: Box<Plan>,
        all: bool, // true = UNION ALL (keep duplicates), false = UNION (distinct)
    },

    /// `DELETE` - delete nodes/edges (with input plan for variable resolution)
    Delete {
        input: Box<Plan>,
        detach: bool,
        expressions: Vec<Expression>,
    },
    /// `SetProperty` - update properties on nodes/edges
    SetProperty {
        input: Box<Plan>,
        items: Vec<(String, String, Expression)>, // (variable, key, value_expression)
    },
    /// `SET n = {...}` / `SET n += {...}` - replace or append properties from map expression
    SetPropertiesFromMap {
        input: Box<Plan>,
        items: Vec<(String, Expression, bool)>, // (variable, map_expression, append)
    },
    /// `SET n:Label` - add labels on nodes
    SetLabels {
        input: Box<Plan>,
        items: Vec<(String, Vec<String>)>, // (variable, labels)
    },
    /// `REMOVE n.prop` - remove properties from nodes/edges
    RemoveProperty {
        input: Box<Plan>,
        items: Vec<(String, String)>, // (variable, key)
    },
    /// `REMOVE n:Label` - remove labels from nodes
    RemoveLabels {
        input: Box<Plan>,
        items: Vec<(String, Vec<String>)>, // (variable, labels)
    },
    /// `IndexSeek` - optimize scan using index if available, else fallback
    IndexSeek {
        alias: String,
        label: String,
        field: String,
        value_expr: Expression,
        fallback: Box<Plan>,
    },
    /// `CartesianProduct` - multiply two plans (join without shared variables)
    CartesianProduct {
        left: Box<Plan>,
        right: Box<Plan>,
    },
    /// `Apply` - execute subquery for each row (Correlated Subquery)
    Apply {
        input: Box<Plan>,
        subquery: Box<Plan>,
        alias: Option<String>, // Optional alias for subquery result? usually subquery projects...
    },
    /// `CALL namespace.name(args) YIELD x, y`
    ProcedureCall {
        input: Box<Plan>,
        name: Vec<String>,
        args: Vec<Expression>,
        yields: Vec<(String, Option<String>)>, // (field_name, alias)
    },
    Foreach {
        input: Box<Plan>,
        variable: String,
        list: Expression,
        sub_plan: Box<Plan>,
    },
    // Injects specific rows into the pipeline (used for FOREACH context and constructing literal rows)
    Values {
        rows: Vec<Row>,
    },
    Create {
        input: Box<Plan>,
        pattern: Pattern,
        merge: bool,
    },
}

pub enum PlanIterator<'a, S: GraphSnapshot> {
    ReturnOne(std::iter::Once<Result<Row>>),
    NodeScan(NodeScanIter<'a, S>),
    Filter(FilterIter<'a, S>),
    CartesianProduct(Box<CartesianProductIter<'a, S>>),
    Apply(Box<ApplyIter<'a, S>>),
    ProcedureCall(Box<ProcedureCallIter<'a, S>>),

    Dynamic(Box<dyn Iterator<Item = Result<Row>> + 'a>),
}

impl<'a, S: GraphSnapshot> Iterator for PlanIterator<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            PlanIterator::ReturnOne(iter) => iter.next(),
            PlanIterator::NodeScan(iter) => iter.next(),
            PlanIterator::Filter(iter) => iter.next(),
            PlanIterator::CartesianProduct(iter) => iter.next(),
            PlanIterator::Apply(iter) => iter.next(),
            PlanIterator::ProcedureCall(iter) => iter.next(),

            PlanIterator::Dynamic(iter) => iter.next(),
        }
    }
}
