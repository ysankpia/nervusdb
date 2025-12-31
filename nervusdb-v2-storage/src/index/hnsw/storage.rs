use crate::blob_store::BlobStore;
use crate::index::btree::BTree;
use crate::pager::Pager;
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
        self.btree.insert(pager, &key, blob_id)
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
        self.btree.insert(pager, &key, blob_id)
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
        self.btree.insert(pager, &key, blob_id)
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
