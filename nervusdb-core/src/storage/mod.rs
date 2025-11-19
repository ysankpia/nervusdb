use std::path::Path;

use crate::{Result, Triple};

#[cfg(not(target_arch = "wasm32"))]
pub mod disk;
pub mod memory;

pub type HexastoreIter = Box<dyn Iterator<Item = Triple>>;

/// Unified interface implemented by every concrete storage backend.
pub trait Hexastore: Send {
    fn insert(&mut self, triple: &Triple) -> Result<bool>;
    fn query(
        &self,
        subject_id: Option<u64>,
        predicate_id: Option<u64>,
        object_id: Option<u64>,
    ) -> HexastoreIter;
    fn iter(&self) -> HexastoreIter {
        self.query(None, None, None)
    }
}

/// Instantiate the default storage backend for the current target.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_store<P: AsRef<Path>>(path: P) -> Result<Box<dyn Hexastore + Send>> {
    Ok(Box::new(disk::DiskHexastore::open(path)?))
}

/// Instantiate the in-memory storage backend for WASM targets.
#[cfg(target_arch = "wasm32")]
pub fn open_store<P: AsRef<Path>>(_path: P) -> Result<Box<dyn Hexastore + Send>> {
    Ok(Box::new(memory::MemoryHexastore::new()))
}
