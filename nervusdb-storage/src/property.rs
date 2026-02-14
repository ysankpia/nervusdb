pub use nervusdb_api::{DecodeError, PropertyValue};

#[cfg(test)]
mod tests {
    use super::PropertyValue;
    use std::collections::BTreeMap;

    #[test]
    fn storage_property_roundtrip_still_available_via_api_type() {
        let value = PropertyValue::Map(BTreeMap::from([(
            "k".to_string(),
            PropertyValue::List(vec![PropertyValue::Int(1), PropertyValue::Bool(false)]),
        )]));

        let encoded = value.encode();
        let decoded = PropertyValue::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded, value);
    }

    #[test]
    fn storage_property_as_float_is_preserved() {
        assert_eq!(PropertyValue::Float(2.5).as_float(), Some(2.5));
        assert_eq!(PropertyValue::String("x".to_string()).as_float(), None);
    }
}
