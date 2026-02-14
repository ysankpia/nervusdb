use super::{PropertyValue, Value};
use std::collections::BTreeMap;

pub(super) fn merge_props_to_values(
    props: &BTreeMap<String, PropertyValue>,
) -> BTreeMap<String, Value> {
    props
        .iter()
        .map(|(k, v)| (k.clone(), merge_storage_property_to_value(v)))
        .collect()
}

pub(super) fn merge_storage_property_to_api(v: &PropertyValue) -> nervusdb_api::PropertyValue {
    match v {
        PropertyValue::Null => nervusdb_api::PropertyValue::Null,
        PropertyValue::Bool(b) => nervusdb_api::PropertyValue::Bool(*b),
        PropertyValue::Int(i) => nervusdb_api::PropertyValue::Int(*i),
        PropertyValue::Float(f) => nervusdb_api::PropertyValue::Float(*f),
        PropertyValue::String(s) => nervusdb_api::PropertyValue::String(s.clone()),
        PropertyValue::DateTime(i) => nervusdb_api::PropertyValue::DateTime(*i),
        PropertyValue::Blob(b) => nervusdb_api::PropertyValue::Blob(b.clone()),
        PropertyValue::List(l) => {
            nervusdb_api::PropertyValue::List(l.iter().map(merge_storage_property_to_api).collect())
        }
        PropertyValue::Map(m) => nervusdb_api::PropertyValue::Map(
            m.iter()
                .map(|(k, vv)| (k.clone(), merge_storage_property_to_api(vv)))
                .collect(),
        ),
    }
}

fn api_property_to_storage(v: &nervusdb_api::PropertyValue) -> PropertyValue {
    match v {
        nervusdb_api::PropertyValue::Null => PropertyValue::Null,
        nervusdb_api::PropertyValue::Bool(b) => PropertyValue::Bool(*b),
        nervusdb_api::PropertyValue::Int(i) => PropertyValue::Int(*i),
        nervusdb_api::PropertyValue::Float(f) => PropertyValue::Float(*f),
        nervusdb_api::PropertyValue::String(s) => PropertyValue::String(s.clone()),
        nervusdb_api::PropertyValue::DateTime(i) => PropertyValue::DateTime(*i),
        nervusdb_api::PropertyValue::Blob(b) => PropertyValue::Blob(b.clone()),
        nervusdb_api::PropertyValue::List(l) => {
            PropertyValue::List(l.iter().map(api_property_to_storage).collect())
        }
        nervusdb_api::PropertyValue::Map(m) => PropertyValue::Map(
            m.iter()
                .map(|(k, vv)| (k.clone(), api_property_to_storage(vv)))
                .collect(),
        ),
    }
}

pub(super) fn api_property_map_to_storage(
    map: &BTreeMap<String, nervusdb_api::PropertyValue>,
) -> BTreeMap<String, PropertyValue> {
    map.iter()
        .map(|(k, v)| (k.clone(), api_property_to_storage(v)))
        .collect()
}

pub(super) fn merge_storage_property_to_value(v: &PropertyValue) -> Value {
    match v {
        PropertyValue::Null => Value::Null,
        PropertyValue::Bool(b) => Value::Bool(*b),
        PropertyValue::Int(i) => Value::Int(*i),
        PropertyValue::Float(f) => Value::Float(*f),
        PropertyValue::String(s) => Value::String(s.clone()),
        PropertyValue::DateTime(i) => Value::DateTime(*i),
        PropertyValue::Blob(b) => Value::Blob(b.clone()),
        PropertyValue::List(l) => {
            Value::List(l.iter().map(merge_storage_property_to_value).collect())
        }
        PropertyValue::Map(m) => Value::Map(
            m.iter()
                .map(|(k, vv)| (k.clone(), merge_storage_property_to_value(vv)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        api_property_map_to_storage, merge_props_to_values, merge_storage_property_to_api,
        merge_storage_property_to_value,
    };
    use crate::executor::{PropertyValue, Value};
    use std::collections::BTreeMap;

    #[test]
    fn storage_property_to_value_recurses_map_and_list() {
        let mut inner = BTreeMap::new();
        inner.insert("k".to_string(), PropertyValue::Int(7));
        let prop = PropertyValue::List(vec![
            PropertyValue::Bool(true),
            PropertyValue::Map(inner.clone()),
        ]);

        let converted = merge_storage_property_to_value(&prop);
        assert_eq!(
            converted,
            Value::List(vec![
                Value::Bool(true),
                Value::Map(BTreeMap::from([("k".to_string(), Value::Int(7))])),
            ])
        );

        let api = merge_storage_property_to_api(&PropertyValue::Map(inner));
        assert_eq!(
            api,
            nervusdb_api::PropertyValue::Map(BTreeMap::from([(
                "k".to_string(),
                nervusdb_api::PropertyValue::Int(7),
            )]))
        );
    }

    #[test]
    fn api_map_to_storage_and_props_to_values_roundtrip_shape() {
        let api_props = BTreeMap::from([(
            "m".to_string(),
            nervusdb_api::PropertyValue::Map(BTreeMap::from([(
                "x".to_string(),
                nervusdb_api::PropertyValue::List(vec![
                    nervusdb_api::PropertyValue::String("ok".to_string()),
                    nervusdb_api::PropertyValue::Int(1),
                ]),
            )])),
        )]);

        let storage = api_property_map_to_storage(&api_props);
        let projected = merge_props_to_values(&storage);

        assert_eq!(
            projected,
            BTreeMap::from([(
                "m".to_string(),
                Value::Map(BTreeMap::from([(
                    "x".to_string(),
                    Value::List(vec![Value::String("ok".to_string()), Value::Int(1)]),
                )])),
            )])
        );
    }
}
