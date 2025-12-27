use nervusdb_v2_storage::engine::GraphEngine;
use nervusdb_v2_storage::snapshot::{EdgeKey, RelTypeId, Snapshot};
use std::path::{Path, PathBuf};

pub use nervusdb_v2_storage::idmap::{ExternalId, InternalNodeId, LabelId};
pub use nervusdb_v2_storage::{Error, Result};

/// Property value types (re-exported from API for convenience).
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

#[derive(Debug)]
pub struct Db {
    engine: GraphEngine,
    ndb_path: PathBuf,
    wal_path: PathBuf,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let (ndb_path, wal_path) = derive_paths(path);
        Self::open_paths(ndb_path, wal_path)
    }

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

    #[inline]
    pub fn ndb_path(&self) -> &Path {
        &self.ndb_path
    }

    #[inline]
    pub fn wal_path(&self) -> &Path {
        &self.wal_path
    }

    pub fn begin_read(&self) -> ReadTxn {
        ReadTxn {
            snapshot: self.engine.begin_read(),
        }
    }

    pub fn begin_write(&self) -> WriteTxn<'_> {
        WriteTxn {
            inner: self.engine.begin_write(),
        }
    }

    pub fn compact(&self) -> Result<()> {
        self.engine.compact()
    }

    pub fn checkpoint(&self) -> Result<()> {
        // MVP: checkpoint == explicit compaction boundary + durability manifest.
        self.engine.compact()
    }
}

#[derive(Debug, Clone)]
pub struct ReadTxn {
    snapshot: Snapshot,
}

impl ReadTxn {
    pub fn neighbors(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> impl Iterator<Item = EdgeKey> + '_ {
        self.snapshot.neighbors(src, rel)
    }
}

pub struct WriteTxn<'a> {
    inner: nervusdb_v2_storage::engine::WriteTxn<'a>,
}

impl<'a> WriteTxn<'a> {
    pub fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> Result<InternalNodeId> {
        self.inner.create_node(external_id, label_id)
    }

    pub fn create_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.inner.create_edge(src, rel, dst);
    }

    pub fn tombstone_node(&mut self, node: InternalNodeId) {
        self.inner.tombstone_node(node);
    }

    pub fn tombstone_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.inner.tombstone_edge(src, rel, dst);
    }

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

    pub fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()> {
        self.inner.remove_node_property(node, key);
        Ok(())
    }

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
