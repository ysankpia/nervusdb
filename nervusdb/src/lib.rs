//! # NervusDB 0.1 Rust Facade
//!
//! Rust-first embedded property graph database: SQLite-style local files,
//! crash-safe persistence, and a deliberately small graph query surface.
//!
//! The 0.1 line is intentionally narrow. The core path is:
//!
//! ```text
//! Db::open -> begin_write/commit -> snapshot -> Mini-Cypher/direct traversal
//! ```
//!
//! ## 0.1 Core APIs
//!
//! - [`Db::open`] and [`Db::open_paths`] open local `.ndb` and `.wal` files.
//! - [`Db::snapshot`] creates a read snapshot for query execution.
//! - [`Db::begin_write`] creates the single-writer transaction path.
//! - [`ReadTxn`] and [`DbSnapshot`] expose label/property/traversal reads.
//! - [`WriteTxn`] exposes node, edge, label, relationship type, and property
//!   persistence.
//! - [`query`] re-exports the Mini-Cypher query crate for supported 0.1 reads
//!   and writes.
//!
//! ## Experimental Or Maintenance APIs
//!
//! Existing maintenance APIs such as [`Db::create_index`], [`Db::compact`],
//! and [`Db::checkpoint`] remain available for compatibility. They are not
//! the default 0.1 product surface.
//!
//! See `docs/reference/rust-api.md` for the repository-level API contract.
//!
//! ## Quickstart
//!
//! Add `nervusdb` to your `Cargo.toml`. Then, you can start building your graph:
//!
//! ```rust,no_run
//! use nervusdb::{Db, Result};
//! use nervusdb_query::{prepare, query_collect, Params};
//!
//! fn main() -> Result<()> {
//!     let db = Db::open("my_graph.ndb")?;
//!
//!     let snapshot = db.snapshot();
//!     let create = prepare("CREATE (n:Person {name: 'Alice'})")
//!         .map_err(|e| nervusdb::Error::Other(e.to_string()))?;
//!     let mut txn = db.begin_write();
//!     create
//!         .execute_write(&snapshot, &mut txn, &Params::new())
//!         .map_err(|e| nervusdb::Error::Other(e.to_string()))?;
//!     txn.commit()?;
//!
//!     let rows = query_collect(
//!         &db.snapshot(),
//!         "MATCH (n:Person) RETURN n.name LIMIT 10",
//!         &Params::new(),
//!     )
//!     .map_err(|e| nervusdb::Error::Other(e.to_string()))?;
//!     assert_eq!(rows.len(), 1);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Core Concepts
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

/// The main Rust facade for the 0.1 embedded database core.
///
/// `Db` is the 0.1 entry point for opening local `.ndb` / `.wal` files,
/// creating write transactions, and taking read snapshots. APIs outside that
/// path are retained for maintenance or experiments and are called out in their
/// own docs.
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
    /// Opens a local database path.
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
    /// This is part of the 0.1 core API for callers that want predictable file
    /// placement.
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
    ///
    /// This is part of the 0.1 core API.
    #[inline]
    pub fn ndb_path(&self) -> &Path {
        &self.ndb_path
    }

    /// Returns the path to the WAL file (`.wal`).
    ///
    /// This is part of the 0.1 core API.
    #[inline]
    pub fn wal_path(&self) -> &Path {
        &self.wal_path
    }

    /// Begins a read-only transaction.
    ///
    /// The returned `ReadTxn` provides a consistent view of the database
    /// at the time of creation. It can be used concurrently with other
    /// read transactions and will not see writes that commit after its creation.
    /// This is part of the 0.1 core API.
    pub fn begin_read(&self) -> ReadTxn {
        ReadTxn {
            snapshot: self.engine.begin_read(),
        }
    }

    /// Creates a snapshot for query execution and direct graph reads.
    ///
    /// Returns a `DbSnapshot` that implements `GraphSnapshot` trait,
    /// suitable for use with the query engine and direct traversal. This is
    /// part of the 0.1 core API.
    pub fn snapshot(&self) -> DbSnapshot {
        DbSnapshot(self.engine.snapshot())
    }

    /// Begins a write transaction.
    ///
    /// Write transactions are exclusive - only one can exist at a time.
    /// The transaction must be explicitly committed with `commit()`.
    /// This is part of the 0.1 core API.
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
    /// Experimental / maintenance API. Not part of the 0.1 core API contract.
    ///
    /// Compaction merges frozen MemTables into CSR segments and removes
    /// tombstoned entries. This is a potentially expensive operation
    /// that should be done during maintenance windows.
    pub fn compact(&self) -> Result<()> {
        self.engine.compact().map_err(Error::from)
    }

    /// Creates a durability checkpoint.
    ///
    /// Experimental / maintenance API. Not part of the 0.1 core API contract.
    ///
    /// In MVP, this is equivalent to `compact()`. Future versions may
    /// implement lightweight checkpoints that don't require full compaction.
    pub fn checkpoint(&self) -> Result<()> {
        // MVP: checkpoint == explicit compaction boundary + durability manifest.
        self.engine.compact().map_err(Error::from)
    }

    /// Explicitly closes the DB and performs a best-effort checkpoint-on-close.
    ///
    /// Experimental / maintenance API. Not part of the 0.1 core API contract.
    ///
    /// This is intentionally not implemented in `Drop` to avoid hiding expensive IO.
    pub fn close(self) -> Result<()> {
        self.engine.checkpoint_on_close().map_err(Error::from)?;
        Ok(())
    }

    /// Creates an index on the specified label and property.
    ///
    /// Experimental / maintenance API. Not part of the 0.1 core API contract.
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
}

/// A 0.1 core read snapshot returned by [`Db::snapshot`].
///
/// `DbSnapshot` implements [`GraphSnapshot`] so callers can scan nodes, resolve
/// labels and relationship types, read properties, and traverse neighbors
/// without coupling to storage internals.
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

/// A 0.1 core read-only transaction.
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
    /// This is part of the 0.1 core API.
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

/// A 0.1 core write transaction.
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
    /// This is part of the 0.1 core API.
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
    ///
    /// This is part of the 0.1 core API.
    pub fn get_or_create_label(&mut self, name: &str) -> Result<LabelId> {
        self.inner.get_or_create_label(name).map_err(Error::from)
    }

    /// Gets or creates a relationship type ID for the given name.
    ///
    /// This is part of the 0.1 core API.
    pub fn get_or_create_rel_type(&mut self, name: &str) -> Result<RelTypeId> {
        self.inner.get_or_create_rel_type(name).map_err(Error::from)
    }

    /// Creates a directed edge from source to destination.
    ///
    /// The relationship type is identified by `rel`.
    /// This is part of the 0.1 core API.
    pub fn create_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.inner.create_edge(src, rel, dst);
    }

    /// Soft-deletes a node.
    ///
    /// The node becomes invisible to queries but its data is retained
    /// until compaction removes it. Outgoing edges are also hidden.
    /// This is part of the 0.1 core API.
    pub fn tombstone_node(&mut self, node: InternalNodeId) {
        self.inner.tombstone_node(node);
    }

    /// Soft-deletes an edge.
    ///
    /// The edge becomes invisible to neighbor queries.
    /// This is part of the 0.1 core API.
    pub fn tombstone_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.inner.tombstone_edge(src, rel, dst);
    }

    /// Sets a property on a node.
    ///
    /// If the property already exists, it is overwritten.
    /// This is part of the 0.1 core API.
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
    /// This is part of the 0.1 core API.
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
    /// This is part of the 0.1 core API.
    pub fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()> {
        self.inner.remove_node_property(node, key);
        Ok(())
    }

    /// Removes a property from an edge.
    ///
    /// If the property doesn't exist, this is a no-op.
    /// This is part of the 0.1 core API.
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

    /// Commits the transaction.
    ///
    /// All modifications are written to the WAL and made visible
    /// to new read transactions. The transaction is consumed.
    /// This is part of the 0.1 core API.
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
