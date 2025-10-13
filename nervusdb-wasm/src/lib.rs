use wasm_bindgen::prelude::*;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Triple representation in NervusDB
#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct Triple {
    subject: String,
    predicate: String,
    object: String,
}

#[wasm_bindgen]
impl Triple {
    #[wasm_bindgen(constructor)]
    pub fn new(subject: String, predicate: String, object: String) -> Triple {
        Triple {
            subject,
            predicate,
            object,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn subject(&self) -> String {
        self.subject.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn predicate(&self) -> String {
        self.predicate.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn object(&self) -> String {
        self.object.clone()
    }
}

/// Core storage engine for NervusDB (WASM version)
/// 
/// This is a simplified in-memory implementation.
/// Future versions will add:
/// - B-Tree indexing
/// - LSM Tree for persistence
/// - Write-Ahead Log (WAL)
#[wasm_bindgen]
pub struct StorageEngine {
    // In-memory storage: key -> serialized triple
    data: HashMap<String, Vec<u8>>,
    // Statistics
    insert_count: u64,
    query_count: u64,
}

#[wasm_bindgen]
impl StorageEngine {
    /// Create a new storage engine
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<StorageEngine, JsValue> {
        Ok(StorageEngine {
            data: HashMap::new(),
            insert_count: 0,
            query_count: 0,
        })
    }

    /// Insert a triple
    #[wasm_bindgen]
    pub fn insert(&mut self, subject: &str, predicate: &str, object: &str) -> Result<(), JsValue> {
        let key = format!("{}:{}:{}", subject, predicate, object);
        let triple = Triple {
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
        };

        let serialized = serde_json::to_vec(&triple)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

        self.data.insert(key, serialized);
        self.insert_count += 1;

        Ok(())
    }

    /// Query triples by subject
    #[wasm_bindgen]
    pub fn query_by_subject(&mut self, subject: &str) -> Result<JsValue, JsValue> {
        self.query_count += 1;

        let results: Vec<Triple> = self
            .data
            .iter()
            .filter_map(|(_, value)| {
                if let Ok(triple) = serde_json::from_slice::<Triple>(value) {
                    if triple.subject == subject {
                        Some(triple)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        serde_wasm_bindgen::to_value(&results)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Query triples by predicate
    #[wasm_bindgen]
    pub fn query_by_predicate(&mut self, predicate: &str) -> Result<JsValue, JsValue> {
        self.query_count += 1;

        let results: Vec<Triple> = self
            .data
            .iter()
            .filter_map(|(_, value)| {
                if let Ok(triple) = serde_json::from_slice::<Triple>(value) {
                    if triple.predicate == predicate {
                        Some(triple)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        serde_wasm_bindgen::to_value(&results)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Get statistics
    #[wasm_bindgen]
    pub fn get_stats(&self) -> Result<JsValue, JsValue> {
        let stats = serde_json::json!({
            "total_triples": self.data.len(),
            "insert_count": self.insert_count,
            "query_count": self.query_count,
        });

        serde_wasm_bindgen::to_value(&stats)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Clear all data
    #[wasm_bindgen]
    pub fn clear(&mut self) {
        self.data.clear();
        self.insert_count = 0;
        self.query_count = 0;
    }

    /// Get total number of triples
    #[wasm_bindgen]
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

// Add serde_wasm_bindgen for better serialization
use serde_wasm_bindgen;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_engine() {
        let mut engine = StorageEngine::new().unwrap();
        
        engine.insert("Alice", "knows", "Bob").unwrap();
        engine.insert("Bob", "knows", "Charlie").unwrap();
        engine.insert("Alice", "likes", "Coffee").unwrap();

        assert_eq!(engine.size(), 3);
    }
}
