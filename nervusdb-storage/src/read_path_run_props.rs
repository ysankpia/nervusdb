use crate::idmap::InternalNodeId;
use crate::property::PropertyValue;
use crate::snapshot::{EdgeKey, L0Run};

pub(crate) fn node_property_in_run<'a>(
    run: &'a L0Run,
    node: InternalNodeId,
    key: &str,
) -> Option<&'a PropertyValue> {
    if let Some(deleted) = run.tombstoned_node_properties.get(&node)
        && deleted.contains(key)
    {
        return None;
    }
    run.node_properties
        .get(&node)
        .and_then(|props| props.get(key))
}

pub(crate) fn edge_property_in_run<'a>(
    run: &'a L0Run,
    edge: EdgeKey,
    key: &str,
) -> Option<&'a PropertyValue> {
    if let Some(deleted) = run.tombstoned_edge_properties.get(&edge)
        && deleted.contains(key)
    {
        return None;
    }
    run.edge_properties
        .get(&edge)
        .and_then(|props| props.get(key))
}

#[cfg(test)]
mod tests {
    use super::{edge_property_in_run, node_property_in_run};
    use crate::property::PropertyValue;
    use crate::snapshot::{EdgeKey, L0Run};
    use std::collections::{BTreeMap, BTreeSet};

    fn build_run(
        node_props: BTreeMap<u32, BTreeMap<String, PropertyValue>>,
        edge_props: BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
        tombstoned_node_props: BTreeMap<u32, BTreeSet<String>>,
        tombstoned_edge_props: BTreeMap<EdgeKey, BTreeSet<String>>,
    ) -> L0Run {
        L0Run::new(
            1,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            node_props,
            edge_props,
            tombstoned_node_props,
            tombstoned_edge_props,
        )
    }

    #[test]
    fn node_property_in_run_honors_tombstone_over_value() {
        let run = build_run(
            BTreeMap::from([(
                1,
                BTreeMap::from([(
                    "name".to_string(),
                    PropertyValue::String("alice".to_string()),
                )]),
            )]),
            BTreeMap::new(),
            BTreeMap::from([(1, BTreeSet::from(["name".to_string()]))]),
            BTreeMap::new(),
        );

        assert_eq!(node_property_in_run(&run, 1, "name"), None);
    }

    #[test]
    fn edge_property_in_run_reads_live_value() {
        let edge = EdgeKey {
            src: 1,
            rel: 2,
            dst: 3,
        };
        let run = build_run(
            BTreeMap::new(),
            BTreeMap::from([(
                edge,
                BTreeMap::from([("weight".to_string(), PropertyValue::Int(9))]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
        );

        assert_eq!(
            edge_property_in_run(&run, edge, "weight"),
            Some(&PropertyValue::Int(9))
        );
    }
}
