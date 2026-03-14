use crate::blob_store::BlobStore;
use crate::index::btree::BTree;
use crate::pager::{PageId, Pager};
use crate::{Error, Result};
use std::collections::{HashMap, VecDeque};

/// Trait for storing vectors.
pub trait VectorStorage<Ctx> {
    fn insert_vector(&mut self, ctx: &mut Ctx, id: u32, vector: &[f32]) -> Result<()>;
    fn get_vector(&mut self, ctx: &mut Ctx, id: u32) -> Result<Vec<f32>>;
}

/// Trait for storing the HNSW graph structure.
pub trait GraphStorage<Ctx> {
    fn set_neighbors(
        &mut self,
        ctx: &mut Ctx,
        layer: u8,
        node: u32,
        neighbors: Vec<u32>,
    ) -> Result<()>;
    fn get_neighbors(&mut self, ctx: &mut Ctx, layer: u8, node: u32) -> Result<Vec<u32>>;
    fn set_meta(&mut self, ctx: &mut Ctx, entry_point: Option<u32>, max_layer: u8) -> Result<()>;
    fn get_meta(&mut self, ctx: &mut Ctx) -> Result<(Option<u32>, u8)>;
}

#[derive(Debug)]
pub struct PersistentVectorStorage {
    btree: BTree,
    cache: VectorCache,
}

impl PersistentVectorStorage {
    pub fn new(btree: BTree) -> Self {
        Self {
            btree,
            cache: VectorCache::new(DEFAULT_VECTOR_CACHE_CAP),
        }
    }

    pub fn root(&self) -> PageId {
        self.btree.root()
    }
}

fn maybe_delete_replaced_blob(
    pager: &mut Pager,
    btree: &BTree,
    key: &[u8],
    replaced_blob_id: Option<u64>,
    new_blob_id: u64,
) -> Result<()> {
    let Some(old_blob_id) = replaced_blob_id.filter(|old| *old != new_blob_id) else {
        return Ok(());
    };

    // Guard against deleting a blob that is still referenced by duplicate tuples.
    let refs = btree.count_payload_refs_for_key(pager, key, old_blob_id)?;
    if refs == 0 {
        BlobStore::delete(pager, old_blob_id)?;
    }
    Ok(())
}

impl VectorStorage<Pager> for PersistentVectorStorage {
    fn insert_vector(&mut self, pager: &mut Pager, id: u32, vector: &[f32]) -> Result<()> {
        let key = encode_vector_key(id);

        let mut data = Vec::with_capacity(vector.len() * 4);
        for val in vector {
            data.extend_from_slice(&val.to_le_bytes());
        }

        let blob_id = BlobStore::write_direct(pager, &data)?;
        self.cache.put(id, vector.to_vec());
        let replaced_blob_id = match self.btree.upsert_unique(pager, &key, blob_id) {
            Ok(replaced_blob_id) => replaced_blob_id,
            Err(err) => {
                let _ = BlobStore::delete(pager, blob_id);
                return Err(err);
            }
        };
        maybe_delete_replaced_blob(pager, &self.btree, &key, replaced_blob_id, blob_id)?;
        Ok(())
    }

    fn get_vector(&mut self, pager: &mut Pager, id: u32) -> Result<Vec<f32>> {
        if let Some(v) = self.cache.get(id) {
            return Ok(v);
        }

        let key = encode_vector_key(id);

        let mut cursor = self.btree.cursor_lower_bound(pager, &key)?;
        if !cursor.is_valid()? {
            return Err(Error::WalProtocol("Vector not found"));
        }
        let found_key = cursor.key()?;
        if found_key != key {
            return Err(Error::WalProtocol("Vector not found"));
        }
        let blob_id = cursor.payload()?;

        let data = BlobStore::read_direct(pager, blob_id)?;

        if data.len() % 4 != 0 {
            let payloads = self
                .btree
                .exact_key_payloads_full_scan(pager, &key)
                .unwrap_or_default();
            eprintln!(
                "hnsw vector decode mismatch: key={:?} blob_id={} len={} payloads={:?}",
                key,
                blob_id,
                data.len(),
                payloads
            );
            return Err(Error::StorageCorrupted("Invalid vector data length"));
        }
        let mut vector = Vec::with_capacity(data.len() / 4);
        for chunk in data.chunks_exact(4) {
            let val = f32::from_le_bytes(chunk.try_into().unwrap());
            vector.push(val);
        }

        self.cache.put(id, vector.clone());
        Ok(vector)
    }
}

const DEFAULT_VECTOR_CACHE_CAP: usize = 1024;

#[derive(Debug)]
struct VectorCache {
    cap: usize,
    map: HashMap<u32, Vec<f32>>,
    lru: VecDeque<u32>,
}

impl VectorCache {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            map: HashMap::new(),
            lru: VecDeque::new(),
        }
    }

    fn get(&mut self, id: u32) -> Option<Vec<f32>> {
        let v = self.map.get(&id)?.clone();
        self.touch(id);
        Some(v)
    }

    fn put(&mut self, id: u32, v: Vec<f32>) {
        self.map.insert(id, v);
        self.touch(id);
        while self.map.len() > self.cap {
            if let Some(evicted) = self.lru.pop_front() {
                self.map.remove(&evicted);
            } else {
                break;
            }
        }
    }

    fn touch(&mut self, id: u32) {
        if let Some(pos) = self.lru.iter().position(|&x| x == id) {
            let _ = self.lru.remove(pos);
        }
        self.lru.push_back(id);
    }
}

#[derive(Debug)]
pub struct PersistentGraphStorage {
    btree: BTree,
}

impl PersistentGraphStorage {
    pub fn new(btree: BTree) -> Self {
        Self { btree }
    }

    pub fn root(&self) -> PageId {
        self.btree.root()
    }
}

impl GraphStorage<Pager> for PersistentGraphStorage {
    fn set_neighbors(
        &mut self,
        pager: &mut Pager,
        layer: u8,
        node: u32,
        neighbors: Vec<u32>,
    ) -> Result<()> {
        let key = encode_graph_key(layer, node);

        let mut data = Vec::with_capacity(neighbors.len() * 4);
        for n in neighbors {
            data.extend_from_slice(&n.to_le_bytes());
        }

        let blob_id = BlobStore::write_direct(pager, &data)?;
        let replaced_blob_id = match self.btree.upsert_unique(pager, &key, blob_id) {
            Ok(replaced_blob_id) => replaced_blob_id,
            Err(err) => {
                let _ = BlobStore::delete(pager, blob_id);
                return Err(err);
            }
        };
        maybe_delete_replaced_blob(pager, &self.btree, &key, replaced_blob_id, blob_id)?;
        Ok(())
    }

    fn get_neighbors(&mut self, pager: &mut Pager, layer: u8, node: u32) -> Result<Vec<u32>> {
        let key = encode_graph_key(layer, node);

        let mut cursor = self.btree.cursor_lower_bound(pager, &key)?;
        if !cursor.is_valid()? {
            return Ok(Vec::new());
        }
        let found_key = cursor.key()?;
        if found_key != key {
            return Ok(Vec::new());
        }
        let blob_id = cursor.payload()?;

        let data = BlobStore::read_direct(pager, blob_id)?;

        if data.len() % 4 != 0 {
            let payloads = self
                .btree
                .exact_key_payloads_full_scan(pager, &key)
                .unwrap_or_default();
            eprintln!(
                "hnsw neighbor decode mismatch: key={:?} blob_id={} len={} payloads={:?}",
                key,
                blob_id,
                data.len(),
                payloads
            );
            return Err(Error::StorageCorrupted("Invalid neighbor list data length"));
        }
        let mut neighbors = Vec::with_capacity(data.len() / 4);
        for chunk in data.chunks_exact(4) {
            let val = u32::from_le_bytes(chunk.try_into().unwrap());
            neighbors.push(val);
        }

        Ok(neighbors)
    }

    fn set_meta(
        &mut self,
        pager: &mut Pager,
        entry_point: Option<u32>,
        max_layer: u8,
    ) -> Result<()> {
        let key = vec![TAG_META];
        let mut data = Vec::with_capacity(5);
        // byte 0: 0=None, 1=Some
        if let Some(ep) = entry_point {
            data.push(1);
            data.extend_from_slice(&ep.to_le_bytes());
        } else {
            data.push(0);
            data.extend_from_slice(&0u32.to_le_bytes());
        }
        data.push(max_layer);

        let blob_id = BlobStore::write_direct(pager, &data)?;
        let replaced_blob_id = match self.btree.upsert_unique(pager, &key, blob_id) {
            Ok(replaced_blob_id) => replaced_blob_id,
            Err(err) => {
                let _ = BlobStore::delete(pager, blob_id);
                return Err(err);
            }
        };
        maybe_delete_replaced_blob(pager, &self.btree, &key, replaced_blob_id, blob_id)?;
        Ok(())
    }

    fn get_meta(&mut self, pager: &mut Pager) -> Result<(Option<u32>, u8)> {
        let key = vec![TAG_META];
        let mut cursor = self.btree.cursor_lower_bound(pager, &key)?;
        if !cursor.is_valid()? {
            return Ok((None, 0));
        }
        let found_key = cursor.key()?;
        if found_key != key {
            return Ok((None, 0));
        }
        let blob_id = cursor.payload()?;
        let data = BlobStore::read_direct(pager, blob_id)?;

        if data.len() < 6 {
            // 1 flag + 4 ep + 1 layer
            return Ok((None, 0));
        }

        let is_some = data[0] == 1;
        let ep = u32::from_le_bytes(data[1..5].try_into().unwrap());
        let max_layer = data[5];

        if is_some {
            Ok((Some(ep), max_layer))
        } else {
            Ok((None, max_layer))
        }
    }
}

// Key Encoding Helpers

const TAG_META: u8 = 1;
const TAG_VECTOR: u8 = 2;
const TAG_GRAPH: u8 = 3;

fn encode_vector_key(id: u32) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 4);
    key.push(TAG_VECTOR);
    key.extend_from_slice(&id.to_be_bytes()); // BE for order
    key
}

fn encode_graph_key(layer: u8, node: u32) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 1 + 4);
    key.push(TAG_GRAPH);
    key.push(layer);
    key.extend_from_slice(&node.to_be_bytes()); // BE for order
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::catalog::IndexCatalog;
    use crate::pager::Pager;
    use tempfile::tempdir;

    fn ndb_len(path: &std::path::Path) -> u64 {
        std::fs::metadata(path).unwrap().len()
    }

    fn payloads_for_exact_key(tree: &BTree, pager: &Pager, key: &[u8]) -> Vec<u64> {
        let mut cur = tree.cursor_lower_bound(pager, key).unwrap();
        let mut out = Vec::new();
        while cur.is_valid().unwrap() {
            let found = cur.key().unwrap();
            if found != key {
                break;
            }
            out.push(cur.payload().unwrap());
            if !cur.advance().unwrap() {
                break;
            }
        }
        out
    }

    #[test]
    fn vector_overwrite_does_not_grow_pages_unbounded() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("vector-overwrite.ndb");
        let mut pager = Pager::open(&ndb).unwrap();
        let mut catalog = IndexCatalog::open_or_create(&mut pager).unwrap();
        let def = catalog
            .get_or_create(&mut pager, "__test_hnsw_vec")
            .unwrap();
        let mut storage = PersistentVectorStorage::new(BTree::load(def.root));

        let vector = vec![1.0_f32; 4096];
        for _ in 0..400 {
            storage.insert_vector(&mut pager, 7, &vector).unwrap();
        }

        let bytes = ndb_len(&ndb);
        assert!(
            bytes < 8 * 1024 * 1024,
            "vector overwrite leaked blob pages: {bytes} bytes"
        );
    }

    #[test]
    fn graph_overwrite_does_not_grow_pages_unbounded() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("graph-overwrite.ndb");
        let mut pager = Pager::open(&ndb).unwrap();
        let mut catalog = IndexCatalog::open_or_create(&mut pager).unwrap();
        let def = catalog
            .get_or_create(&mut pager, "__test_hnsw_graph")
            .unwrap();
        let mut storage = PersistentGraphStorage::new(BTree::load(def.root));

        let neighbors: Vec<u32> = (0..4096).collect();
        for _ in 0..400 {
            storage
                .set_neighbors(&mut pager, 0, 42, neighbors.clone())
                .unwrap();
        }

        let bytes = ndb_len(&ndb);
        assert!(
            bytes < 8 * 1024 * 1024,
            "graph overwrite leaked blob pages: {bytes} bytes"
        );
    }

    #[test]
    fn vector_upsert_does_not_delete_blob_still_referenced_by_duplicate_key() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("vector-duplicate-ref.ndb");
        let mut pager = Pager::open(&ndb).unwrap();
        let mut catalog = IndexCatalog::open_or_create(&mut pager).unwrap();
        let def = catalog
            .get_or_create(&mut pager, "__test_hnsw_vec_dup")
            .unwrap();
        let mut storage = PersistentVectorStorage::new(BTree::load(def.root));

        let key = encode_vector_key(7);
        let mut old_data = Vec::new();
        for _ in 0..16 {
            old_data.extend_from_slice(&1.0_f32.to_le_bytes());
        }
        let old_blob = BlobStore::write_direct(&mut pager, &old_data).unwrap();

        // Simulate legacy duplicate tuples for the same key.
        storage.btree.insert(&mut pager, &key, old_blob).unwrap();
        storage.btree.insert(&mut pager, &key, old_blob).unwrap();

        let new_vector = vec![2.0_f32; 16];
        storage.insert_vector(&mut pager, 7, &new_vector).unwrap();

        let payloads = payloads_for_exact_key(&storage.btree, &pager, &key);
        assert!(
            !payloads.is_empty(),
            "expected at least one tuple for vector key"
        );
        for blob_id in payloads {
            let bytes = BlobStore::read_direct(&pager, blob_id)
                .expect("all blob ids referenced by key should remain readable");
            assert_eq!(
                bytes.len() % 4,
                0,
                "vector blob should keep float-aligned layout"
            );
        }
    }

    #[test]
    fn graph_churn_keeps_single_live_payload_per_key() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("graph-churn.ndb");
        let mut pager = Pager::open(&ndb).unwrap();
        let mut catalog = IndexCatalog::open_or_create(&mut pager).unwrap();
        let def = catalog
            .get_or_create(&mut pager, "__test_hnsw_graph_churn")
            .unwrap();
        let mut storage = PersistentGraphStorage::new(BTree::load(def.root));

        for node in 0..1500u32 {
            let base = node.saturating_sub(4);
            let neighbors: Vec<u32> = (base..=node).collect();
            storage
                .set_neighbors(&mut pager, 0, node, neighbors)
                .unwrap();
        }

        for round in 0..8u32 {
            for node in 0..1500u32 {
                let span = ((node + round) % 12) + 1;
                let start = node.saturating_sub(span);
                let neighbors: Vec<u32> = (start..=node).rev().take(span as usize).collect();
                storage
                    .set_neighbors(&mut pager, 0, node, neighbors)
                    .unwrap();
            }
        }

        for node in [3u32, 17, 129, 511, 1024, 1499] {
            let key = encode_graph_key(0, node);
            let payloads = storage
                .btree
                .exact_key_payloads_full_scan(&mut pager, &key)
                .unwrap();
            assert_eq!(
                payloads.len(),
                1,
                "graph key should not accumulate duplicate tuples: node={node}"
            );

            let bytes = BlobStore::read_direct(&pager, payloads[0]).unwrap();
            assert_eq!(
                bytes.len() % 4,
                0,
                "graph key should point to a valid neighbor blob: node={node}, len={}",
                bytes.len()
            );
        }
    }
}
