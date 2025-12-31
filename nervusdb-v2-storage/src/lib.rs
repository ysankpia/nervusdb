pub mod api;
pub mod backup;
pub mod blob_store;
pub mod bulkload;
pub mod csr;
pub mod engine;
mod error;
pub mod idmap;
pub mod index;
pub mod label_interner;
pub mod memtable;
pub mod pager;
pub mod property;
pub mod snapshot;
pub mod stats;
pub mod vacuum;
pub mod wal;

pub use crate::error::{Error, Result};

pub const PAGE_SIZE: usize = 8192;
pub const FILE_MAGIC: [u8; 16] = *b"NERVUSDBv2\x00\x00\x00\x00\x00\x00";
pub const VERSION_MAJOR: u32 = 2;
pub const VERSION_MINOR: u32 = 0;
