//! Unstable administrative tools for NervusDB 0.x.
//!
//! This module is behind the `unstable-admin` feature. It is intentionally not
//! part of the 0.1 stable Rust API surface; the CLI uses it for offline
//! maintenance commands.

use crate::api::{EdgeKey, InternalNodeId, LabelId, PropertyValue, RelTypeId};
use crate::storage::engine::{GraphEngine, scalar_indexable_value};
use crate::storage::layout::*;
use crate::{Error, Result};
use fjall::{PersistMode, Readable};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Options for the unstable fsck-lite check.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsckOptions {
    /// Rebuild repairable derived indexes after checking.
    pub repair: bool,
}

/// Fsck-lite report.
#[derive(Debug, Clone, Serialize)]
pub struct FsckReport {
    pub ok: bool,
    pub repaired: bool,
    pub checked: FsckChecked,
    pub issues: Vec<FsckIssue>,
    pub repairs: Vec<FsckRepair>,
}

/// Raw keyspace counters inspected by fsck-lite.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct FsckChecked {
    pub nodes: u64,
    pub node_labels: u64,
    pub label_nodes: u64,
    pub node_props: u64,
    pub idx_node_props: u64,
    pub adj_out: u64,
    pub adj_in: u64,
    pub edge_props: u64,
}

/// One fsck-lite issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FsckIssue {
    pub kind: FsckIssueKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<InternalNodeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<LabelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel: Option<RelTypeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst: Option<InternalNodeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_key: Option<String>,
}

/// Fsck-lite issue kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FsckIssueKind {
    MissingLabelNodeIndex,
    StaleLabelNodeIndex,
    MissingNodePropertyIndex,
    StaleNodePropertyIndex,
    AdjacencyMismatch,
    OrphanEdgeProperty,
    OrphanNodeProperty,
    OrphanNodeLabel,
    MalformedNode,
    MalformedNodeLabel,
    MalformedLabelNode,
    MalformedNodeProperty,
    MalformedNodePropertyIndex,
    MalformedAdjOut,
    MalformedAdjIn,
    MalformedEdgeProperty,
}

/// One fsck-lite repair action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FsckRepair {
    pub kind: FsckRepairKind,
    pub removed: u64,
    pub inserted: u64,
}

/// Fsck-lite repair kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FsckRepairKind {
    RebuiltLabelNodes,
    RebuiltNodePropertyIndex,
}

#[derive(Debug, Default)]
struct CheckState {
    checked: FsckChecked,
    issues: Vec<FsckIssue>,
    live_nodes: BTreeSet<InternalNodeId>,
    node_labels: BTreeMap<InternalNodeId, BTreeSet<LabelId>>,
    node_props: BTreeMap<InternalNodeId, BTreeMap<String, PropertyValue>>,
    expected_label_nodes: BTreeSet<Vec<u8>>,
    actual_label_nodes: BTreeSet<Vec<u8>>,
    all_label_node_keys: Vec<Vec<u8>>,
    expected_node_prop_indexes: BTreeSet<Vec<u8>>,
    actual_node_prop_indexes: BTreeSet<Vec<u8>>,
    all_node_prop_index_keys: Vec<Vec<u8>>,
    adj_out: BTreeSet<EdgeKey>,
    adj_in: BTreeSet<EdgeKey>,
}

/// Run fsck-lite against a database directory.
///
/// `repair` mode is intended for offline use. It only rebuilds derived indexes
/// (`label_nodes` and `idx_node_props`) from canonical graph keyspaces.
pub fn fsck(path: impl AsRef<Path>, options: FsckOptions) -> Result<FsckReport> {
    let engine = GraphEngine::open(path).map_err(Error::from)?;
    fsck_engine(&engine, options).map_err(Error::from)
}

fn fsck_engine(engine: &GraphEngine, options: FsckOptions) -> crate::storage::Result<FsckReport> {
    let initial = check_engine(engine)?;
    if !options.repair {
        return Ok(report_from_state(false, initial, Vec::new()));
    }

    let _guard = engine.write_lock.lock().unwrap();
    let repairs = repair_derived_indexes(engine, &initial)?;
    drop(_guard);

    let final_state = check_engine(engine)?;
    Ok(report_from_state(true, final_state, repairs))
}

fn report_from_state(repaired: bool, state: CheckState, repairs: Vec<FsckRepair>) -> FsckReport {
    FsckReport {
        ok: state.issues.is_empty(),
        repaired,
        checked: state.checked,
        issues: state.issues,
        repairs,
    }
}

fn check_engine(engine: &GraphEngine) -> crate::storage::Result<CheckState> {
    let snapshot = engine.db.snapshot();
    let keyspaces = &engine.keyspaces;
    let mut state = CheckState::default();

    for guard in snapshot.prefix(&keyspaces.graph_data, node_scan_prefix()) {
        state.checked.nodes += 1;
        let Ok((key, value)) = guard.into_inner() else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNode));
            continue;
        };
        let Some(node) = parse_node_key(key.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNode));
            continue;
        };
        let Some((_, flags)) = parse_node_value(value.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNode).with_node(node));
            continue;
        };
        if flags & KEY_FLAG_TOMBSTONE == 0 {
            state.live_nodes.insert(node);
        }
    }

    for guard in snapshot.prefix(&keyspaces.graph_data, node_label_scan_prefix()) {
        state.checked.node_labels += 1;
        let Ok((key, _)) = guard.into_inner() else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNodeLabel));
            continue;
        };
        let Some((node, label)) = parse_node_label_key(key.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNodeLabel));
            continue;
        };
        if state.live_nodes.contains(&node) {
            state.node_labels.entry(node).or_default().insert(label);
            state
                .expected_label_nodes
                .insert(label_node_key(label, node));
        } else {
            state.issues.push(
                FsckIssue::new(FsckIssueKind::OrphanNodeLabel)
                    .with_node(node)
                    .with_label(label),
            );
        }
    }

    for guard in snapshot.prefix(&keyspaces.graph_data, node_prop_scan_prefix()) {
        state.checked.node_props += 1;
        let Ok((key, value)) = guard.into_inner() else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNodeProperty));
            continue;
        };
        let Some((node, property_key)) = parse_node_prop_key(key.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNodeProperty));
            continue;
        };
        let Ok(property_value) = parse_prop_value(value.as_ref()) else {
            state.issues.push(
                FsckIssue::new(FsckIssueKind::MalformedNodeProperty)
                    .with_node(node)
                    .with_property_key(property_key),
            );
            continue;
        };
        if state.live_nodes.contains(&node) {
            state
                .node_props
                .entry(node)
                .or_default()
                .insert(property_key, property_value);
        } else {
            state.issues.push(
                FsckIssue::new(FsckIssueKind::OrphanNodeProperty)
                    .with_node(node)
                    .with_property_key(property_key),
            );
        }
    }

    for (node, labels) in &state.node_labels {
        let Some(props) = state.node_props.get(node) else {
            continue;
        };
        for label in labels {
            for (property_key, property_value) in props {
                if scalar_indexable_value(property_value) {
                    state.expected_node_prop_indexes.insert(node_prop_index_key(
                        *label,
                        property_key,
                        property_value,
                        *node,
                    ));
                }
            }
        }
    }

    for guard in snapshot.prefix(&keyspaces.graph_data, label_node_scan_prefix()) {
        state.checked.label_nodes += 1;
        let Ok((key, _)) = guard.into_inner() else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedLabelNode));
            continue;
        };
        let raw_key = key.as_ref().to_vec();
        state.all_label_node_keys.push(raw_key.clone());
        let Some((label, node)) = parse_label_node_key(&raw_key) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedLabelNode));
            continue;
        };
        state.actual_label_nodes.insert(raw_key);
        if !state.live_nodes.contains(&node)
            || !state
                .node_labels
                .get(&node)
                .is_some_and(|labels| labels.contains(&label))
        {
            state.issues.push(
                FsckIssue::new(FsckIssueKind::StaleLabelNodeIndex)
                    .with_node(node)
                    .with_label(label),
            );
        }
    }

    for key in state
        .expected_label_nodes
        .difference(&state.actual_label_nodes)
    {
        if let Some((label, node)) = parse_label_node_key(key) {
            state.issues.push(
                FsckIssue::new(FsckIssueKind::MissingLabelNodeIndex)
                    .with_node(node)
                    .with_label(label),
            );
        }
    }

    for guard in snapshot.prefix(&keyspaces.graph_data, node_prop_index_scan_prefix()) {
        state.checked.idx_node_props += 1;
        let Ok((key, _)) = guard.into_inner() else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNodePropertyIndex));
            continue;
        };
        let raw_key = key.as_ref().to_vec();
        state.all_node_prop_index_keys.push(raw_key.clone());
        let Some(entry) = parse_node_prop_index_key(&raw_key) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedNodePropertyIndex));
            continue;
        };
        state.actual_node_prop_indexes.insert(raw_key);
        let index_is_current = state.live_nodes.contains(&entry.node)
            && scalar_indexable_value(&entry.value)
            && state
                .node_labels
                .get(&entry.node)
                .is_some_and(|labels| labels.contains(&entry.label))
            && state
                .node_props
                .get(&entry.node)
                .and_then(|props| props.get(&entry.property_key))
                .is_some_and(|value| value == &entry.value);
        if !index_is_current {
            state.issues.push(
                FsckIssue::new(FsckIssueKind::StaleNodePropertyIndex)
                    .with_node(entry.node)
                    .with_label(entry.label)
                    .with_property_key(entry.property_key),
            );
        }
    }

    for key in state
        .expected_node_prop_indexes
        .difference(&state.actual_node_prop_indexes)
    {
        if let Some(entry) = parse_node_prop_index_key(key) {
            state.issues.push(
                FsckIssue::new(FsckIssueKind::MissingNodePropertyIndex)
                    .with_node(entry.node)
                    .with_label(entry.label)
                    .with_property_key(entry.property_key),
            );
        }
    }

    for guard in snapshot.prefix(&keyspaces.adj_out, adj_out_scan_prefix()) {
        state.checked.adj_out += 1;
        let Ok((key, value)) = guard.into_inner() else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedAdjOut));
            continue;
        };
        let Some((src, rel)) = parse_adj_out_key(key.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedAdjOut));
            continue;
        };
        let Some(dsts) = decode_adjacent_nodes(value.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedAdjOut));
            continue;
        };
        for dst in dsts {
            state.adj_out.insert(EdgeKey { src, rel, dst });
        }
    }

    for guard in snapshot.prefix(&keyspaces.adj_in, adj_in_scan_prefix()) {
        state.checked.adj_in += 1;
        let Ok((key, value)) = guard.into_inner() else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedAdjIn));
            continue;
        };
        let Some((dst, rel)) = parse_adj_in_key(key.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedAdjIn));
            continue;
        };
        let Some(srcs) = decode_adjacent_nodes(value.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedAdjIn));
            continue;
        };
        for src in srcs {
            state.adj_in.insert(EdgeKey { src, rel, dst });
        }
    }

    for edge in state.adj_out.difference(&state.adj_in) {
        state
            .issues
            .push(FsckIssue::new(FsckIssueKind::AdjacencyMismatch).with_edge(*edge));
    }
    for edge in state.adj_in.difference(&state.adj_out) {
        state
            .issues
            .push(FsckIssue::new(FsckIssueKind::AdjacencyMismatch).with_edge(*edge));
    }

    for guard in snapshot.prefix(&keyspaces.graph_data, edge_prop_scan_prefix()) {
        state.checked.edge_props += 1;
        let Ok((key, _)) = guard.into_inner() else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedEdgeProperty));
            continue;
        };
        let Some((edge, property_key)) = parse_edge_prop_key(key.as_ref()) else {
            state
                .issues
                .push(FsckIssue::new(FsckIssueKind::MalformedEdgeProperty));
            continue;
        };
        if !edge_is_visible(&state, edge) {
            state.issues.push(
                FsckIssue::new(FsckIssueKind::OrphanEdgeProperty)
                    .with_edge(edge)
                    .with_property_key(property_key),
            );
        }
    }

    Ok(state)
}

fn repair_derived_indexes(
    engine: &GraphEngine,
    state: &CheckState,
) -> crate::storage::Result<Vec<FsckRepair>> {
    let mut batch = engine.db.batch().durability(Some(PersistMode::SyncAll));
    for key in &state.all_label_node_keys {
        batch.remove(&engine.keyspaces.graph_data, key);
    }
    for key in &state.expected_label_nodes {
        batch.insert(&engine.keyspaces.graph_data, key, []);
    }
    for key in &state.all_node_prop_index_keys {
        batch.remove(&engine.keyspaces.graph_data, key);
    }
    for key in &state.expected_node_prop_indexes {
        batch.insert(&engine.keyspaces.graph_data, key, []);
    }
    batch.commit()?;

    Ok(vec![
        FsckRepair {
            kind: FsckRepairKind::RebuiltLabelNodes,
            removed: state.all_label_node_keys.len() as u64,
            inserted: state.expected_label_nodes.len() as u64,
        },
        FsckRepair {
            kind: FsckRepairKind::RebuiltNodePropertyIndex,
            removed: state.all_node_prop_index_keys.len() as u64,
            inserted: state.expected_node_prop_indexes.len() as u64,
        },
    ])
}

fn edge_is_visible(state: &CheckState, edge: EdgeKey) -> bool {
    state.live_nodes.contains(&edge.src)
        && state.live_nodes.contains(&edge.dst)
        && state.adj_out.contains(&edge)
        && state.adj_in.contains(&edge)
}

impl FsckIssue {
    fn new(kind: FsckIssueKind) -> Self {
        Self {
            kind,
            node: None,
            label: None,
            rel: None,
            dst: None,
            property_key: None,
        }
    }

    fn with_node(mut self, node: InternalNodeId) -> Self {
        self.node = Some(node);
        self
    }

    fn with_label(mut self, label: LabelId) -> Self {
        self.label = Some(label);
        self
    }

    fn with_edge(mut self, edge: EdgeKey) -> Self {
        self.node = Some(edge.src);
        self.rel = Some(edge.rel);
        self.dst = Some(edge.dst);
        self
    }

    fn with_property_key(mut self, property_key: String) -> Self {
        self.property_key = Some(property_key);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GraphSnapshot;
    use tempfile::tempdir;

    fn seed_indexed_node(path: &Path) -> (InternalNodeId, LabelId) {
        let engine = GraphEngine::open(path).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        let mut tx = engine.begin_write();
        let alice = tx.create_node(10, person).unwrap();
        tx.set_node_property(alice, "name".to_string(), "Alice".into())
            .unwrap();
        tx.commit().unwrap();
        (alice, person)
    }

    #[test]
    fn fsck_clean_database_returns_ok() {
        let dir = tempdir().unwrap();
        seed_indexed_node(dir.path());

        let report = fsck(dir.path(), FsckOptions { repair: false }).unwrap();

        assert!(report.ok);
        assert!(!report.repaired);
        assert!(report.issues.is_empty());
        assert_eq!(report.repairs.len(), 0);
        assert_eq!(report.checked.nodes, 1);
    }

    #[test]
    fn fsck_detects_and_repairs_missing_label_node_index() {
        let dir = tempdir().unwrap();
        let (alice, person) = seed_indexed_node(dir.path());
        {
            let engine = GraphEngine::open(dir.path()).unwrap();
            let mut batch = engine.db.batch().durability(Some(PersistMode::SyncAll));
            batch.remove(&engine.keyspaces.graph_data, label_node_key(person, alice));
            batch.commit().unwrap();
        }

        let broken = fsck(dir.path(), FsckOptions { repair: false }).unwrap();
        assert!(!broken.ok);
        assert!(broken.issues.iter().any(|issue| {
            issue.kind == FsckIssueKind::MissingLabelNodeIndex
                && issue.node == Some(alice)
                && issue.label == Some(person)
        }));

        let repaired = fsck(dir.path(), FsckOptions { repair: true }).unwrap();
        assert!(repaired.ok, "{:?}", repaired.issues);
        assert!(repaired.repaired);
        assert_eq!(
            GraphEngine::open(dir.path())
                .unwrap()
                .snapshot()
                .nodes_with_label(person)
                .collect::<Vec<_>>(),
            vec![alice]
        );
    }

    #[test]
    fn fsck_detects_and_repairs_stale_label_node_index() {
        let dir = tempdir().unwrap();
        seed_indexed_node(dir.path());
        {
            let engine = GraphEngine::open(dir.path()).unwrap();
            let mut batch = engine.db.batch().durability(Some(PersistMode::SyncAll));
            batch.insert(&engine.keyspaces.graph_data, label_node_key(99, 999), []);
            batch.commit().unwrap();
        }

        let broken = fsck(dir.path(), FsckOptions { repair: false }).unwrap();
        assert!(!broken.ok);
        assert!(
            broken
                .issues
                .iter()
                .any(|issue| issue.kind == FsckIssueKind::StaleLabelNodeIndex)
        );

        let repaired = fsck(dir.path(), FsckOptions { repair: true }).unwrap();
        assert!(repaired.ok, "{:?}", repaired.issues);
    }

    #[test]
    fn fsck_detects_and_repairs_missing_node_property_index() {
        let dir = tempdir().unwrap();
        let (alice, person) = seed_indexed_node(dir.path());
        {
            let engine = GraphEngine::open(dir.path()).unwrap();
            let key = node_prop_index_key(
                person,
                "name",
                &PropertyValue::String("Alice".to_string()),
                alice,
            );
            let mut batch = engine.db.batch().durability(Some(PersistMode::SyncAll));
            batch.remove(&engine.keyspaces.graph_data, key);
            batch.commit().unwrap();
        }

        let broken = fsck(dir.path(), FsckOptions { repair: false }).unwrap();
        assert!(!broken.ok);
        assert!(broken.issues.iter().any(|issue| {
            issue.kind == FsckIssueKind::MissingNodePropertyIndex
                && issue.node == Some(alice)
                && issue.label == Some(person)
                && issue.property_key.as_deref() == Some("name")
        }));

        let repaired = fsck(dir.path(), FsckOptions { repair: true }).unwrap();
        assert!(repaired.ok, "{:?}", repaired.issues);
    }

    #[test]
    fn fsck_detects_and_repairs_stale_node_property_index() {
        let dir = tempdir().unwrap();
        let (_, person) = seed_indexed_node(dir.path());
        {
            let engine = GraphEngine::open(dir.path()).unwrap();
            let stale_key =
                node_prop_index_key(person, "name", &PropertyValue::String("Ghost".into()), 999);
            let mut batch = engine.db.batch().durability(Some(PersistMode::SyncAll));
            batch.insert(&engine.keyspaces.graph_data, stale_key, []);
            batch.commit().unwrap();
        }

        let broken = fsck(dir.path(), FsckOptions { repair: false }).unwrap();
        assert!(!broken.ok);
        assert!(
            broken
                .issues
                .iter()
                .any(|issue| issue.kind == FsckIssueKind::StaleNodePropertyIndex)
        );

        let repaired = fsck(dir.path(), FsckOptions { repair: true }).unwrap();
        assert!(repaired.ok, "{:?}", repaired.issues);
    }

    #[test]
    fn fsck_reports_adjacency_and_orphan_props_without_repairing_them() {
        let dir = tempdir().unwrap();
        let engine = GraphEngine::open(dir.path()).unwrap();
        let person = engine.get_or_create_label("Person").unwrap();
        let knows = engine.get_or_create_rel_type("KNOWS").unwrap();
        let mut tx = engine.begin_write();
        let alice = tx.create_node(10, person).unwrap();
        let bob = tx.create_node(20, person).unwrap();
        tx.create_edge(alice, knows, bob).unwrap();
        tx.commit().unwrap();
        let edge = EdgeKey {
            src: alice,
            rel: knows,
            dst: bob,
        };
        {
            let mut batch = engine.db.batch().durability(Some(PersistMode::SyncAll));
            batch.remove(&engine.keyspaces.adj_in, adj_in_key(edge.dst, edge.rel));
            batch.insert(
                &engine.keyspaces.graph_data,
                edge_prop_key(edge, "since"),
                PropertyValue::Int(2024).encode(),
            );
            batch.insert(
                &engine.keyspaces.graph_data,
                node_prop_key(999, "name"),
                PropertyValue::String("Ghost".into()).encode(),
            );
            batch.commit().unwrap();
        }
        drop(engine);

        let broken = fsck(dir.path(), FsckOptions { repair: false }).unwrap();
        assert!(!broken.ok);
        assert!(
            broken
                .issues
                .iter()
                .any(|issue| issue.kind == FsckIssueKind::AdjacencyMismatch)
        );
        assert!(
            broken
                .issues
                .iter()
                .any(|issue| issue.kind == FsckIssueKind::OrphanEdgeProperty)
        );
        assert!(
            broken
                .issues
                .iter()
                .any(|issue| issue.kind == FsckIssueKind::OrphanNodeProperty)
        );

        let repaired = fsck(dir.path(), FsckOptions { repair: true }).unwrap();
        assert!(!repaired.ok);
        assert!(
            repaired
                .issues
                .iter()
                .any(|issue| issue.kind == FsckIssueKind::AdjacencyMismatch)
        );
        assert!(
            repaired
                .issues
                .iter()
                .any(|issue| issue.kind == FsckIssueKind::OrphanEdgeProperty)
        );
        assert!(
            repaired
                .issues
                .iter()
                .any(|issue| issue.kind == FsckIssueKind::OrphanNodeProperty)
        );
    }

    #[test]
    fn fsck_json_keeps_stable_top_level_fields() {
        let dir = tempdir().unwrap();
        seed_indexed_node(dir.path());

        let report = fsck(dir.path(), FsckOptions { repair: false }).unwrap();
        let json = serde_json::to_value(report).unwrap();

        assert!(json.get("ok").is_some());
        assert!(json.get("repaired").is_some());
        assert!(json.get("checked").is_some());
        assert!(json.get("issues").is_some());
        assert!(json.get("repairs").is_some());
    }
}
