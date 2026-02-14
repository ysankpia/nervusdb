use crate::idmap::InternalNodeId;
use crate::property::PropertyValue;
use crate::snapshot::{EdgeKey, L0Run};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(crate) fn node_property_from_runs(
    runs: &Arc<Vec<Arc<L0Run>>>,
    node: InternalNodeId,
    key: &str,
) -> Option<PropertyValue> {
    for run in runs.iter() {
        if let Some(deleted) = run.tombstoned_node_properties.get(&node)
            && deleted.contains(key)
        {
            return None;
        }
        if let Some(value) = run.node_property(node, key) {
            return Some(value.clone());
        }
    }
    None
}

pub(crate) fn edge_property_from_runs(
    runs: &Arc<Vec<Arc<L0Run>>>,
    edge: EdgeKey,
    key: &str,
) -> Option<PropertyValue> {
    for run in runs.iter() {
        if let Some(deleted) = run.tombstoned_edge_properties.get(&edge)
            && deleted.contains(key)
        {
            return None;
        }
        if let Some(value) = run.edge_property(edge, key) {
            return Some(value.clone());
        }
    }
    None
}

pub(crate) fn merge_node_properties_from_runs(
    runs: &Arc<Vec<Arc<L0Run>>>,
    node: InternalNodeId,
) -> Option<BTreeMap<String, PropertyValue>> {
    let mut merged = BTreeMap::new();
    let mut resolved = BTreeSet::new();

    for run in runs.iter() {
        if let Some(deleted_keys) = run.tombstoned_node_properties.get(&node) {
            for key in deleted_keys {
                resolved.insert(key.clone());
            }
        }

        if let Some(props) = run.node_properties(node) {
            for (key, value) in props {
                if resolved.insert(key.clone()) {
                    merged.insert(key.clone(), value.clone());
                }
            }
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

pub(crate) fn merge_edge_properties_from_runs(
    runs: &Arc<Vec<Arc<L0Run>>>,
    edge: EdgeKey,
) -> Option<BTreeMap<String, PropertyValue>> {
    let mut merged = BTreeMap::new();
    let mut resolved = BTreeSet::new();

    for run in runs.iter() {
        if let Some(deleted_keys) = run.tombstoned_edge_properties.get(&edge) {
            for key in deleted_keys {
                resolved.insert(key.clone());
            }
        }

        if let Some(props) = run.edge_properties(edge) {
            for (key, value) in props {
                if resolved.insert(key.clone()) {
                    merged.insert(key.clone(), value.clone());
                }
            }
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        edge_property_from_runs, merge_edge_properties_from_runs, merge_node_properties_from_runs,
        node_property_from_runs,
    };
    use crate::property::PropertyValue;
    use crate::snapshot::{EdgeKey, L0Run};
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    fn run(
        txid: u64,
        node_props: BTreeMap<u32, BTreeMap<String, PropertyValue>>,
        edge_props: BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
        tombstoned_node_props: BTreeMap<u32, BTreeSet<String>>,
        tombstoned_edge_props: BTreeMap<EdgeKey, BTreeSet<String>>,
    ) -> Arc<L0Run> {
        Arc::new(L0Run::new(
            txid,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            node_props,
            edge_props,
            tombstoned_node_props,
            tombstoned_edge_props,
        ))
    }

    #[test]
    fn node_property_prefers_newest_and_honors_tombstone() {
        let mut old_props = BTreeMap::new();
        old_props.insert(
            1,
            BTreeMap::from([("name".to_string(), PropertyValue::String("old".to_string()))]),
        );
        let old_run = run(
            1,
            old_props,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        let mut new_tomb = BTreeMap::new();
        new_tomb.insert(1, BTreeSet::from(["name".to_string()]));
        let new_run = run(
            2,
            BTreeMap::new(),
            BTreeMap::new(),
            new_tomb,
            BTreeMap::new(),
        );

        let runs = Arc::new(vec![new_run, old_run]);
        assert_eq!(node_property_from_runs(&runs, 1, "name"), None);
    }

    #[test]
    fn merge_node_properties_keeps_newest_values() {
        let old_run = run(
            1,
            BTreeMap::from([(
                1,
                BTreeMap::from([
                    ("a".to_string(), PropertyValue::Int(1)),
                    ("b".to_string(), PropertyValue::Int(2)),
                ]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        let new_run = run(
            2,
            BTreeMap::from([(
                1,
                BTreeMap::from([("a".to_string(), PropertyValue::Int(9))]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        let runs = Arc::new(vec![new_run, old_run]);
        let merged = merge_node_properties_from_runs(&runs, 1).expect("merged node props");
        assert_eq!(merged.get("a"), Some(&PropertyValue::Int(9)));
        assert_eq!(merged.get("b"), Some(&PropertyValue::Int(2)));
    }

    #[test]
    fn edge_property_prefers_newest_and_honors_tombstone() {
        let edge = EdgeKey {
            src: 1,
            rel: 2,
            dst: 3,
        };

        let old_run = run(
            1,
            BTreeMap::new(),
            BTreeMap::from([(
                edge,
                BTreeMap::from([("k".to_string(), PropertyValue::String("v".to_string()))]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        let new_run = run(
            2,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::from([(edge, BTreeSet::from(["k".to_string()]))]),
        );

        let runs = Arc::new(vec![new_run, old_run]);
        assert_eq!(edge_property_from_runs(&runs, edge, "k"), None);
    }

    #[test]
    fn merge_edge_properties_keeps_newest_values() {
        let edge = EdgeKey {
            src: 5,
            rel: 6,
            dst: 7,
        };

        let old_run = run(
            1,
            BTreeMap::new(),
            BTreeMap::from([(
                edge,
                BTreeMap::from([
                    ("x".to_string(), PropertyValue::Int(1)),
                    ("y".to_string(), PropertyValue::Int(2)),
                ]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        let new_run = run(
            2,
            BTreeMap::new(),
            BTreeMap::from([(
                edge,
                BTreeMap::from([("x".to_string(), PropertyValue::Int(3))]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        let runs = Arc::new(vec![new_run, old_run]);
        let merged = merge_edge_properties_from_runs(&runs, edge).expect("merged edge props");
        assert_eq!(merged.get("x"), Some(&PropertyValue::Int(3)));
        assert_eq!(merged.get("y"), Some(&PropertyValue::Int(2)));
    }

    #[test]
    fn merge_node_properties_honors_tombstone_from_newer_run() {
        let old_run = run(
            1,
            BTreeMap::from([(
                1,
                BTreeMap::from([
                    ("a".to_string(), PropertyValue::Int(1)),
                    ("b".to_string(), PropertyValue::Int(2)),
                ]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        let new_run = run(
            2,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::from([(1, BTreeSet::from(["a".to_string()]))]),
            BTreeMap::new(),
        );

        let runs = Arc::new(vec![new_run, old_run]);
        let merged = merge_node_properties_from_runs(&runs, 1).expect("merged node props");
        assert!(!merged.contains_key("a"));
        assert_eq!(merged.get("b"), Some(&PropertyValue::Int(2)));
    }

    #[test]
    fn merge_edge_properties_honors_tombstone_from_newer_run() {
        let edge = EdgeKey {
            src: 8,
            rel: 9,
            dst: 10,
        };

        let old_run = run(
            1,
            BTreeMap::new(),
            BTreeMap::from([(
                edge,
                BTreeMap::from([
                    ("x".to_string(), PropertyValue::Int(1)),
                    ("y".to_string(), PropertyValue::Int(2)),
                ]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        let new_run = run(
            2,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::from([(edge, BTreeSet::from(["x".to_string()]))]),
        );

        let runs = Arc::new(vec![new_run, old_run]);
        let merged = merge_edge_properties_from_runs(&runs, edge).expect("merged edge props");
        assert!(!merged.contains_key("x"));
        assert_eq!(merged.get("y"), Some(&PropertyValue::Int(2)));
    }
}
