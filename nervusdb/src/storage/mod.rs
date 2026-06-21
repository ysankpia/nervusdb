pub mod api;
pub mod engine;
mod error;
pub mod property;
pub mod snapshot;

pub use crate::storage::error::{Error, Result};

/// Legacy compatibility constant. The Fjall backend has no NervusDB page size.
pub const PAGE_SIZE: usize = 8192;
pub const FILE_MAGIC: [u8; 16] = *b"NERVUSDBFJALL\x00\x00\x00";
pub const VERSION_MAJOR: u32 = 3;
pub const VERSION_MINOR: u32 = 0;
pub const STORAGE_FORMAT_EPOCH: u64 = 2;
