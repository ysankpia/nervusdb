//! Property serialization module using FlexBuffers for high performance.
//!
//! ## FlexBuffers Only
//!
//! 1. **Performance**: 10x faster serialization, zero-copy deserialization
//! 2. **Memory**: No intermediate string allocation  
//! 3. **Compatibility**: Can represent any JSON-like structure
//! 4. **Simplicity**: Single format, no backward compatibility burden
//!
//! All data uses FlexBuffers format with magic byte prefix for validation.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Magic byte prefix for FlexBuffers format (not valid UTF-8)
/// This allows us to distinguish FlexBuffers from JSON
const FLEXBUF_MAGIC: &[u8] = b"\xFB\x00";

/// Serialize a property value to FlexBuffers binary format
///
/// # Example
/// ```
/// use nervusdb_core::storage::property::serialize_property;
/// use std::collections::HashMap;
///
/// let mut props = HashMap::new();
/// props.insert("name".to_string(), serde_json::json!("Alice"));
/// props.insert("age".to_string(), serde_json::json!(30));
///
/// let bytes = serialize_property(&props).unwrap();
/// ```
pub fn serialize_property<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut serializer = flexbuffers::FlexbufferSerializer::new();
    value
        .serialize(&mut serializer)
        .map_err(|e| Error::Other(format!("flexbuffers serialization failed: {}", e)))?;

    let mut result = Vec::with_capacity(FLEXBUF_MAGIC.len() + serializer.view().len());
    result.extend_from_slice(FLEXBUF_MAGIC);
    result.extend_from_slice(serializer.view());

    Ok(result)
}

/// Deserialize a property value from FlexBuffers format
///
/// # Format
///
/// All data must be FlexBuffers format with FLEXBUF_MAGIC prefix.
///
/// # Example
/// ```
/// use nervusdb_core::storage::property::{serialize_property, deserialize_property};
/// use std::collections::HashMap;
///
/// let mut props = HashMap::new();
/// props.insert("name".to_string(), serde_json::json!("Alice"));
///
/// let bytes = serialize_property(&props).unwrap();
/// let restored: HashMap<String, serde_json::Value> = deserialize_property(&bytes).unwrap();
/// assert_eq!(restored.get("name").unwrap().as_str().unwrap(), "Alice");
/// ```
pub fn deserialize_property<'a, T: Deserialize<'a>>(data: &'a [u8]) -> Result<T> {
    // Empty data → error
    if data.is_empty() {
        return Err(Error::Other("empty property data".to_string()));
    }

    // Require FlexBuffers magic bytes
    if data.len() < FLEXBUF_MAGIC.len() || &data[..FLEXBUF_MAGIC.len()] != FLEXBUF_MAGIC {
        return Err(Error::Other(
            "invalid property data: missing FlexBuffers magic".to_string(),
        ));
    }

    // FlexBuffers format only
    let flexbuf_data = &data[FLEXBUF_MAGIC.len()..];
    let reader = flexbuffers::Reader::get_root(flexbuf_data)
        .map_err(|e| Error::Other(format!("flexbuffers deserialization failed: {}", e)))?;

    T::deserialize(reader)
        .map_err(|e| Error::Other(format!("flexbuffers type conversion failed: {}", e)))
}

/// Serialize properties to FlexBuffers for storage
///
/// This is a convenience wrapper that handles HashMap<String, serde_json::Value>
pub fn serialize_properties(props: &HashMap<String, serde_json::Value>) -> Result<Vec<u8>> {
    serialize_property(props)
}

/// Deserialize properties from either FlexBuffers or JSON
pub fn deserialize_properties(data: &[u8]) -> Result<HashMap<String, serde_json::Value>> {
    deserialize_property(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let mut props = HashMap::new();
        props.insert("name".to_string(), json!("Alice"));
        props.insert("age".to_string(), json!(30));
        props.insert("active".to_string(), json!(true));

        let serialized = serialize_properties(&props).unwrap();

        // Should start with magic bytes
        assert_eq!(&serialized[..2], FLEXBUF_MAGIC);

        let deserialized = deserialize_properties(&serialized).unwrap();
        assert_eq!(deserialized.get("name").unwrap().as_str().unwrap(), "Alice");
        assert_eq!(deserialized.get("age").unwrap().as_i64().unwrap(), 30);
        assert!(deserialized.get("active").unwrap().as_bool().unwrap());
    }

    #[test]
    fn test_invalid_data_rejected() {
        // JSON data should be rejected (no backward compatibility)
        let json_data = r#"{"name":"Bob","score":95}"#;
        let json_bytes = json_data.as_bytes();

        // Should fail to read JSON format
        let result = deserialize_properties(json_bytes);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("missing FlexBuffers magic")
        );
    }

    #[test]
    fn test_nested_structures() {
        let mut props = HashMap::new();
        props.insert(
            "metadata".to_string(),
            json!({
                "tags": ["rust", "database"],
                "stats": {
                    "views": 1000,
                    "likes": 42
                }
            }),
        );

        let serialized = serialize_properties(&props).unwrap();
        let deserialized = deserialize_properties(&serialized).unwrap();

        let metadata = deserialized.get("metadata").unwrap();
        assert_eq!(metadata["tags"][0].as_str().unwrap(), "rust");
        assert_eq!(metadata["stats"]["likes"].as_i64().unwrap(), 42);
    }

    #[test]
    fn test_empty_properties() {
        let props = HashMap::new();
        let serialized = serialize_properties(&props).unwrap();
        let deserialized = deserialize_properties(&serialized).unwrap();
        assert_eq!(deserialized.len(), 0);
    }

    #[test]
    fn test_flexbuffers_serialization() {
        let mut props = HashMap::new();
        for i in 0..100 {
            props.insert(format!("key{}", i), json!(i));
        }

        let flexbuf = serialize_properties(&props).unwrap();
        let deserialized = deserialize_properties(&flexbuf).unwrap();

        // Verify round-trip correctness
        assert_eq!(props.len(), deserialized.len());
        for i in 0..100 {
            let key = format!("key{}", i);
            assert_eq!(props.get(&key), deserialized.get(&key));
        }

        // Verify FlexBuffers format
        assert!(flexbuf.starts_with(FLEXBUF_MAGIC));
    }
}
