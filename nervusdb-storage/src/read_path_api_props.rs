use crate::idmap::InternalNodeId;
use crate::read_path_convert::{
    api_edge_to_internal, convert_property_map_to_api, convert_property_to_api,
};
use crate::snapshot::Snapshot;
use std::collections::BTreeMap;

pub(crate) fn node_property_as_api(
    snapshot: &Snapshot,
    iid: InternalNodeId,
    key: &str,
) -> Option<nervusdb_api::PropertyValue> {
    snapshot
        .node_property(iid, key)
        .map(convert_property_to_api)
}

pub(crate) fn edge_property_as_api(
    snapshot: &Snapshot,
    edge: nervusdb_api::EdgeKey,
    key: &str,
) -> Option<nervusdb_api::PropertyValue> {
    let internal_edge = api_edge_to_internal(edge);
    snapshot
        .edge_property(internal_edge, key)
        .map(convert_property_to_api)
}

pub(crate) fn node_properties_as_api(
    snapshot: &Snapshot,
    iid: InternalNodeId,
) -> Option<BTreeMap<String, nervusdb_api::PropertyValue>> {
    snapshot
        .node_properties(iid)
        .map(convert_property_map_to_api)
}

pub(crate) fn edge_properties_as_api(
    snapshot: &Snapshot,
    edge: nervusdb_api::EdgeKey,
) -> Option<BTreeMap<String, nervusdb_api::PropertyValue>> {
    let internal_edge = api_edge_to_internal(edge);
    snapshot
        .edge_properties(internal_edge)
        .map(convert_property_map_to_api)
}

#[cfg(test)]
mod tests {
    use super::{
        edge_properties_as_api, edge_property_as_api, node_properties_as_api, node_property_as_api,
    };
    use crate::label_interner::LabelSnapshot;
    use crate::property::PropertyValue;
    use crate::snapshot::{EdgeKey, L0Run, Snapshot};
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use std::sync::Arc;

    fn sample_snapshot() -> Snapshot {
        let edge = EdgeKey {
            src: 1,
            rel: 2,
            dst: 3,
        };
        let run = Arc::new(L0Run::new(
            1,
            BTreeMap::from([(1, vec![edge])]),
            BTreeMap::from([(3, vec![edge])]),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeMap::from([(
                1,
                BTreeMap::from([("age".to_string(), PropertyValue::Int(42))]),
            )]),
            BTreeMap::from([(
                edge,
                BTreeMap::from([("active".to_string(), PropertyValue::Bool(true))]),
            )]),
            BTreeMap::new(),
            BTreeMap::new(),
        ));

        Snapshot::new(
            Arc::new(vec![run]),
            Arc::new(Vec::new()),
            Arc::new(LabelSnapshot::new(HashMap::new(), Vec::new())),
            Arc::new(vec![Vec::new(); 4]),
            0,
            0,
        )
    }

    #[test]
    fn node_property_helpers_convert_scalar_and_map_paths() {
        let snapshot = sample_snapshot();
        let val = node_property_as_api(&snapshot, 1, "age");
        assert_eq!(val, Some(nervusdb_api::PropertyValue::Int(42)));

        let map = node_properties_as_api(&snapshot, 1).expect("node property map");
        assert_eq!(map.get("age"), Some(&nervusdb_api::PropertyValue::Int(42)));
    }

    #[test]
    fn edge_property_helpers_convert_scalar_and_map_paths() {
        let snapshot = sample_snapshot();
        let edge = nervusdb_api::EdgeKey {
            src: 1,
            rel: 2,
            dst: 3,
        };

        let val = edge_property_as_api(&snapshot, edge, "active");
        assert_eq!(val, Some(nervusdb_api::PropertyValue::Bool(true)));

        let map = edge_properties_as_api(&snapshot, edge).expect("edge property map");
        assert_eq!(
            map.get("active"),
            Some(&nervusdb_api::PropertyValue::Bool(true))
        );
    }
}
