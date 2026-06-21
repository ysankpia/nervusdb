use crate::ast::{Expression, PathElement, Pattern, RelationshipDirection};
use crate::error::{Error, Result};
use crate::evaluator::evaluate_expression_value;
mod core_types;
mod create_delete_ops;
mod label_constraint;
mod match_bound_rel_plan;
mod match_out_plan;
mod plan_dispatch;
mod plan_head;
mod plan_iterators;
mod plan_mid;
mod plan_tail;
mod plan_types;
mod read_path;
mod write_dispatch;
mod write_path;
use create_delete_ops::{execute_create, execute_delete};
use label_constraint::{LabelConstraint, node_matches_label_constraint, resolve_label_constraint};
use match_bound_rel_plan::MatchBoundRelIter;
use match_out_plan::FilteredMatchOutIter;
pub use nervusdb_api::LabelId;
use nervusdb_api::{EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, RelTypeId};
use plan_iterators::{
    CartesianProductIter, FilterIter, LimitIter, NodeScanIter, ProjectIter, ValuesIter,
};
use read_path::ExpandIter;
use write_path::{convert_executor_value_to_property, execute_set};

const UNLABELED_LABEL_ID: LabelId = LabelId::MAX;
pub use core_types::{NodeValue, PathValue, ReifiedPathValue, RelationshipValue, Row, Value};
pub use plan_types::{Plan, PlanIterator};

pub fn execute_plan<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    plan: &'a Plan,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    plan_dispatch::execute_plan(snapshot, plan, params)
}

/// Execute a write plan (CREATE/DELETE/SET/REMOVE) with a transaction
pub fn execute_write<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
) -> Result<u32> {
    write_dispatch::execute_write(plan, snapshot, txn, params)
}

pub use nervusdb_api::{PropertyValue, WriteableGraph};

pub fn convert_api_property_to_value(api_value: &nervusdb_api::PropertyValue) -> Value {
    match api_value {
        nervusdb_api::PropertyValue::Null => Value::Null,
        nervusdb_api::PropertyValue::Bool(b) => Value::Bool(*b),
        nervusdb_api::PropertyValue::Int(i) => Value::Int(*i),
        nervusdb_api::PropertyValue::Float(f) => Value::Float(*f),
        nervusdb_api::PropertyValue::String(s) => Value::String(s.clone()),
        nervusdb_api::PropertyValue::DateTime(i) => Value::DateTime(*i),
        nervusdb_api::PropertyValue::Blob(b) => Value::Blob(b.clone()),
        nervusdb_api::PropertyValue::List(l) => {
            Value::List(l.iter().map(convert_api_property_to_value).collect())
        }
        nervusdb_api::PropertyValue::Map(m) => Value::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                .collect(),
        ),
    }
}

pub fn parse_u32_identifier(name: &str) -> Result<u32> {
    name.parse::<u32>()
        .map_err(|_| Error::NotImplemented("non-numeric label/rel identifiers in M3"))
}
