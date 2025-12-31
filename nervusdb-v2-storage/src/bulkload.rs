use crate::csr::CsrSegment;
use crate::idmap::{ExternalId, InternalNodeId, LabelId};
use crate::label_interner::LabelInterner;
use crate::property::PropertyValue;
use crate::wal::SegmentPointer;
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A node to be bulk-loaded into the database.
#[derive(Debug, Clone)]
pub struct BulkNode {
    pub external_id: ExternalId,
    pub label: String,
    pub properties: BTreeMap<String, PropertyValue>,
}

/// An edge to be bulk-loaded into the database.
#[derive(Debug, Clone)]
pub struct BulkEdge {
    pub src_external_id: ExternalId,
    pub rel_type: String,
    pub dst_external_id: ExternalId,
    pub properties: BTreeMap<String, PropertyValue>,
}

/// Offline bulk loader for high-performance data import.
///
/// This bypasses WAL and directly generates L1 Segments and IdMap,
/// achieving significantly higher throughput than regular write transactions.
///
/// # Example
///
/// ```ignore
/// let mut loader = BulkLoader::new(PathBuf::from("db.ndb"))?;
///
/// loader.add_node(BulkNode {
///     external_id: 1,
///     label: "Person".to_string(),
///     properties: BTreeMap::from([
///         ("name".to_string(), PropertyValue::String("Alice".to_string())),
///     ]),
/// })?;
///
/// loader.add_edge(BulkEdge {
///     src_external_id: 1,
///     rel_type: "KNOWS".to_string(),
///     dst_external_id: 2,
///     properties: BTreeMap::new(),
/// })?;
///
/// loader.commit()?;
/// ```
pub struct BulkLoader {
    db_path: PathBuf,
    wal_path: PathBuf,
    nodes: Vec<BulkNode>,
    edges: Vec<BulkEdge>,
}

impl BulkLoader {
    /// Creates a new bulk loader for the specified database path.
    ///
    /// The database MUST NOT exist yet. The loader will create a new database
    /// from scratch.
    pub fn new(db_path: PathBuf) -> Result<Self> {
        // Verify database doesn't exist
        if db_path.exists() {
            return Err(Error::WalProtocol(
                "Database file already exists. BulkLoader only works with new databases.",
            ));
        }

        // Derive WAL path
        let wal_path = db_path.with_extension("wal");

        Ok(Self {
            db_path,
            wal_path,
            nodes: Vec::new(),
            edges: Vec::new(),
        })
    }

    /// Adds a node to the bulk load batch.
    ///
    /// Nodes are buffered in memory until `commit()` is called.
    ///
    /// # Errors
    ///
    /// Returns an error if the external_id is not unique.
    pub fn add_node(&mut self, node: BulkNode) -> Result<()> {
        // Uniqueness and referential integrity are validated in `commit()` to allow
        // streaming ingestion without requiring nodes/edges ordering constraints here.
        self.nodes.push(node);
        Ok(())
    }

    /// Adds an edge to the bulk load batch.
    ///
    /// Edges are buffered in memory until `commit()` is called.
    ///
    /// # Errors
    ///
    /// Returns an error if src or dst external_id doesn't reference a node.
    pub fn add_edge(&mut self, edge: BulkEdge) -> Result<()> {
        // Referential integrity is validated in `commit()` to allow loading edges
        // before all nodes have been buffered.
        self.edges.push(edge);
        Ok(())
    }

    /// Commits the bulk load, writing all data to disk.
    ///
    /// This performs the following steps:
    /// 1. Validates all data (uniqueness, referential integrity)
    /// 2. Assigns internal IDs to all nodes
    /// 3. Generates L1 Segments from edges
    /// 4. Writes properties to B-Tree
    /// 5. Initializes WAL with manifest
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Validation fails
    /// - Disk I/O fails
    /// - Database constraints are violated
    pub fn commit(self) -> Result<()> {
        // Step 1: Validate data
        self.validate()?;

        // Step 2: Create Pager
        let mut pager = crate::pager::Pager::open(&self.db_path)?;

        // Step 3: Build the unified label/rel_type interner (must be consistent across
        // idmap, segments, properties, and WAL label definitions).
        let mut label_interner = self.build_label_interner();

        // Step 4: Build IdMap and node label IDs
        let (external_to_internal, node_label_ids) =
            self.build_idmap_and_labels(&mut pager, &mut label_interner)?;

        // Step 5: Generate L1 Segments
        let segments = self.build_segments(&external_to_internal, &label_interner)?;

        // Clone for write_statistics (write_segments consumes segments)
        let segments_for_stats = segments.clone();

        // Step 6: Write segments to pager and get segment pointers
        let segment_pointers = self.write_segments(&mut pager, segments)?;

        // Step 7: Write properties and get properties_root
        let properties_root =
            self.write_properties(&mut pager, &external_to_internal, &label_interner)?;

        // Step 8: Collect statistics and get stats_root
        let stats_root = self.write_statistics(&mut pager, &node_label_ids, &segments_for_stats)?;

        // Step 9: Initialize WAL with manifest + label definitions
        self.initialize_wal(
            &segment_pointers,
            properties_root,
            stats_root,
            &label_interner,
        )?;

        Ok(())
    }

    fn build_label_interner(&self) -> LabelInterner {
        let mut interner = LabelInterner::new();
        for node in &self.nodes {
            interner.get_or_create(&node.label);
        }
        for edge in &self.edges {
            interner.get_or_create(&edge.rel_type);
        }
        interner
    }

    /// Validates all bulk data for consistency.
    fn validate(&self) -> Result<()> {
        // Check external_id uniqueness
        let mut seen_ids = BTreeMap::new();
        for (idx, node) in self.nodes.iter().enumerate() {
            if seen_ids.insert(node.external_id, idx).is_some() {
                return Err(Error::WalProtocol("Duplicate external_id in bulk load"));
            }
        }

        // Build set of valid external IDs for edge validation
        let valid_ids: BTreeMap<ExternalId, ()> =
            self.nodes.iter().map(|n| (n.external_id, ())).collect();

        // Validate edges reference existing nodes
        for edge in &self.edges {
            if !valid_ids.contains_key(&edge.src_external_id) {
                return Err(Error::WalProtocol(
                    "Edge src_external_id references non-existent node",
                ));
            }
            if !valid_ids.contains_key(&edge.dst_external_id) {
                return Err(Error::WalProtocol(
                    "Edge dst_external_id references non-existent node",
                ));
            }
        }

        Ok(())
    }

    /// Builds IdMap and label interner, returns mappings for subsequent steps.
    fn build_idmap_and_labels(
        &self,
        pager: &mut crate::pager::Pager,
        label_interner: &mut LabelInterner,
    ) -> Result<(BTreeMap<ExternalId, InternalNodeId>, Vec<LabelId>)> {
        use crate::idmap::IdMap;

        // Load IdMap (should be empty for new database)
        let mut idmap = IdMap::load(pager)?;

        // Build external_to_internal mapping
        let mut external_to_internal = BTreeMap::new();
        let mut label_snapshot = Vec::new();

        // Assign internal IDs and register labels
        for (idx, node) in self.nodes.iter().enumerate() {
            let internal_id = idx as InternalNodeId;
            let label_id = label_interner.get_or_create(&node.label);

            external_to_internal.insert(node.external_id, internal_id);
            label_snapshot.push(label_id);

            // Write to IdMap
            idmap.apply_create_node(pager, node.external_id, label_id, internal_id)?;
        }

        // We don't persist label interner snapshot to pager metadata. The canonical mapping
        // is reconstructed from WAL `CreateLabel` records during `GraphEngine::open()`.

        Ok((external_to_internal, label_snapshot))
    }

    /// Builds L1 CSR segments from edges.
    fn build_segments(
        &self,
        external_to_internal: &BTreeMap<ExternalId, InternalNodeId>,
        label_interner: &LabelInterner,
    ) -> Result<Vec<CsrSegment>> {
        use crate::csr::{EdgeRecord, SegmentId};
        use crate::snapshot::EdgeKey;

        // Build edge list with internal IDs
        let mut edges: Vec<EdgeKey> = Vec::with_capacity(self.edges.len());

        for edge in &self.edges {
            let src = external_to_internal[&edge.src_external_id];
            let dst = external_to_internal[&edge.dst_external_id];
            let rel = label_interner
                .get_id(&edge.rel_type)
                .ok_or(Error::WalProtocol(
                    "unknown relationship type during bulkload",
                ))?;
            edges.push(EdgeKey { src, rel, dst });
        }

        // Sort edges by (src, rel, dst)
        edges.sort();

        if edges.is_empty() {
            // Return empty segment
            return Ok(vec![CsrSegment {
                id: SegmentId(0),
                meta_page_id: 0,
                min_src: 0,
                max_src: 0,
                min_dst: 0,
                max_dst: 0,
                offsets: vec![0, 0],
                edges: Vec::new(),
                in_offsets: Vec::new(),
                in_edges: Vec::new(),
            }]);
        }

        // Group edges by source node
        let (min_src, max_src) = edges.iter().fold((u32::MAX, 0u32), |(min_s, max_s), e| {
            (min_s.min(e.src), max_s.max(e.src))
        });

        let range = (max_src - min_src) as usize + 2;
        let mut offsets = vec![0u64; range];
        let mut edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeRecord>> = BTreeMap::new();

        for e in edges {
            edges_by_src.entry(e.src).or_default().push(EdgeRecord {
                rel: e.rel,
                dst: e.dst,
            });
        }

        // Build CSR format
        let mut edge_vec: Vec<EdgeRecord> = Vec::new();
        let mut cursor = 0u64;
        for src in min_src..=max_src {
            let idx = (src - min_src) as usize;
            offsets[idx] = cursor;
            if let Some(mut list) = edges_by_src.remove(&src) {
                list.sort_by_key(|r| (r.rel, r.dst));
                cursor += list.len() as u64;
                edge_vec.extend(list);
            }
        }
        offsets[(max_src - min_src) as usize + 1] = cursor;

        Ok(vec![CsrSegment {
            id: SegmentId(0),
            meta_page_id: 0,
            min_src,
            max_src,
            min_dst: 0,
            max_dst: 0,
            offsets,
            edges: edge_vec,
            in_offsets: Vec::new(),
            in_edges: Vec::new(),
        }])
    }

    /// Writes segments to pager and returns segment pointers.
    fn write_segments(
        &self,
        pager: &mut crate::pager::Pager,
        mut segments: Vec<CsrSegment>,
    ) -> Result<Vec<SegmentPointer>> {
        let mut pointers = Vec::with_capacity(segments.len());

        for seg in segments.iter_mut() {
            seg.persist(pager)?;
            pointers.push(SegmentPointer {
                id: seg.id.0,
                meta_page_id: seg.meta_page_id,
            });
        }

        Ok(pointers)
    }

    /// Writes properties to B-Tree and returns the root page ID.
    fn write_properties(
        &self,
        pager: &mut crate::pager::Pager,
        external_to_internal: &BTreeMap<ExternalId, InternalNodeId>,
        label_interner: &LabelInterner,
    ) -> Result<u64> {
        use crate::index::btree::BTree;

        // Create B-Tree for properties
        let mut tree = BTree::create(pager)?;

        // Write node properties (Tag 0)
        for node in &self.nodes {
            let internal_id = external_to_internal[&node.external_id];
            for (key, value) in &node.properties {
                let mut btree_key = Vec::with_capacity(1 + 4 + 4 + key.len());
                btree_key.push(0u8); // Tag 0: Node Property
                btree_key.extend_from_slice(&internal_id.to_be_bytes());
                btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
                btree_key.extend_from_slice(key.as_bytes());

                let encoded_val = value.encode();
                let blob_id = crate::blob_store::BlobStore::write_direct(pager, &encoded_val)?;
                tree.insert(pager, &btree_key, blob_id)?;
            }
        }

        // Write edge properties (Tag 1)
        for edge in &self.edges {
            let src = external_to_internal[&edge.src_external_id];
            let dst = external_to_internal[&edge.dst_external_id];
            let rel = label_interner
                .get_id(&edge.rel_type)
                .ok_or(Error::WalProtocol(
                    "unknown relationship type during bulkload",
                ))?;

            for (key, value) in &edge.properties {
                let mut btree_key = Vec::with_capacity(1 + 4 + 4 + 4 + 4 + key.len());
                btree_key.push(1u8); // Tag 1: Edge Property
                btree_key.extend_from_slice(&src.to_be_bytes());
                btree_key.extend_from_slice(&rel.to_be_bytes());
                btree_key.extend_from_slice(&dst.to_be_bytes());
                btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
                btree_key.extend_from_slice(key.as_bytes());

                let encoded_val = value.encode();
                let blob_id = crate::blob_store::BlobStore::write_direct(pager, &encoded_val)?;
                tree.insert(pager, &btree_key, blob_id)?;
            }
        }

        Ok(tree.root().as_u64())
    }

    /// Collects statistics and writes them to blob store.
    fn write_statistics(
        &self,
        pager: &mut crate::pager::Pager,
        label_snapshot: &[LabelId],
        segments: &[CsrSegment],
    ) -> Result<u64> {
        let mut stats = crate::stats::GraphStatistics::default();

        // Count nodes per label
        stats.total_nodes = label_snapshot.len() as u64;
        for &label_id in label_snapshot.iter() {
            *stats.node_counts_by_label.entry(label_id).or_default() += 1;
        }

        // Count edges per type
        for seg in segments {
            stats.total_edges += seg.edges.len() as u64;
            for edge in &seg.edges {
                *stats.edge_counts_by_type.entry(edge.rel).or_default() += 1;
            }
        }

        // Write stats to blob store
        let encoded_stats = stats.encode();
        let stats_root = crate::blob_store::BlobStore::write_direct(pager, &encoded_stats)?;

        Ok(stats_root)
    }

    /// Initializes WAL with manifest and label definitions.
    fn initialize_wal(
        &self,
        segment_pointers: &[SegmentPointer],
        properties_root: u64,
        stats_root: u64,
        label_interner: &LabelInterner,
    ) -> Result<()> {
        use crate::wal::{Wal, WalRecord};

        let mut wal = Wal::open(&self.wal_path)?;

        let txid = 0; // First transaction

        // Begin transaction
        wal.append(&WalRecord::BeginTx { txid })?;

        // Write label definitions using snapshot's iter_ids
        let snapshot = label_interner.snapshot();
        for id in snapshot.iter_ids() {
            if let Some(name) = snapshot.get_name(id) {
                wal.append(&WalRecord::CreateLabel {
                    name: name.to_string(),
                    label_id: id,
                })?;
            }
        }

        // Write manifest switch
        wal.append(&WalRecord::ManifestSwitch {
            epoch: 0,
            segments: segment_pointers.to_vec(),
            properties_root,
            stats_root,
        })?;

        // Write checkpoint
        wal.append(&WalRecord::Checkpoint {
            up_to_txid: 0,
            epoch: 0,
            properties_root,
            stats_root,
        })?;

        // Commit transaction
        wal.append(&WalRecord::CommitTx { txid })?;

        // Fsync WAL for durability
        wal.fsync()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_bulkloader_creation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.ndb");

        let loader = BulkLoader::new(db_path).unwrap();
        assert_eq!(loader.nodes.len(), 0);
        assert_eq!(loader.edges.len(), 0);
    }

    #[test]
    fn test_bulkloader_rejects_existing_db() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.ndb");

        // Create existing file
        std::fs::write(&db_path, b"dummy").unwrap();

        let result = BulkLoader::new(db_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_node() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.ndb");

        let mut loader = BulkLoader::new(db_path).unwrap();
        loader
            .add_node(BulkNode {
                external_id: 1,
                label: "Person".to_string(),
                properties: BTreeMap::new(),
            })
            .unwrap();

        assert_eq!(loader.nodes.len(), 1);
    }

    #[test]
    fn test_add_edge() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.ndb");

        let mut loader = BulkLoader::new(db_path).unwrap();
        loader
            .add_edge(BulkEdge {
                src_external_id: 1,
                rel_type: "KNOWS".to_string(),
                dst_external_id: 2,
                properties: BTreeMap::new(),
            })
            .unwrap();

        assert_eq!(loader.edges.len(), 1);
    }

    #[test]
    fn test_bulkloader_commit_and_reopen() {
        use crate::engine::GraphEngine;

        let dir = tempdir().unwrap();
        let ndb = dir.path().join("test.ndb");
        let wal = dir.path().join("test.wal");

        // Create database with bulk loader
        {
            let mut loader = BulkLoader::new(ndb.to_path_buf()).unwrap();

            loader
                .add_node(BulkNode {
                    external_id: 100,
                    label: "Person".to_string(),
                    properties: BTreeMap::from([
                        (
                            "name".to_string(),
                            PropertyValue::String("Alice".to_string()),
                        ),
                        ("age".to_string(), PropertyValue::Int(30)),
                    ]),
                })
                .unwrap();

            loader
                .add_node(BulkNode {
                    external_id: 200,
                    label: "Person".to_string(),
                    properties: BTreeMap::from([(
                        "name".to_string(),
                        PropertyValue::String("Bob".to_string()),
                    )]),
                })
                .unwrap();

            loader
                .add_edge(BulkEdge {
                    src_external_id: 100,
                    rel_type: "KNOWS".to_string(),
                    dst_external_id: 200,
                    properties: BTreeMap::from([("since".to_string(), PropertyValue::Int(2020))]),
                })
                .unwrap();

            loader.commit().unwrap();
        }

        // Reopen and verify
        {
            let engine = GraphEngine::open(&ndb, &wal).unwrap();

            // Check nodes exist
            let alice_iid = engine.lookup_internal_id(100).expect("Alice should exist");
            let bob_iid = engine.lookup_internal_id(200).expect("Bob should exist");

            // Check edges exist
            let snap = engine.begin_read();
            let neighbors: Vec<_> = snap.neighbors(alice_iid, None).collect();
            assert_eq!(neighbors.len(), 1);
            assert_eq!(neighbors[0].dst, bob_iid);

            // Check label names
            assert_eq!(engine.get_label_name(0), Some("Person".to_string()));
            assert_eq!(engine.get_label_name(1), Some("KNOWS".to_string()));
        }
    }
}
