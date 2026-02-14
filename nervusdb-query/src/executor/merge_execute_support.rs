use super::{
    PropertyValue, UNLABELED_LABEL_ID, WriteableGraph, evaluate_property_value,
    merge_storage_property_to_api,
};
use crate::error::Result;
use nervusdb_api::{ExternalId, GraphSnapshot, InternalNodeId};

#[derive(Clone)]
pub(super) struct ExecMergeOverlayNode {
    pub(super) label: Option<String>,
    pub(super) props: Vec<(String, PropertyValue)>,
    pub(super) iid: InternalNodeId,
}

pub(super) fn exec_merge_eval_props(
    props: &crate::ast::PropertyMap,
    params: &crate::query_api::Params,
) -> Result<Vec<(String, PropertyValue)>> {
    let mut out = Vec::with_capacity(props.properties.len());
    for prop in &props.properties {
        let v = evaluate_property_value(&prop.value, params)?;
        // NULL values are allowed in MERGE properties
        out.push((prop.key.clone(), v));
    }
    Ok(out)
}

fn exec_merge_overlay_lookup(
    overlay: &[ExecMergeOverlayNode],
    label: &Option<String>,
    expected: &[(String, PropertyValue)],
) -> Option<InternalNodeId> {
    overlay.iter().find_map(|n| {
        if &n.label != label {
            return None;
        }
        for (k, v) in expected {
            if n.props.iter().find(|(kk, _)| kk == k).map(|(_, vv)| vv) != Some(v) {
                return None;
            }
        }
        Some(n.iid)
    })
}

fn exec_merge_find_existing_node<S: GraphSnapshot>(
    snapshot: &S,
    label: &Option<String>,
    expected: &[(String, PropertyValue)],
) -> Option<InternalNodeId> {
    let label_id = match label {
        None => None,
        Some(name) => match snapshot.resolve_label_id(name) {
            Some(id) => Some(id),
            None => return None,
        },
    };

    for iid in snapshot.nodes() {
        if snapshot.is_tombstoned_node(iid) {
            continue;
        }
        if let Some(lid) = label_id
            && snapshot.node_label(iid) != Some(lid)
        {
            continue;
        }
        let mut ok = true;
        for (k, v) in expected {
            if snapshot.node_property(iid, k) != Some(merge_storage_property_to_api(v)) {
                ok = false;
                break;
            }
        }
        if ok {
            return Some(iid);
        }
    }
    None
}

pub(super) fn exec_merge_create_node(
    txn: &mut dyn WriteableGraph,
    labels: &[String],
    props: &[(String, PropertyValue)],
    created_count: &mut u32,
) -> Result<InternalNodeId> {
    let external_id = ExternalId::from(
        *created_count as u64 + chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
    );
    let label_id = if let Some(l) = labels.first() {
        txn.get_or_create_label_id(l)?
    } else {
        UNLABELED_LABEL_ID
    };

    let iid = txn.create_node(external_id, label_id)?;
    for extra_label in labels.iter().skip(1) {
        let extra_label_id = txn.get_or_create_label_id(extra_label)?;
        txn.add_node_label(iid, extra_label_id)?;
    }
    *created_count += 1;
    for (k, v) in props {
        txn.set_node_property(iid, k.clone(), v.clone())?;
    }
    Ok(iid)
}

pub(super) fn exec_merge_find_or_create_node<S: GraphSnapshot>(
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    node: &crate::ast::NodePattern,
    overlay: &mut Vec<ExecMergeOverlayNode>,
    params: &crate::query_api::Params,
    created_count: &mut u32,
) -> Result<(InternalNodeId, bool)> {
    let labels = node.labels.clone();
    let label = labels.first().cloned();
    let props = node.properties.as_ref();
    let expected = if let Some(props) = props {
        exec_merge_eval_props(props, params)?
    } else {
        Vec::new()
    };

    if let Some(iid) = exec_merge_overlay_lookup(overlay, &label, &expected) {
        return Ok((iid, false));
    }
    if let Some(iid) = exec_merge_find_existing_node(snapshot, &label, &expected) {
        return Ok((iid, false));
    }

    let iid = exec_merge_create_node(txn, &labels, &expected, created_count)?;
    overlay.push(ExecMergeOverlayNode {
        label,
        props: expected,
        iid,
    });
    Ok((iid, true))
}
