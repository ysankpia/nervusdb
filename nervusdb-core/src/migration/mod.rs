//! Migration tools for converting legacy .synapsedb format to redb
//!
//! This module provides utilities to migrate data from NervusDB v1.x
//! (TypeScript + .synapsedb files) to v2.0 (Rust Core + redb).

#![cfg(not(target_arch = "wasm32"))]

pub mod legacy_reader;
pub mod migrator;

pub use legacy_reader::LegacyDatabase;
pub use migrator::{MigrationStats, migrate_database};
