use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use nervusdb_core::{Database, Options, Fact, QueryCriteria};

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
/// This is now a wrapper around nervusdb-core.
#[wasm_bindgen]
pub struct StorageEngine {
    db: Database,
    // Statistics kept for API compatibility, though db might have its own
    insert_count: u64,
    query_count: u64,
}

#[wasm_bindgen]
impl StorageEngine {
    /// Create a new storage engine
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<StorageEngine, JsValue> {
        let options = Options::new("memory");
        let db = Database::open(options)
            .map_err(|e| JsValue::from_str(&format!("Failed to open database: {}", e)))?;
            
        Ok(StorageEngine {
            db,
            insert_count: 0,
            query_count: 0,
        })
    }
    
    /// Create engine with custom capacity (ignored in core for now, kept for compatibility)
    #[wasm_bindgen(js_name = withCapacity)]
    pub fn with_capacity(_capacity: usize) -> Result<StorageEngine, JsValue> {
        Self::new()
    }

    /// Insert a triple
    #[wasm_bindgen]
    pub fn insert(&mut self, subject: &str, predicate: &str, object: &str) -> Result<(), JsValue> {
        let fact = Fact::new(subject, predicate, object);
        self.db.add_fact(fact)
            .map_err(|e| JsValue::from_str(&format!("Failed to insert fact: {}", e)))?;
        
        self.insert_count += 1;
        Ok(())
    }
    
    /// Batch insert multiple triples for better performance
    #[wasm_bindgen(js_name = insertBatch)]
    pub fn insert_batch(&mut self, subjects: Vec<JsValue>, predicates: Vec<JsValue>, objects: Vec<JsValue>) -> Result<usize, JsValue> {
        let len = subjects.len().min(predicates.len()).min(objects.len());
        
        for i in 0..len {
            let subject = subjects[i].as_string()
                .ok_or_else(|| JsValue::from_str("Invalid subject"))?;
            let predicate = predicates[i].as_string()
                .ok_or_else(|| JsValue::from_str("Invalid predicate"))?;
            let object = objects[i].as_string()
                .ok_or_else(|| JsValue::from_str("Invalid object"))?;
                
            self.insert(&subject, &predicate, &object)?;
        }
        
        Ok(len)
    }

    /// Query triples by subject
    #[wasm_bindgen]
    pub fn query_by_subject(&mut self, subject: &str) -> Result<JsValue, JsValue> {
        self.query_count += 1;
        
        let subject_id = self.db.resolve_id(subject)
            .map_err(|e| JsValue::from_str(&format!("Database error: {}", e)))?;
        
        if subject_id.is_none() {
            return serde_wasm_bindgen::to_value(&Vec::<Triple>::new())
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)));
        }

        let criteria = QueryCriteria {
            subject_id,
            predicate_id: None,
            object_id: None,
        };
        
        let results: Vec<Triple> = self.db.query(criteria).map(|t| {
            let s = self.db.resolve_str(t.subject_id).unwrap_or_default().unwrap_or_default();
            let p = self.db.resolve_str(t.predicate_id).unwrap_or_default().unwrap_or_default();
            let o = self.db.resolve_str(t.object_id).unwrap_or_default().unwrap_or_default();
            Triple { subject: s, predicate: p, object: o }
        }).collect();

        serde_wasm_bindgen::to_value(&results)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Query triples by predicate
    #[wasm_bindgen]
    pub fn query_by_predicate(&mut self, predicate: &str) -> Result<JsValue, JsValue> {
        self.query_count += 1;

        let predicate_id = self.db.resolve_id(predicate)
            .map_err(|e| JsValue::from_str(&format!("Database error: {}", e)))?;
        
        if predicate_id.is_none() {
            return serde_wasm_bindgen::to_value(&Vec::<Triple>::new())
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)));
        }

        let criteria = QueryCriteria {
            subject_id: None,
            predicate_id,
            object_id: None,
        };
        
        let results: Vec<Triple> = self.db.query(criteria).map(|t| {
            let s = self.db.resolve_str(t.subject_id).unwrap_or_default().unwrap_or_default();
            let p = self.db.resolve_str(t.predicate_id).unwrap_or_default().unwrap_or_default();
            let o = self.db.resolve_str(t.object_id).unwrap_or_default().unwrap_or_default();
            Triple { subject: s, predicate: p, object: o }
        }).collect();

        serde_wasm_bindgen::to_value(&results)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Execute a Cypher-like query
    #[wasm_bindgen(js_name = executeQuery)]
    pub fn execute_query(&mut self, query: &str) -> Result<JsValue, JsValue> {
        self.query_count += 1;

        let core_triples = self.db.execute_query(query)
            .map_err(|e| JsValue::from_str(&format!("Query execution error: {}", e)))?;

        let results: Vec<Triple> = core_triples.into_iter().map(|t| {
            let s = self.db.resolve_str(t.subject_id).unwrap_or_default().unwrap_or_default();
            let p = self.db.resolve_str(t.predicate_id).unwrap_or_default().unwrap_or_default();
            let o = self.db.resolve_str(t.object_id).unwrap_or_default().unwrap_or_default();
            Triple { subject: s, predicate: p, object: o }
        }).collect();

        serde_wasm_bindgen::to_value(&results)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Get statistics
    #[wasm_bindgen]
    pub fn get_stats(&self) -> Result<JsValue, JsValue> {
        let stats = serde_json::json!({
            "total_triples": self.db.all_triples().len(),
            "insert_count": self.insert_count,
            "query_count": self.query_count,
        });

        serde_wasm_bindgen::to_value(&stats)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Clear all data
    #[wasm_bindgen]
    pub fn clear(&mut self) -> Result<(), JsValue> {
        // Database doesn't support clear/truncate yet easily without reopening.
        // But for memory store, we can just drop and recreate.
        let options = Options::new("memory");
        self.db = Database::open(options)
            .map_err(|e| JsValue::from_str(&format!("Failed to reset database: {}", e)))?;
        self.insert_count = 0;
        self.query_count = 0;
        Ok(())
    }

    /// Get total number of triples
    #[wasm_bindgen]
    pub fn size(&self) -> usize {
        self.db.all_triples().len()
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
