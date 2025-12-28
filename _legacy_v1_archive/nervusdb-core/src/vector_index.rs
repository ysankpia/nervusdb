#![cfg(all(feature = "vector", not(target_arch = "wasm32")))]

use crate::Database;
use crate::error::{Error, Result};
use crate::storage::schema::{TABLE_META, TABLE_NODE_PROPS_BINARY};
use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const VECTOR_INDEX_VERSION: &str = "1";
const META_VECTOR_VERSION: &str = "vector.index.version";
const META_VECTOR_CONFIG: &str = "vector.index.config";

const DEFAULT_VECTOR_PROPERTY: &str = "embedding";
const DEFAULT_VECTOR_METRIC: &str = "cosine";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorIndexConfig {
    pub dim: usize,
    pub property: String,
    pub metric: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VectorSidecarMeta {
    version: String,
    config: VectorIndexConfig,
}

pub struct VectorIndex {
    sidecar_path: PathBuf,
    config: VectorIndexConfig,
    dirty: bool,
    index: usearch::Index,
}

impl VectorIndex {
    pub(crate) fn config(&self) -> &VectorIndexConfig {
        &self.config
    }

    pub fn open_or_rebuild(db: &Database, redb_path: &Path) -> Result<Option<Self>> {
        let sidecar_path = redb_path.with_extension("redb.usearch");
        let Some(config) = read_config(db)? else {
            return Ok(None);
        };

        if config.dim == 0 {
            return Ok(None);
        }

        let meta_ok = read_sidecar_meta(&sidecar_path)
            .map(|m| m.version == VECTOR_INDEX_VERSION && m.config == config)
            .unwrap_or(false);

        if meta_ok {
            if let Ok(index) = open_index(&sidecar_path, &config) {
                return Ok(Some(Self {
                    sidecar_path,
                    config,
                    dirty: false,
                    index,
                }));
            }
        }

        // Sidecar missing/corrupt: rebuild from redb, but don't block DB open if it fails.
        let rebuilt = match create_index(&config) {
            Ok(mut index) => match rebuild_index_data(db, &config, &mut index) {
                Ok(()) => index,
                Err(_) => return Ok(None),
            },
            Err(_) => return Ok(None),
        };

        Ok(Some(Self {
            sidecar_path,
            config,
            dirty: true,
            index: rebuilt,
        }))
    }

    pub fn upsert_from_props(
        &mut self,
        node_id: u64,
        props: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let Some(value) = props.get(&self.config.property) else {
            self.remove(node_id)?;
            return Ok(());
        };
        let Some(vec) = json_array_to_f32_vec(value) else {
            self.remove(node_id)?;
            return Ok(());
        };
        if vec.len() != self.config.dim {
            self.remove(node_id)?;
            return Ok(());
        }

        self.add_vector(node_id, &vec)?;
        self.dirty = true;
        Ok(())
    }

    pub fn upsert(&mut self, node_id: u64, vector: Option<&[f32]>) -> Result<()> {
        match vector {
            Some(vec) => self.add_vector(node_id, vec)?,
            None => {
                self.index
                    .remove(node_id)
                    .map_err(|e| Error::Other(format!("failed to remove vector: {e}")))?;
            }
        }
        self.dirty = true;
        Ok(())
    }

    pub fn remove(&mut self, node_id: u64) -> Result<()> {
        self.index
            .remove(node_id)
            .map_err(|e| Error::Other(format!("failed to remove vector: {e}")))?;
        self.dirty = true;
        Ok(())
    }

    pub fn get_vector(&self, node_id: u64) -> Result<Option<Vec<f32>>> {
        let mut out = Vec::new();
        let found = self
            .index
            .export(node_id, &mut out)
            .map_err(|e| Error::Other(format!("failed to export vector: {e}")))?;
        if found == 0 {
            return Ok(None);
        }
        if out.len() != self.config.dim {
            return Err(Error::Other("exported vector dim mismatch".to_string()));
        }
        Ok(Some(out))
    }

    pub fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(u64, f32)>> {
        if query.len() != self.config.dim || limit == 0 {
            return Ok(Vec::new());
        }
        let matches = self
            .index
            .search(query, limit)
            .map_err(|e| Error::Other(format!("vector search failed: {e}")))?;
        let keys = matches.keys;
        let distances = matches.distances;

        let mut out = Vec::with_capacity(keys.len().min(distances.len()));
        let metric = self.config.metric.to_lowercase();
        for (key, dist) in keys.into_iter().zip(distances.into_iter()) {
            let score = match metric.as_str() {
                // usearch defines Cos/IP as a distance: 1 - similarity.
                "cosine" | "cos" | "ip" => 1.0f32 - dist,
                // For L2sq, smaller distance is better; expose as negative distance.
                "l2" | "l2sq" | "euclidean" => -dist,
                _ => 1.0f32 - dist,
            };
            out.push((key, score));
        }
        Ok(out)
    }

    pub fn flush(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let path_str = self
            .sidecar_path
            .to_str()
            .ok_or_else(|| Error::Other("vector sidecar path is not valid UTF-8".to_string()))?;
        self.index
            .save(path_str)
            .map_err(|e| Error::Other(format!("failed to save vector index: {e}")))?;
        write_sidecar_meta(&self.sidecar_path, &self.config)?;
        self.dirty = false;
        Ok(())
    }
}

impl VectorIndex {
    fn add_vector(&self, node_id: u64, vec: &[f32]) -> Result<()> {
        // Avoid `add` crashing on some platforms when capacity is zero.
        if self.index.capacity() == 0 {
            self.index
                .reserve(1)
                .map_err(|e| Error::Other(format!("failed to reserve vector index: {e}")))?;
        } else if self.index.size() + 1 > self.index.capacity() {
            let current = self.index.capacity().max(1);
            let wanted = (current.saturating_mul(2)).max(self.index.size() + 1);
            self.index
                .reserve(wanted)
                .map_err(|e| Error::Other(format!("failed to reserve vector index: {e}")))?;
        }

        if self.index.contains(node_id) {
            let _ = self.index.remove(node_id);
        }

        self.index
            .add(node_id, vec)
            .map_err(|e| Error::Other(format!("failed to add vector: {e}")))?;
        Ok(())
    }
}

pub(crate) fn write_config(db: &Database, config: &VectorIndexConfig) -> Result<()> {
    let tx = db
        .redb
        .begin_write()
        .map_err(|e| Error::Other(e.to_string()))?;
    {
        let mut table = tx
            .open_table(TABLE_META)
            .map_err(|e| Error::Other(e.to_string()))?;
        table
            .insert(META_VECTOR_VERSION, VECTOR_INDEX_VERSION)
            .map_err(|e| Error::Other(e.to_string()))?;
        let json = serde_json::to_string(config)
            .map_err(|e| Error::Other(format!("failed to encode vector config: {e}")))?;
        table
            .insert(META_VECTOR_CONFIG, json.as_str())
            .map_err(|e| Error::Other(e.to_string()))?;
    }
    tx.commit().map_err(|e| Error::Other(e.to_string()))?;
    Ok(())
}

pub(crate) fn clear_config(db: &Database) -> Result<()> {
    let tx = db
        .redb
        .begin_write()
        .map_err(|e| Error::Other(e.to_string()))?;
    {
        let mut table = tx
            .open_table(TABLE_META)
            .map_err(|e| Error::Other(e.to_string()))?;
        table
            .remove(META_VECTOR_VERSION)
            .map_err(|e| Error::Other(e.to_string()))?;
        table
            .remove(META_VECTOR_CONFIG)
            .map_err(|e| Error::Other(e.to_string()))?;
    }
    tx.commit().map_err(|e| Error::Other(e.to_string()))?;
    Ok(())
}

fn read_config(db: &Database) -> Result<Option<VectorIndexConfig>> {
    let tx = db
        .redb
        .begin_read()
        .map_err(|e| Error::Other(e.to_string()))?;
    let table = tx
        .open_table(TABLE_META)
        .map_err(|e| Error::Other(e.to_string()))?;

    let Some(version) = table
        .get(META_VECTOR_VERSION)
        .map_err(|e| Error::Other(e.to_string()))?
        .map(|v| v.value().to_string())
    else {
        return Ok(None);
    };
    if version != VECTOR_INDEX_VERSION {
        return Ok(None);
    }

    let Some(config_json) = table
        .get(META_VECTOR_CONFIG)
        .map_err(|e| Error::Other(e.to_string()))?
        .map(|v| v.value().to_string())
    else {
        return Ok(Some(VectorIndexConfig {
            dim: 0,
            property: DEFAULT_VECTOR_PROPERTY.to_string(),
            metric: DEFAULT_VECTOR_METRIC.to_string(),
        }));
    };

    let mut config: VectorIndexConfig = serde_json::from_str(&config_json)
        .map_err(|e| Error::Other(format!("invalid vector index config: {e}")))?;
    if config.property.is_empty() {
        config.property = DEFAULT_VECTOR_PROPERTY.to_string();
    }
    if config.metric.is_empty() {
        config.metric = DEFAULT_VECTOR_METRIC.to_string();
    }
    Ok(Some(config))
}

fn sidecar_meta_path(sidecar_path: &Path) -> PathBuf {
    // "mydb.redb.usearch" -> "mydb.redb.usearch.meta.json"
    sidecar_path.with_extension("usearch.meta.json")
}

fn read_sidecar_meta(sidecar_path: &Path) -> Option<VectorSidecarMeta> {
    let path = sidecar_meta_path(sidecar_path);
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_sidecar_meta(sidecar_path: &Path, config: &VectorIndexConfig) -> Result<()> {
    let path = sidecar_meta_path(sidecar_path);
    let meta = VectorSidecarMeta {
        version: VECTOR_INDEX_VERSION.to_string(),
        config: config.clone(),
    };
    let bytes = serde_json::to_vec(&meta)
        .map_err(|e| Error::Other(format!("failed to encode vector meta: {e}")))?;
    std::fs::write(path, bytes).map_err(|e| Error::Other(e.to_string()))?;
    Ok(())
}

fn json_array_to_f32_vec(value: &serde_json::Value) -> Option<Vec<f32>> {
    let serde_json::Value::Array(items) = value else {
        return None;
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(item.as_f64()? as f32);
    }
    Some(out)
}

fn rebuild_index_data(
    db: &Database,
    config: &VectorIndexConfig,
    index: &mut usearch::Index,
) -> Result<()> {
    let tx = db
        .redb
        .begin_read()
        .map_err(|e| Error::Other(e.to_string()))?;
    let table = tx
        .open_table(TABLE_NODE_PROPS_BINARY)
        .map_err(|e| Error::Other(e.to_string()))?;

    for entry in table.iter().map_err(|e| Error::Other(e.to_string()))? {
        let (node_id, bytes) = entry.map_err(|e| Error::Other(e.to_string()))?;
        let Ok(props) = crate::storage::property::deserialize_properties(bytes.value()) else {
            continue;
        };
        let Some(vec) = props.get(&config.property).and_then(json_array_to_f32_vec) else {
            continue;
        };
        if vec.len() != config.dim {
            continue;
        }
        index
            .add(node_id.value(), &vec)
            .map_err(|e| Error::Other(format!("failed to add vector: {e}")))?;
    }

    Ok(())
}

fn create_index(config: &VectorIndexConfig) -> Result<usearch::Index> {
    let metric_kind = match config.metric.to_lowercase().as_str() {
        "cosine" | "cos" => usearch::MetricKind::Cos,
        "ip" => usearch::MetricKind::IP,
        "l2" | "l2sq" | "euclidean" => usearch::MetricKind::L2sq,
        _ => usearch::MetricKind::Cos,
    };
    let index = usearch::Index::new(&usearch::IndexOptions {
        dimensions: config.dim,
        metric: metric_kind,
        quantization: usearch::ScalarKind::F32,
        // Conservative defaults (avoid relying on platform-specific implicit choices).
        connectivity: 16,
        expansion_add: 64,
        expansion_search: 64,
        multi: false,
    })
    .map_err(|e| Error::Other(format!("failed to create vector index: {e}")))?;
    // usearch requires an explicit reserve on some platforms (otherwise `add` may hit null buffers).
    index
        .reserve(1)
        .map_err(|e| Error::Other(format!("failed to reserve vector index: {e}")))?;
    Ok(index)
}

fn open_index(path: &Path, config: &VectorIndexConfig) -> Result<usearch::Index> {
    if !path.exists() {
        return Err(Error::Other("vector sidecar missing".to_string()));
    }
    let index = create_index(config)?;
    let path_str = path
        .to_str()
        .ok_or_else(|| Error::Other("vector sidecar path is not valid UTF-8".to_string()))?;
    index
        .load(path_str)
        .map_err(|e| Error::Other(format!("failed to load vector index: {e}")))?;
    Ok(index)
}
