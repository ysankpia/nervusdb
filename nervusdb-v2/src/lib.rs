use nervusdb_v2_api::GraphStore;
use nervusdb_v2_storage::api::StorageSnapshot;
use nervusdb_v2_storage::engine::GraphEngine;
use nervusdb_v2_storage::snapshot::{EdgeKey, RelTypeId, Snapshot};
use std::path::{Path, PathBuf};

pub use nervusdb_v2_query as query;
pub use nervusdb_v2_storage::idmap::{ExternalId, InternalNodeId, LabelId};
pub use nervusdb_v2_storage::{Error, Result};

/// Property value types for nodes and edges.
///
/// See [`nervusdb_v2_api::PropertyValue`] for the API-level type definition.
/// This type is provided for convenience in the v2 facade.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

/// The main database handle for NervusDB v2.
///
/// # Example
///
/// ```ignore
/// use nervusdb_v2::Db;
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
    /// Returns a `StorageSnapshot` that implements `GraphSnapshot` trait,
    /// suitable for use with the query engine.
    pub fn snapshot(&self) -> StorageSnapshot {
        self.engine.snapshot()
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
        self.engine.compact()
    }

    /// Creates a durability checkpoint.
    ///
    /// In MVP, this is equivalent to `compact()`. Future versions may
    /// implement lightweight checkpoints that don't require full compaction.
    pub fn checkpoint(&self) -> Result<()> {
        // MVP: checkpoint == explicit compaction boundary + durability manifest.
        self.engine.compact()
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
        self.snapshot.neighbors(src, rel)
    }
}

/// A write transaction.
///
/// Created by [`Db::begin_write()`]. All modifications are buffered
/// until `commit()` is called. The transaction consumes `self` on commit.
pub struct WriteTxn<'a> {
    inner: nervusdb_v2_storage::engine::WriteTxn<'a>,
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
        self.inner.create_node(external_id, label_id)
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

    /// Commits the transaction.
    ///
    /// All modifications are written to the WAL and made visible
    /// to new read transactions. The transaction is consumed.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if commit fails.
    pub fn commit(self) -> Result<()> {
        self.inner.commit()
    }
}

fn convert_to_storage_property_value(
    v: PropertyValue,
) -> nervusdb_v2_storage::property::PropertyValue {
    match v {
        PropertyValue::Null => nervusdb_v2_storage::property::PropertyValue::Null,
        PropertyValue::Bool(b) => nervusdb_v2_storage::property::PropertyValue::Bool(b),
        PropertyValue::Int(i) => nervusdb_v2_storage::property::PropertyValue::Int(i),
        PropertyValue::Float(f) => nervusdb_v2_storage::property::PropertyValue::Float(f),
        PropertyValue::String(s) => nervusdb_v2_storage::property::PropertyValue::String(s),
    }
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
impl nervusdb_v2_query::WriteableGraph for WriteTxn<'_> {
    fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> nervusdb_v2_query::Result<InternalNodeId> {
        self.inner
            .create_node(external_id, label_id)
            .map_err(|e| nervusdb_v2_query::Error::Other(e.to_string()))
    }

    fn create_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> nervusdb_v2_query::Result<()> {
        self.inner.create_edge(src, rel, dst);
        Ok(())
    }

    fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: nervusdb_v2_storage::property::PropertyValue,
    ) -> nervusdb_v2_query::Result<()> {
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
        value: nervusdb_v2_storage::property::PropertyValue,
    ) -> nervusdb_v2_query::Result<()> {
        self.inner.set_edge_property(src, rel, dst, key, value);
        Ok(())
    }

    fn tombstone_node(&mut self, node: InternalNodeId) -> nervusdb_v2_query::Result<()> {
        self.inner.tombstone_node(node);
        Ok(())
    }

    fn tombstone_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> nervusdb_v2_query::Result<()> {
        self.inner.tombstone_edge(src, rel, dst);
        Ok(())
    }
}
