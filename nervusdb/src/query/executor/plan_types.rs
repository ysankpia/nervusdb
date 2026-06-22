use super::{
    CartesianProductIter, ExpandIter, Expression, FilterIter, FilteredMatchOutIter, GraphSnapshot,
    LimitIter, MatchBoundRelIter, NodeScanIter, Pattern, ProjectIter, RelationshipDirection,
    Result, Row, ValuesIter,
};
use crate::api::PropertyValue;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Plan {
    /// `RETURN 1`
    ReturnOne,
    /// `MATCH (n) RETURN ...`
    NodeScan {
        alias: Arc<str>,
        label: Option<String>,
        property_eq: Option<(String, PropertyValue)>,
        optional: bool,
    },
    /// `MATCH (a)-[:rel]->(b) RETURN ...`
    MatchOut {
        input: Option<Box<Plan>>,
        src_alias: Arc<str>,
        rels: Vec<String>,
        edge_alias: Option<Arc<str>>,
        dst_alias: Arc<str>,
        dst_labels: Vec<String>,
        src_prebound: bool,
        limit: Option<u32>,
        // Note: project is kept for backward compatibility but projection
        // should happen after filtering (see Plan::Project)
        project: Vec<String>,
        project_external: bool,
        optional: bool,
        optional_unbind: Vec<String>,
        path_alias: Option<Arc<str>>,
    },
    MatchBoundRel {
        input: Box<Plan>,
        rel_alias: Arc<str>,
        src_alias: Arc<str>,
        dst_alias: Arc<str>,
        dst_labels: Vec<String>,
        src_prebound: bool,
        rels: Vec<String>,
        direction: RelationshipDirection,
        optional: bool,
        optional_unbind: Vec<String>,
        path_alias: Option<Arc<str>>,
    },
    /// `MATCH ... WHERE ... RETURN ...` (with filter)
    Filter {
        input: Box<Plan>,
        predicate: Expression,
    },
    /// Project expressions to new variables
    Project {
        input: Box<Plan>,
        projections: Vec<(String, Expression)>, // (Result/Alias Name, Expression to Eval)
    },
    /// `LIMIT` - limit result count
    Limit {
        input: Box<Plan>,
        limit: Expression,
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
    /// `CartesianProduct` - multiply two plans (join without shared variables)
    CartesianProduct {
        left: Box<Plan>,
        right: Box<Plan>,
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

#[allow(clippy::large_enum_variant)]
pub enum PlanIterator<'a, S: GraphSnapshot> {
    ReturnOne(std::iter::Once<Result<Row>>),
    NodeScan(NodeScanIter<'a, S>),
    Filter(FilterIter<'a, S>),
    Project(Box<ProjectIter<'a, S>>),
    Limit(Box<LimitIter<'a, S>>),
    Values(Box<ValuesIter>),
    Expand(Box<ExpandIter<'a, S>>),
    MatchOutFiltered(Box<FilteredMatchOutIter<'a, S>>),
    MatchBoundRel(Box<MatchBoundRelIter<'a, S>>),
    CartesianProduct(Box<CartesianProductIter<'a, S>>),
}

impl<'a, S: GraphSnapshot> Iterator for PlanIterator<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            PlanIterator::ReturnOne(iter) => iter.next(),
            PlanIterator::NodeScan(iter) => iter.next(),
            PlanIterator::Filter(iter) => iter.next(),
            PlanIterator::Project(iter) => iter.next(),
            PlanIterator::Limit(iter) => iter.next(),
            PlanIterator::Values(iter) => iter.next(),
            PlanIterator::Expand(iter) => iter.next(),
            PlanIterator::MatchOutFiltered(iter) => iter.next(),
            PlanIterator::MatchBoundRel(iter) => iter.next(),
            PlanIterator::CartesianProduct(iter) => iter.next(),
        }
    }
}
