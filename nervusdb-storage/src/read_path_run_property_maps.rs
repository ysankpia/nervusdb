use crate::idmap::InternalNodeId;
use crate::property::PropertyValue;
use crate::snapshot::EdgeKey;
use std::collections::BTreeMap;

pub(crate) fn node_properties_in_run<'a>(
    node_properties: &'a BTreeMap<InternalNodeId, BTreeMap<String, PropertyValue>>,
    node: InternalNodeId,
) -> Option<&'a BTreeMap<String, PropertyValue>> {
    node_properties.get(&node)
}

pub(crate) fn edge_properties_in_run<'a>(
    edge_properties: &'a BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
    edge: EdgeKey,
) -> Option<&'a BTreeMap<String, PropertyValue>> {
    edge_properties.get(&edge)
}

#[cfg(test)]
mod tests {
    use super::{edge_properties_in_run, node_properties_in_run};
    use crate::property::PropertyValue;
    use crate::snapshot::EdgeKey;
    use std::collections::BTreeMap;

    #[test]
    fn node_properties_helper_reads_bucket_or_none() {
        let node_properties = BTreeMap::from([(
            1,
            BTreeMap::from([(
                "name".to_string(),
                PropertyValue::String("alice".to_string()),
            )]),
        )]);

        assert!(node_properties_in_run(&node_properties, 1).is_some());
        assert!(node_properties_in_run(&node_properties, 2).is_none());
    }

    #[test]
    fn edge_properties_helper_reads_bucket_or_none() {
        let edge = EdgeKey {
            src: 1,
            rel: 2,
            dst: 3,
        };
        let edge_properties = BTreeMap::from([(
            edge,
            BTreeMap::from([("weight".to_string(), PropertyValue::Int(7))]),
        )]);

        assert!(edge_properties_in_run(&edge_properties, edge).is_some());
        assert!(
            edge_properties_in_run(
                &edge_properties,
                EdgeKey {
                    src: 9,
                    rel: 9,
                    dst: 9
                }
            )
            .is_none()
        );
    }
}
