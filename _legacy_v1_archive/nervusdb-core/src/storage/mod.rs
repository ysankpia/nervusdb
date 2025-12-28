#[cfg(target_arch = "wasm32")]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use crate::{Fact, Result, Triple};
#[cfg(not(target_arch = "wasm32"))]
use redb::Database;

#[cfg(not(target_arch = "wasm32"))]
pub mod disk;
pub mod memory;
pub mod property;
#[cfg(not(target_arch = "wasm32"))]
pub mod schema;
#[cfg(not(target_arch = "wasm32"))]
pub mod varint_key;

pub type HexastoreIter = Box<dyn Iterator<Item = Triple>>;

/// Unified interface implemented by every concrete storage backend.
pub trait Hexastore: Send {
    fn insert(&mut self, triple: &Triple) -> Result<bool>;
    fn delete(&mut self, triple: &Triple) -> Result<bool>;
    fn insert_fact(&mut self, fact: Fact<'_>) -> Result<Triple>;
    fn query(
        &self,
        subject_id: Option<u64>,
        predicate_id: Option<u64>,
        object_id: Option<u64>,
    ) -> HexastoreIter;
    fn iter(&self) -> HexastoreIter {
        self.query(None, None, None)
    }

    // Dictionary operations
    fn resolve_str(&self, id: u64) -> Result<Option<String>>;
    fn resolve_id(&self, value: &str) -> Result<Option<u64>>;
    fn intern(&mut self, value: &str) -> Result<u64>;
    fn dictionary_size(&self) -> Result<u64>;

    // Property operations (legacy string-based, maintained for backward compatibility)
    fn set_node_property(&mut self, id: u64, value: &str) -> Result<()>;
    fn get_node_property(&self, id: u64) -> Result<Option<String>>;
    fn set_edge_property(&mut self, s: u64, p: u64, o: u64, value: &str) -> Result<()>;
    fn get_edge_property(&self, s: u64, p: u64, o: u64) -> Result<Option<String>>;

    // Binary property operations (v2.0, using FlexBuffers for performance)
    // These are the preferred methods for new code

    /// Set node property using binary format (FlexBuffers)
    /// This is 10x faster than JSON string serialization
    fn set_node_property_binary(&mut self, id: u64, value: &[u8]) -> Result<()> {
        // Default implementation: convert to string (fallback for legacy implementations)
        let json_str = std::str::from_utf8(value)
            .map_err(|e| crate::Error::Other(format!("invalid UTF-8: {}", e)))?;
        self.set_node_property(id, json_str)
    }

    /// Get node property as binary (FlexBuffers or JSON)
    fn get_node_property_binary(&self, id: u64) -> Result<Option<Vec<u8>>> {
        // Default implementation: convert from string
        self.get_node_property(id)
            .map(|opt| opt.map(|s| s.into_bytes()))
    }

    /// Set edge property using binary format (FlexBuffers)
    fn set_edge_property_binary(&mut self, s: u64, p: u64, o: u64, value: &[u8]) -> Result<()> {
        let json_str = std::str::from_utf8(value)
            .map_err(|e| crate::Error::Other(format!("invalid UTF-8: {}", e)))?;
        self.set_edge_property(s, p, o, json_str)
    }

    /// Get edge property as binary (FlexBuffers or JSON)
    fn get_edge_property_binary(&self, s: u64, p: u64, o: u64) -> Result<Option<Vec<u8>>> {
        self.get_edge_property(s, p, o)
            .map(|opt| opt.map(|s| s.into_bytes()))
    }

    /// Delete node properties (both legacy and binary)
    fn delete_node_properties(&mut self, _id: u64) -> Result<()> {
        // Default implementation: no-op for backward compatibility
        // Implementations should override this
        Ok(())
    }

    // Batch operations (added in v2.0 for performance)
    // These reduce cross-language call overhead by batching multiple operations

    /// Insert multiple triples in a single transaction
    /// Returns the number of triples actually inserted (excludes duplicates)
    fn batch_insert(&mut self, triples: &[Triple]) -> Result<usize> {
        let mut count = 0;
        for triple in triples {
            if self.insert(triple)? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Delete multiple triples in a single transaction
    /// Returns the number of triples actually deleted
    fn batch_delete(&mut self, triples: &[Triple]) -> Result<usize> {
        let mut count = 0;
        for triple in triples {
            if self.delete(triple)? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Set multiple node properties in a single transaction
    /// Format: (node_id, json_string)
    fn batch_set_node_properties(&mut self, props: &[(u64, &str)]) -> Result<()> {
        for (id, value) in props {
            self.set_node_property(*id, value)?;
        }
        Ok(())
    }

    /// Set multiple edge properties in a single transaction
    /// Format: ((s, p, o), json_string)
    fn batch_set_edge_properties(&mut self, props: &[((u64, u64, u64), &str)]) -> Result<()> {
        for ((s, p, o), value) in props {
            self.set_edge_property(*s, *p, *o, value)?;
        }
        Ok(())
    }

    /// Insert multiple facts (string form) in a single optimized transaction
    /// Opens all tables once and reuses handles for maximum performance
    fn batch_insert_facts(&mut self, facts: &[Fact<'_>]) -> Result<Vec<Triple>> {
        let mut results = Vec::with_capacity(facts.len());
        for fact in facts {
            results.push(self.insert_fact(*fact)?);
        }
        Ok(results)
    }

    /// Bulk intern strings in a single transaction; returns ids in the same order as input.
    fn bulk_intern(&mut self, values: &[&str]) -> Result<Vec<u64>> {
        let mut ids = Vec::with_capacity(values.len());
        for v in values {
            ids.push(self.intern(v)?);
        }
        Ok(ids)
    }

    /// Called after a write transaction has been committed through an external path.
    ///
    /// Example: `Database::commit_transaction()` commits a `redb::WriteTransaction` directly.
    /// Storage backends with read caches should override this hook to invalidate them.
    fn after_write_commit(&self) {}
}

/// Instantiate the default storage backend for the current target.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_store(db: Arc<Database>) -> Result<Box<dyn Hexastore + Send>> {
    Ok(Box::new(disk::DiskHexastore::new(db)?))
}

/// Instantiate the in-memory storage backend for WASM targets.
#[cfg(target_arch = "wasm32")]
pub fn open_store<P: AsRef<Path>>(_path: P) -> Result<Box<dyn Hexastore + Send>> {
    Ok(Box::new(memory::MemoryHexastore::new()))
}
