//! # NervusDB v2 (Rust-First Edition)
//!
//! **The "SQLite" of Graph Databases for Rust.**
//!
//! NervusDB is an embedded graph database designed for local-first applications.
//! It provides a unified, zero-config experience for managing persistent graph data
//! with strong consistency and safety guarantees.
//!
//! ## 🚀 Quickstart
//!
//! Add `nervusdb` to your `Cargo.toml`. Then, you can start building your graph:
//!
//! ```rust,no_run
//! use nervusdb::{Db, Result};
//!
//! fn main() -> Result<()> {
//!     // 1. Open the database (creates .ndb and .wal files)
//!     let db = Db::open("my_graph.ndb")?;
//!
//!     // 2. Write Data
//!     let mut txn = db.begin_write();
//!     // (APIs for node creation in progress, see examples/tour.rs)
//!     txn.commit()?;
//!
//!     // 3. Query Data (Cypher)
//!     let snapshot = db.snapshot();
//!     // snapshot.query("MATCH (n) RETURN n", ...);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## 💡 Core Concepts
//!
//! - **[`Db`]**: The entry point. Handles file management, locking, and engine initialization.
//!   Safe to share across threads (it uses internal locking).
//! - **[`WriteTxn`]**: Exclusive access for modifying the graph. ACID compliant.
//! - **[`ReadTxn`] / [`Snapshot`]**: Consistent view of the graph for querying. Non-blocking.
//! - **[`query`]**: The Cypher execution engine (re-exported from `nervusdb-query`).
//!
//! ## 📦 Feature Flags
//!
//! | Flag | Description | Default |
//! |------|-------------|---------|
//! | `async` | (Planned) Enable async `Db` and `Txn` wrappers | `false` |
//! | `serde` | (Implicit) Serde support for property values | `true` |

mod error;

use nervusdb_storage::api::StorageSnapshot;
use nervusdb_storage::engine::GraphEngine;
use nervusdb_storage::snapshot::Snapshot;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use error::{Error, Result};
pub use nervusdb_api::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    RelTypeId,
};
pub use nervusdb_query as query;
pub use nervusdb_storage::PAGE_SIZE;
pub use nervusdb_storage::backup::{
    BackupHandle, BackupInfo, BackupManager, BackupManifest, BackupStatus,
};
pub use nervusdb_storage::bulkload::{BulkEdge, BulkLoader, BulkNode};
pub use nervusdb_storage::vacuum::VacuumReport;

/// The main database handle for NervusDB v2.
///
/// # Example
///
/// ```ignore
/// use nervusdb::Db;
///
/// let db = Db::open("my_graph.ndb").unwrap();
/// ```
///
/// # Concurrency
///
/// `Db` can be shared across threads. Internal mutations are serialized
/// through a single writer lock.
#[derive(Debug)]
pub struct Db {
    engine: GraphEngine,
    ndb_path: PathBuf,
    wal_path: PathBuf,
}

impl Db {
    /// Opens a database at the given path.
    ///
    /// The path can be:
    /// - A directory path: files will be created as `<path>.ndb` and `<path>.wal`
    /// - An explicit `.ndb` or `.wal` path: the other file is inferred
    ///
    /// Returns an error if the database cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let (ndb_path, wal_path) = derive_paths(path);
        Self::open_paths(ndb_path, wal_path)
    }

    /// Opens a database with explicit paths for the data and WAL files.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let db = Db::open_paths("graph.ndb", "graph.wal").unwrap();
    /// ```
    pub fn open_paths(ndb_path: impl AsRef<Path>, wal_path: impl AsRef<Path>) -> Result<Self> {
        let ndb_path = ndb_path.as_ref().to_path_buf();
        let wal_path = wal_path.as_ref().to_path_buf();
        let engine = GraphEngine::open(&ndb_path, &wal_path)?;
        Ok(Self {
            engine,
            ndb_path,
            wal_path,
        })
    }

    /// Returns the path to the main data file (`.ndb`).
    #[inline]
    pub fn ndb_path(&self) -> &Path {
        &self.ndb_path
    }

    /// Returns the path to the WAL file (`.wal`).
    #[inline]
    pub fn wal_path(&self) -> &Path {
        &self.wal_path
    }

    /// Begins a read-only transaction.
    ///
    /// The returned `ReadTxn` provides a consistent view of the database
    /// at the time of creation. It can be used concurrently with other
    /// read transactions and will not see writes that commit after its creation.
    pub fn begin_read(&self) -> ReadTxn {
        ReadTxn {
            snapshot: self.engine.begin_read(),
        }
    }

    /// Creates a snapshot for query execution.
    ///
    /// Returns a `DbSnapshot` that implements `GraphSnapshot` trait,
    /// suitable for use with the query engine.
    pub fn snapshot(&self) -> DbSnapshot {
        DbSnapshot(self.engine.snapshot())
    }

    /// Begins a write transaction.
    ///
    /// Write transactions are exclusive - only one can exist at a time.
    /// The transaction must be explicitly committed with `commit()`.
    ///
    /// # Panics
    ///
    /// Panics if another write transaction is already in progress.
    pub fn begin_write(&self) -> WriteTxn<'_> {
        WriteTxn {
            inner: self.engine.begin_write(),
        }
    }

    /// Triggers a compaction operation.
    ///
    /// Compaction merges frozen MemTables into CSR segments and removes
    /// tombstoned entries. This is a potentially expensive operation
    /// that should be done during maintenance windows.
    pub fn compact(&self) -> Result<()> {
        self.engine.compact().map_err(Error::from)
    }

    /// Creates a durability checkpoint.
    ///
    /// In MVP, this is equivalent to `compact()`. Future versions may
    /// implement lightweight checkpoints that don't require full compaction.
    pub fn checkpoint(&self) -> Result<()> {
        // MVP: checkpoint == explicit compaction boundary + durability manifest.
        self.engine.compact().map_err(Error::from)
    }

    /// Explicitly closes the DB and performs a best-effort checkpoint-on-close (T106).
    ///
    /// This is intentionally not implemented in `Drop` to avoid hiding expensive IO.
    pub fn close(self) -> Result<()> {
        self.engine.checkpoint_on_close().map_err(Error::from)?;
        Ok(())
    }

    /// Creates an index on the specified label and property.
    ///
    /// # Example
    /// ```ignore
    /// db.create_index("User", "email")?;
    /// ```
    pub fn create_index(&self, label: &str, property: &str) -> Result<()> {
        self.engine
            .create_index(label, property)
            .map_err(Error::from)
    }

    /// Searches for nodes with vectors similar to the query vector.
    ///
    /// Returns a list of `(node_id, distance)` tuples.
    pub fn search_vector(&self, query: &[f32], k: usize) -> Result<Vec<(InternalNodeId, f32)>> {
        self.engine.search_vector(query, k).map_err(Error::from)
    }
}

/// Performs in-place vacuum through the v2 facade.
///
/// This keeps CLI and other callers on the facade surface instead of coupling
/// directly to storage internals.
pub fn vacuum(path: impl AsRef<Path>) -> Result<VacuumReport> {
    let (ndb_path, wal_path) = derive_paths(path.as_ref());
    nervusdb_storage::vacuum::vacuum_in_place(&ndb_path, &wal_path).map_err(Error::from)
}

/// Creates a consistent on-disk backup snapshot.
///
/// The database path accepts either base path, `.ndb`, or `.wal`.
/// Backup artifacts are written under `backup_dir/<backup-id>/`.
pub fn backup(path: impl AsRef<Path>, backup_dir: impl AsRef<Path>) -> Result<BackupInfo> {
    let (ndb_path, _) = derive_paths(path.as_ref());
    let manager = BackupManager::new(ndb_path, backup_dir.as_ref().to_path_buf());
    let handle = manager.begin_backup().map_err(Error::from)?;
    manager.execute_backup(&handle).map_err(Error::from)?;

    match manager.status(&handle).map_err(Error::from)? {
        BackupStatus::Completed(info) => Ok(info),
        BackupStatus::Failed { error } => Err(Error::Other(error)),
        BackupStatus::InProgress { .. } => Err(Error::Other(
            "backup did not reach completed state".to_string(),
        )),
    }
}

/// Bulk loads data into a new database file in offline mode.
pub fn bulkload(path: impl AsRef<Path>, nodes: Vec<BulkNode>, edges: Vec<BulkEdge>) -> Result<()> {
    let (ndb_path, _) = derive_paths(path.as_ref());
    let mut loader = BulkLoader::new(ndb_path).map_err(Error::from)?;
    for node in nodes {
        loader.add_node(node).map_err(Error::from)?;
    }
    for edge in edges {
        loader.add_edge(edge).map_err(Error::from)?;
    }
    loader.commit().map_err(Error::from)
}

/// A wrapper around the storage snapshot to hide internal types.
pub struct DbSnapshot(StorageSnapshot);

impl GraphSnapshot for DbSnapshot {
    type Neighbors<'a> = Box<dyn Iterator<Item = EdgeKey> + 'a>;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        self.0.neighbors(src, rel)
    }

    fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_> {
        self.0.incoming_neighbors(dst, rel)
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        self.0.nodes()
    }

    fn resolve_external(&self, iid: InternalNodeId) -> Option<ExternalId> {
        self.0.resolve_external(iid)
    }

    fn node_label(&self, iid: InternalNodeId) -> Option<LabelId> {
        self.0.node_label(iid)
    }

    fn resolve_node_labels(&self, iid: InternalNodeId) -> Option<Vec<LabelId>> {
        self.0.resolve_node_labels(iid)
    }

    fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        self.0.is_tombstoned_node(iid)
    }

    fn node_property(&self, iid: InternalNodeId, key: &str) -> Option<PropertyValue> {
        self.0.node_property(iid, key)
    }

    fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        self.0.edge_property(edge, key)
    }

    fn node_properties(&self, iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        self.0.node_properties(iid)
    }

    fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        self.0.edge_properties(edge)
    }

    fn resolve_label_id(&self, name: &str) -> Option<LabelId> {
        self.0.resolve_label_id(name)
    }

    fn resolve_rel_type_id(&self, name: &str) -> Option<RelTypeId> {
        self.0.resolve_rel_type_id(name)
    }

    fn resolve_label_name(&self, id: LabelId) -> Option<String> {
        self.0.resolve_label_name(id)
    }

    fn resolve_rel_type_name(&self, id: RelTypeId) -> Option<String> {
        self.0.resolve_rel_type_name(id)
    }

    fn lookup_index(
        &self,
        label: &str,
        field: &str,
        value: &PropertyValue,
    ) -> Option<Vec<InternalNodeId>> {
        self.0.lookup_index(label, field, value)
    }

    fn node_count(&self, label: Option<LabelId>) -> u64 {
        self.0.node_count(label)
    }

    fn edge_count(&self, rel: Option<RelTypeId>) -> u64 {
        self.0.edge_count(rel)
    }
}

/// A read-only transaction.
///
/// Created by [`Db::begin_read()`]. Provides consistent snapshot access.
#[derive(Debug, Clone)]
pub struct ReadTxn {
    snapshot: Snapshot,
}

impl ReadTxn {
    /// Gets outgoing neighbors of a node.
    ///
    /// Returns an iterator over edges. If `rel` is `Some`, only edges
    /// of that relationship type are returned.
    pub fn neighbors(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> impl Iterator<Item = EdgeKey> + '_ {
        self.snapshot.neighbors(src, rel).map(|k| EdgeKey {
            src: k.src,
            rel: k.rel,
            dst: k.dst,
        })
    }
}

/// A write transaction.
///
/// Created by [`Db::begin_write()`]. All modifications are buffered
/// until `commit()` is called. The transaction consumes `self` on commit.
pub struct WriteTxn<'a> {
    inner: nervusdb_storage::engine::WriteTxn<'a>,
}

impl<'a> WriteTxn<'a> {
    /// Creates a new node with the given external ID and label.
    ///
    /// Returns the internal node ID for use in subsequent operations.
    pub fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> Result<InternalNodeId> {
        self.inner
            .create_node(external_id, label_id)
            .map_err(Error::from)
    }

    /// Gets or creates a label ID for the given name.
    pub fn get_or_create_label(&mut self, name: &str) -> Result<LabelId> {
        self.inner.get_or_create_label(name).map_err(Error::from)
    }

    /// Gets or creates a relationship type ID for the given name.
    pub fn get_or_create_rel_type(&mut self, name: &str) -> Result<RelTypeId> {
        self.inner.get_or_create_rel_type(name).map_err(Error::from)
    }

    /// Creates a directed edge from source to destination.
    ///
    /// The relationship type is identified by `rel`.
    pub fn create_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.inner.create_edge(src, rel, dst);
    }

    /// Soft-deletes a node.
    ///
    /// The node becomes invisible to queries but its data is retained
    /// until compaction removes it. Outgoing edges are also hidden.
    pub fn tombstone_node(&mut self, node: InternalNodeId) {
        self.inner.tombstone_node(node);
    }

    /// Soft-deletes an edge.
    ///
    /// The edge becomes invisible to neighbor queries.
    pub fn tombstone_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.inner.tombstone_edge(src, rel, dst);
    }

    /// Sets a property on a node.
    ///
    /// If the property already exists, it is overwritten.
    pub fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()> {
        let storage_value = convert_to_storage_property_value(value);
        self.inner.set_node_property(node, key, storage_value);
        Ok(())
    }

    /// Sets a property on an edge.
    ///
    /// If the property already exists, it is overwritten.
    pub fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()> {
        let storage_value = convert_to_storage_property_value(value);
        self.inner
            .set_edge_property(src, rel, dst, key, storage_value);
        Ok(())
    }

    /// Removes a property from a node.
    ///
    /// If the property doesn't exist, this is a no-op.
    pub fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()> {
        self.inner.remove_node_property(node, key);
        Ok(())
    }

    /// Removes a property from an edge.
    ///
    /// If the property doesn't exist, this is a no-op.
    pub fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) -> Result<()> {
        self.inner.remove_edge_property(src, rel, dst, key);
        Ok(())
    }

    /// Sets the vector embedding for a node.
    ///
    /// This vector can be used for similarity search.
    pub fn set_vector(&mut self, node: InternalNodeId, vector: Vec<f32>) -> Result<()> {
        self.inner.set_vector(node, vector).map_err(Error::from)
    }

    /// Commits the transaction.
    ///
    /// All modifications are written to the WAL and made visible
    /// to new read transactions. The transaction is consumed.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if commit fails.
    pub fn commit(self) -> Result<()> {
        self.inner.commit().map_err(Error::from)
    }
}

fn convert_to_storage_property_value(
    v: PropertyValue,
) -> nervusdb_storage::property::PropertyValue {
    v
}

fn derive_paths(path: &Path) -> (PathBuf, PathBuf) {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ndb") => (path.to_path_buf(), path.with_extension("wal")),
        Some("wal") => (path.with_extension("ndb"), path.to_path_buf()),
        _ => (path.with_extension("ndb"), path.with_extension("wal")),
    }
}

#[cfg(test)]
mod tests {
    use super::{BulkNode, Db, Error, PropertyValue, backup, bulkload, vacuum};
    use std::collections::BTreeMap;

    #[test]
    fn vacuum_reports_not_found_for_missing_db() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("missing-db");
        let err = vacuum(&missing).expect_err("missing db must fail");
        match err {
            Error::Io(inner) => assert_eq!(inner.kind(), std::io::ErrorKind::NotFound),
            other => panic!("expected IO error, got {other:?}"),
        }
    }

    #[test]
    fn vacuum_succeeds_for_existing_db() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let base = dir.path().join("graph");
        let db = Db::open(&base).expect("open db");
        db.close().expect("close db");

        let report = vacuum(&base).expect("vacuum should succeed");
        assert_eq!(
            report.ndb_path.extension().and_then(|s| s.to_str()),
            Some("ndb")
        );
        assert!(
            report.backup_path.exists(),
            "vacuum should emit backup file path"
        );
    }

    #[test]
    fn backup_reports_not_found_for_missing_db() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("missing-db");
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).expect("create backup dir");

        let err = backup(&missing, &backups).expect_err("missing db must fail");
        match err {
            Error::Io(inner) => assert_eq!(inner.kind(), std::io::ErrorKind::NotFound),
            other => panic!("expected IO error, got {other:?}"),
        }
    }

    #[test]
    fn bulkload_creates_database_from_nodes() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let base = dir.path().join("bulkload-graph");
        let nodes = vec![BulkNode {
            external_id: 1,
            label: "User".to_string(),
            properties: BTreeMap::from([(
                "name".to_string(),
                PropertyValue::String("alice".to_string()),
            )]),
        }];

        bulkload(&base, nodes, Vec::new()).expect("bulkload should succeed");
        let db = Db::open(&base).expect("bulkloaded db should open");
        db.close().expect("bulkloaded db should close");
    }
}

// Implement WriteableGraph for Facade WriteTxn
// This bridges the Facade (v2) with the Query Engine (v2-query)
impl nervusdb_query::WriteableGraph for WriteTxn<'_> {
    fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> nervusdb_query::Result<InternalNodeId> {
        self.inner
            .create_node(external_id, label_id)
            .map_err(|e| nervusdb_query::Error::Other(e.to_string()))
    }

    fn add_node_label(
        &mut self,
        node: InternalNodeId,
        label_id: LabelId,
    ) -> nervusdb_query::Result<()> {
        self.inner
            .add_node_label(node, label_id)
            .map_err(|e| nervusdb_query::Error::Other(e.to_string()))
    }

    fn remove_node_label(
        &mut self,
        node: InternalNodeId,
        label_id: LabelId,
    ) -> nervusdb_query::Result<()> {
        self.inner
            .remove_node_label(node, label_id)
            .map_err(|e| nervusdb_query::Error::Other(e.to_string()))
    }

    fn create_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> nervusdb_query::Result<()> {
        self.inner.create_edge(src, rel, dst);
        Ok(())
    }

    fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: nervusdb_storage::property::PropertyValue,
    ) -> nervusdb_query::Result<()> {
        // Query Engine uses storage PropertyValue directly now (from re-export)
        self.inner.set_node_property(node, key, value);
        Ok(())
    }

    fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: nervusdb_storage::property::PropertyValue,
    ) -> nervusdb_query::Result<()> {
        self.inner.set_edge_property(src, rel, dst, key, value);
        Ok(())
    }

    fn remove_node_property(
        &mut self,
        node: InternalNodeId,
        key: &str,
    ) -> nervusdb_query::Result<()> {
        self.inner.remove_node_property(node, key);
        Ok(())
    }

    fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) -> nervusdb_query::Result<()> {
        self.inner.remove_edge_property(src, rel, dst, key);
        Ok(())
    }

    fn tombstone_node(&mut self, node: InternalNodeId) -> nervusdb_query::Result<()> {
        self.inner.tombstone_node(node);
        Ok(())
    }

    fn tombstone_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> nervusdb_query::Result<()> {
        self.inner.tombstone_edge(src, rel, dst);
        Ok(())
    }

    fn get_or_create_label_id(&mut self, name: &str) -> nervusdb_query::Result<LabelId> {
        self.inner
            .get_or_create_label(name)
            .map_err(|e| nervusdb_query::Error::Other(e.to_string()))
    }

    fn get_or_create_rel_type_id(&mut self, name: &str) -> nervusdb_query::Result<RelTypeId> {
        self.inner
            .get_or_create_rel_type(name)
            .map_err(|e| nervusdb_query::Error::Other(e.to_string()))
    }

    fn staged_created_nodes_with_labels(&self) -> Vec<(InternalNodeId, Vec<String>)> {
        self.inner.staged_created_nodes_with_labels()
    }
}
