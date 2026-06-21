//! # NervusDB — embedded property graph database for Rust
//!
//! SQLite-style local database directory, Fjall-backed crash-safe persistence,
//! ACID single-writer transactions, lock-free snapshot reads. Zero
//! configuration, zero server.
//!
//! ## Quickstart
//!
//! ```rust,ignore
//! use nervusdb::Db;
//! use nervusdb_query::{prepare, query_collect, Params};
//!
//! let db = Db::open("my_graph")?;
//!
//! // CREATE
//! let snapshot = db.snapshot();
//! let mut txn = db.begin_write();
//! prepare("CREATE (n:Person {name: 'Alice', age: 30})")?
//!     .execute_write(&snapshot, &mut txn, &Params::new())?;
//! txn.commit()?;
//!
//! // QUERY
//! let rows = query_collect(
//!     &db.snapshot(),
//!     "MATCH (n:Person) WHERE n.age > 20 RETURN n.name LIMIT 10",
//!     &Params::new(),
//! )?;
//! assert_eq!(rows[0].columns()[0].1, "Alice".into());
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Supported Mini-Cypher
//!
//! | Feature | Example |
//! |---------|---------|
//! | Node scan | `MATCH (n:Person) RETURN n` |
//! | One-hop traversal | `MATCH (a)-[:KNOWS]->(b) RETURN b` |
//! | Two-hop traversal | `MATCH (a)-[:KNOWS]->(b)-[:KNOWS]->(c)` |
//! | Property filter | `MATCH (n) WHERE n.age = 30` |
//! | Create | `CREATE (n:Person {name: 'Alice'})` |
//! | Set property | `MATCH (n) SET n.name = 'Bob'` |
//! | Delete | `MATCH (n) WHERE ... DELETE n` |
//! | Remove property | `MATCH (n) REMOVE n.name` |
//! | LIMIT | `LIMIT 10` |
//! | EXPLAIN | `EXPLAIN MATCH (n) RETURN n` |
//! | Label operations | `SET n:Label`, `REMOVE n:Label` |
//! | Property Map SET | `SET n = {x: 1}` |
//!
//! ## Core API
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Db`] | Open/create database, begin transactions, create indexes |
//! | [`WriteTxn`] | ACID write transaction — create nodes/edges, set properties |
//! | [`DbSnapshot`] | Lock-free read snapshot for queries and traversals |
//! | [`ReadTxn`] | Lightweight read transaction, neighbor traversal |
//! | [`query`] | Mini-Cypher parser and executor (re-export) |
//!
//! ## Architecture
//!
//! ```text
//!         ┌──────────┐
//!         │    Db    │  ← entry point
//!         └────┬─────┘
//!          ┌───┴───┐
//!          ▼       ▼
//!     ┌────────┐ ┌──────────┐
//!     │WriteTxn│ │DbSnapshot│  ← single writer, lock-free readers
//!     └───┬────┘ └────┬─────┘
//!         │           │
//!     ┌───▼──────┐  ┌─▼──────────┐
//!     │ GraphEngine │ │ Mini-Cypher │
//!     └────────────┘ └────────────┘
//! ```
//!
//! Storage path: a local database directory managed by Fjall. Fjall's internal
//! files are not part of the NervusDB public format contract.

mod error;

use nervusdb_storage::api::StorageSnapshot;
use nervusdb_storage::engine::GraphEngine;
use nervusdb_storage::snapshot::Snapshot;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use error::{Error, Result};
pub use nervusdb_api::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    RelTypeId, WriteableGraph,
};
pub use nervusdb_query as query;
pub use nervusdb_storage::PAGE_SIZE;

/// Open and manage an embedded property graph database.
///
/// # Example
///
/// ```rust,ignore
/// use nervusdb::Db;
///
/// let db = Db::open("my_graph").unwrap();
/// ```
///
/// # Concurrency
///
/// `Db` is `Send + Sync`. Internal mutations are serialized through a
/// single writer lock — only one write transaction can exist at a time.
/// Read snapshots are lock-free and never block writers.
#[derive(Debug)]
pub struct Db {
    engine: GraphEngine,
    storage_dir: PathBuf,
}

impl Db {
    /// Open a database directory at the given path.
    ///
    /// Creates the directory and Fjall-managed storage files if they don't
    /// exist. NervusDB 0.1 does not expose or preserve a `.ndb/.wal` file-pair
    /// format.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or opened, or if
    /// storage recovery fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let storage_dir = path.as_ref().to_path_buf();
        let engine = GraphEngine::open(&storage_dir)?;
        Ok(Self {
            engine,
            storage_dir,
        })
    }

    /// Path to the local database directory.
    #[inline]
    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    /// Create a read snapshot for queries and traversals.
    ///
    /// The snapshot reflects a consistent view of the graph at creation time.
    /// It is lock-free and non-blocking — concurrent write transactions do
    /// not affect the data visible through this snapshot.
    ///
    /// Use the snapshot directly with [`GraphSnapshot`] methods or pass it
    /// to [`query::query_collect`] for Mini-Cypher queries.
    pub fn snapshot(&self) -> DbSnapshot {
        DbSnapshot(self.engine.snapshot())
    }

    /// Begin a read-only transaction.
    ///
    /// Returns a [`ReadTxn`] providing low-level neighbor traversal. For
    /// Mini-Cypher queries, prefer [`Db::snapshot`] instead.
    pub fn begin_read(&self) -> ReadTxn {
        ReadTxn {
            snapshot: self.engine.begin_read(),
        }
    }

    /// Begin an ACID write transaction.
    ///
    /// Only one write transaction can exist at a time across all threads.
    /// All modifications are buffered in memory until [`WriteTxn::commit`]
    /// is called, at which point they are written atomically through Fjall and
    /// made visible to new snapshots.
    ///
    /// # Panics
    ///
    /// Panics if another write transaction is already in progress.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut txn = db.begin_write();
    /// let person = txn.get_or_create_label("Person").unwrap();
    /// let node = txn.create_node(1, person).unwrap();
    /// txn.set_node_property(node, "name".into(), "Alice".into()).unwrap();
    /// txn.commit().unwrap();
    /// ```
    pub fn begin_write(&self) -> WriteTxn<'_> {
        WriteTxn {
            inner: self.engine.begin_write(),
        }
    }

    /// Persist committed graph data through the storage backend.
    pub fn checkpoint(&self) -> Result<()> {
        self.engine.persist().map_err(Error::from)
    }

    /// Close the database after a best-effort checkpoint.
    ///
    /// Not implemented in `Drop` — call this explicitly to flush pending
    /// state before discarding the handle.
    pub fn close(self) -> Result<()> {
        self.engine.checkpoint_on_close().map_err(Error::from)?;
        Ok(())
    }
}

/// A read snapshot that implements [`GraphSnapshot`].
///
/// Created by [`Db::snapshot`]. Provides access to nodes, labels,
/// properties, neighbor traversal, label/relationship-type resolution,
/// All methods are read-only and lock-free.
///
/// # Methods
///
/// All [`GraphSnapshot`] methods are available directly:
/// - `nodes()` — iterate all non-tombstoned node IDs
/// - `neighbors()` / `incoming_neighbors()` — traverse relationships
/// - `node_property()` / `edge_property()` — read property values
/// - `node_properties()` / `edge_properties()` — read all properties of a node/edge
/// - `resolve_label_id()` / `resolve_label_name()` — label name ↔ id
/// - `resolve_rel_type_id()` / `resolve_rel_type_name()` — rel type name ↔ id
/// - `node_count()` / `edge_count()` — count entities
pub struct DbSnapshot(StorageSnapshot);

impl GraphSnapshot for DbSnapshot {
    type Neighbors<'a> = Box<dyn Iterator<Item = EdgeKey> + 'a>;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        Box::new(self.0.neighbors(src, rel))
    }

    fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_> {
        Box::new(self.0.incoming_neighbors(dst, rel))
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        self.0.nodes()
    }

    fn nodes_with_label(&self, label: LabelId) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        self.0.nodes_with_label(label)
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

    fn node_count(&self, label: Option<LabelId>) -> u64 {
        self.0.node_count(label)
    }

    fn edge_count(&self, rel: Option<RelTypeId>) -> u64 {
        self.0.edge_count(rel)
    }
}

/// A low-level read transaction returned by [`Db::begin_read`].
///
/// Use for direct neighbor traversal by relationship type.
/// For Mini-Cypher queries, use [`Db::snapshot`] instead.
#[derive(Debug, Clone)]
pub struct ReadTxn {
    snapshot: Snapshot,
}

impl ReadTxn {
    /// Iterate outgoing edges from `src`. Optionally filter by `rel` type.
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

/// An ACID write transaction.
///
/// Created by [`Db::begin_write`]. All modifications are buffered in memory
/// until [`commit`](WriteTxn::commit) writes them atomically through the
/// storage backend.
///
/// Only one write transaction may exist at a time. Drop without committing
/// discards all pending changes.
///
/// # Multiple statements in one transaction
///
/// You can call [`query::prepare`] + [`execute_write`](prepared_query_impl)
/// multiple times within the same transaction:
///
/// ```rust,ignore
/// let snapshot = db.snapshot();
/// let mut txn = db.begin_write();
///
/// prepare("CREATE (a:Person {name: 'Alice'})")?
///     .execute_write(&snapshot, &mut txn, &Params::new())?;
/// prepare("CREATE (b:Person {name: 'Bob'})")?
///     .execute_write(&snapshot, &mut txn, &Params::new())?;
///
/// txn.commit()?;  // both creates are atomic
/// ```
pub struct WriteTxn<'a> {
    inner: nervusdb_storage::engine::WriteTxn<'a>,
}

impl<'a> WriteTxn<'a> {
    /// Create a new node.
    ///
    /// `external_id` must be unique (across all committed nodes). Returns
    /// the internal node ID used for edge creation and property operations.
    /// External ID 0 is reserved — use positive integers.
    pub fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> Result<InternalNodeId> {
        self.inner
            .create_node(external_id, label_id)
            .map_err(Error::from)
    }

    /// Get or create a label by name. Returns the label ID.
    pub fn get_or_create_label(&mut self, name: &str) -> Result<LabelId> {
        self.inner.get_or_create_label(name).map_err(Error::from)
    }

    /// Get or create a relationship type by name. Returns the type ID.
    ///
    pub fn get_or_create_rel_type(&mut self, name: &str) -> Result<RelTypeId> {
        self.inner.get_or_create_rel_type(name).map_err(Error::from)
    }

    /// Create a directed edge from `src` to `dst` with relationship type `rel`.
    pub fn create_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()> {
        self.inner.create_edge(src, rel, dst);
        Ok(())
    }

    /// Soft-delete a node. The node becomes invisible to queries but its
    /// data is retained until compaction.
    pub fn tombstone_node(&mut self, node: InternalNodeId) {
        self.inner.tombstone_node(node);
    }

    /// Soft-delete an edge.
    pub fn tombstone_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.inner.tombstone_edge(src, rel, dst);
    }

    /// Set a property on a node. Overwrites existing value.
    pub fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()> {
        self.inner.set_node_property(node, key, value);
        Ok(())
    }

    /// Set a property on an edge. Overwrites existing value.
    pub fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()> {
        self.inner.set_edge_property(src, rel, dst, key, value);
        Ok(())
    }

    /// Remove a property from a node. No-op if the property doesn't exist.
    pub fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()> {
        self.inner.remove_node_property(node, key);
        Ok(())
    }

    /// Remove a property from an edge. No-op if the property doesn't exist.
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

    /// Commit this transaction atomically.
    ///
    /// Writes all buffered modifications through Fjall, then makes them
    /// visible to new read snapshots. Consumes the transaction; call once per
    /// `begin_write`.
    pub fn commit(self) -> Result<()> {
        self.inner.commit().map_err(Error::from)
    }
}

impl WriteableGraph for WriteTxn<'_> {
    fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> nervusdb_api::GraphWriteResult<InternalNodeId> {
        self.inner
            .create_node(external_id, label_id)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    fn add_node_label(
        &mut self,
        node: InternalNodeId,
        label_id: LabelId,
    ) -> nervusdb_api::GraphWriteResult<()> {
        self.inner
            .add_node_label(node, label_id)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    fn remove_node_label(
        &mut self,
        node: InternalNodeId,
        label_id: LabelId,
    ) -> nervusdb_api::GraphWriteResult<()> {
        self.inner
            .remove_node_label(node, label_id)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    fn create_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> nervusdb_api::GraphWriteResult<()> {
        self.inner.create_edge(src, rel, dst);
        Ok(())
    }

    fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> nervusdb_api::GraphWriteResult<()> {
        self.inner.set_node_property(node, key, value);
        Ok(())
    }

    fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> nervusdb_api::GraphWriteResult<()> {
        self.inner.set_edge_property(src, rel, dst, key, value);
        Ok(())
    }

    fn remove_node_property(
        &mut self,
        node: InternalNodeId,
        key: &str,
    ) -> nervusdb_api::GraphWriteResult<()> {
        self.inner.remove_node_property(node, key);
        Ok(())
    }

    fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) -> nervusdb_api::GraphWriteResult<()> {
        self.inner.remove_edge_property(src, rel, dst, key);
        Ok(())
    }

    fn tombstone_node(&mut self, node: InternalNodeId) -> nervusdb_api::GraphWriteResult<()> {
        self.inner.tombstone_node(node);
        Ok(())
    }

    fn tombstone_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> nervusdb_api::GraphWriteResult<()> {
        self.inner.tombstone_edge(src, rel, dst);
        Ok(())
    }

    fn get_or_create_label_id(&mut self, name: &str) -> nervusdb_api::GraphWriteResult<LabelId> {
        self.inner
            .get_or_create_label(name)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    fn get_or_create_rel_type_id(
        &mut self,
        name: &str,
    ) -> nervusdb_api::GraphWriteResult<RelTypeId> {
        self.inner
            .get_or_create_rel_type(name)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    fn staged_created_nodes_with_labels(&self) -> Vec<(InternalNodeId, Vec<String>)> {
        self.inner.staged_created_nodes_with_labels()
    }
}
