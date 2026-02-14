use crate::property::PropertyValue;
use crate::snapshot::EdgeKey;
use std::collections::BTreeMap;

pub(crate) fn convert_property_to_api(value: PropertyValue) -> nervusdb_api::PropertyValue {
    value
}

pub(crate) fn convert_property_map_to_api(
    props: BTreeMap<String, PropertyValue>,
) -> BTreeMap<String, nervusdb_api::PropertyValue> {
    props
}

pub(crate) fn api_edge_to_internal(edge: nervusdb_api::EdgeKey) -> EdgeKey {
    edge
}

pub(crate) fn internal_edge_to_api(edge: EdgeKey) -> nervusdb_api::EdgeKey {
    edge
}

pub(crate) fn convert_property_to_storage(
    value: nervusdb_api::PropertyValue,
) -> crate::property::PropertyValue {
    value
}

#[cfg(test)]
mod tests {
    use super::{
        api_edge_to_internal, convert_property_map_to_api, convert_property_to_api,
        convert_property_to_storage, internal_edge_to_api,
    };
    use crate::property::PropertyValue;
    use std::collections::BTreeMap;

    #[test]
    fn convert_property_to_api_is_identity_after_type_unification() {
        let value = PropertyValue::List(vec![
            PropertyValue::String("root".to_string()),
            PropertyValue::Map(BTreeMap::from([("x".to_string(), PropertyValue::Int(7))])),
        ]);

        let converted = convert_property_to_api(value.clone());
        assert_eq!(converted, value);
    }

    #[test]
    fn convert_property_map_to_api_is_identity_after_type_unification() {
        let props = BTreeMap::from([
            ("n".to_string(), PropertyValue::Int(42)),
            ("s".to_string(), PropertyValue::String("ok".to_string())),
        ]);

        let converted = convert_property_map_to_api(props.clone());
        assert_eq!(converted, props);
    }

    #[test]
    fn edge_converters_are_identity_after_edge_key_unification() {
        let edge = nervusdb_api::EdgeKey {
            src: 11,
            rel: 22,
            dst: 33,
        };
        assert_eq!(api_edge_to_internal(edge), edge);
        assert_eq!(internal_edge_to_api(edge), edge);
    }

    #[test]
    fn convert_property_to_storage_is_identity_after_type_unification() {
        let value = nervusdb_api::PropertyValue::Map(BTreeMap::from([(
            "k".to_string(),
            nervusdb_api::PropertyValue::List(vec![
                nervusdb_api::PropertyValue::Int(7),
                nervusdb_api::PropertyValue::Null,
            ]),
        )]));

        let got = convert_property_to_storage(value.clone());
        assert_eq!(got, value);
    }
}
