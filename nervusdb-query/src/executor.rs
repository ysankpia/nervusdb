use crate::ast::{
    AggregateFunction, Direction, Expression, PathElement, Pattern, RelationshipDirection,
};
use crate::error::{Error, Result};
use crate::evaluator::evaluate_expression_value;
mod binding_utils;
mod core_types;
mod create_delete_ops;
mod foreach_ops;
mod index_seek_plan;
mod join_apply;
mod label_constraint;
mod match_bound_rel_plan;
mod match_in_undirected_plan;
mod match_out_plan;
mod merge_execution;
mod merge_helpers;
mod merge_overlay;
mod path_usage;
mod plan_dispatch;
mod plan_head;
mod plan_iterators;
mod plan_mid;
mod plan_tail;
mod plan_types;
mod procedure_registry;
mod projection_sort;
mod property_bridge;
mod read_path;
mod runtime_limits;
mod txn_engine_impl;
mod write_dispatch;
mod write_forwarders;
mod write_orchestration;
mod write_path;
mod write_support;
use binding_utils::{
    apply_optional_unbinds_row, row_contains_all_bindings, row_matches_node_binding,
};
use join_apply::{ApplyIter, ProcedureCallIter};
use label_constraint::{LabelConstraint, node_matches_label_constraint, resolve_label_constraint};
use merge_overlay::{MergeOverlayEdge, MergeOverlayNode, MergeOverlayState};
pub use nervusdb_api::LabelId;
use nervusdb_api::{EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, RelTypeId};
use path_usage::{edge_multiplicity, path_alias_contains_edge};
use plan_iterators::{CartesianProductIter, FilterIter, NodeScanIter};
use projection_sort::execute_aggregate;
use property_bridge::{
    api_property_map_to_storage, merge_props_to_values, merge_storage_property_to_api,
};
use write_forwarders::{
    convert_executor_value_to_property, execute_create, execute_create_write_rows, execute_delete,
    execute_delete_on_rows, execute_foreach, execute_merge_create_from_rows,
};
use write_path::{
    apply_label_overlay_to_rows, apply_removed_property_overlay_to_rows,
    apply_set_map_overlay_to_rows, apply_set_property_overlay_to_rows, execute_remove,
    execute_remove_labels, execute_set, execute_set_from_maps, execute_set_labels,
};

const UNLABELED_LABEL_ID: LabelId = LabelId::MAX;
pub use core_types::{NodeValue, PathValue, ReifiedPathValue, RelationshipValue, Row, Value};
pub use plan_types::{Plan, PlanIterator};
pub use procedure_registry::{
    ErasedSnapshot, Procedure, ProcedureRegistry, TestProcedureField, TestProcedureFixture,
    TestProcedureType, clear_test_procedure_fixtures, get_procedure_registry,
    get_test_procedure_fixture, register_test_procedure_fixture,
};

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

pub fn execute_write_with_rows<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
) -> Result<(u32, Vec<Row>)> {
    write_orchestration::execute_write_with_rows(plan, snapshot, txn, params)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_merge_with_rows<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
    on_create_items: &[(String, String, Expression)],
    on_create_map_items: &[(String, Expression, bool)],
    on_match_items: &[(String, String, Expression)],
    on_match_map_items: &[(String, Expression, bool)],
    on_create_labels: &[(String, Vec<String>)],
    on_match_labels: &[(String, Vec<String>)],
) -> Result<(u32, Vec<Row>)> {
    write_orchestration::execute_merge_with_rows(
        plan,
        snapshot,
        txn,
        params,
        on_create_items,
        on_create_map_items,
        on_match_items,
        on_match_map_items,
        on_create_labels,
        on_match_labels,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_merge<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
    on_create_items: &[(String, String, Expression)],
    on_create_map_items: &[(String, Expression, bool)],
    on_match_items: &[(String, String, Expression)],
    on_match_map_items: &[(String, Expression, bool)],
    on_create_labels: &[(String, Vec<String>)],
    on_match_labels: &[(String, Vec<String>)],
) -> Result<u32> {
    merge_execution::execute_merge(
        plan,
        snapshot,
        txn,
        params,
        on_create_items,
        on_create_map_items,
        on_match_items,
        on_match_map_items,
        on_create_labels,
        on_match_labels,
    )
}

pub trait WriteableGraph {
    fn create_node(&mut self, external_id: ExternalId, label_id: LabelId)
    -> Result<InternalNodeId>;
    fn add_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()>;
    fn remove_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()>;
    fn create_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()>;
    fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()>;
    fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()>;
    fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()>;
    fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) -> Result<()>;
    fn tombstone_node(&mut self, node: InternalNodeId) -> Result<()>;
    fn tombstone_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()>;

    // T65: Dynamic schema support
    fn get_or_create_label_id(&mut self, name: &str) -> Result<LabelId>;
    fn get_or_create_rel_type_id(&mut self, name: &str) -> Result<RelTypeId>;

    fn staged_created_nodes_with_labels(&self) -> Vec<(InternalNodeId, Vec<String>)> {
        Vec::new()
    }
}

pub use nervusdb_storage::property::PropertyValue;

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
