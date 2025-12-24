#![cfg(all(feature = "fts", not(target_arch = "wasm32")))]

use crate::Database;
use crate::error::{Error, Result};
use crate::storage::schema::{TABLE_META, TABLE_NODE_PROPS, TABLE_NODE_PROPS_BINARY};
use lru::LruCache;
use redb::{ReadableDatabase, ReadableTable, WriteTransaction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{
    Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TEXT, TantivyDocument, Value as _,
};
use tantivy::{Index, IndexReader, IndexWriter, Term, doc};

const FTS_INDEX_VERSION: &str = "1";
const META_FTS_VERSION: &str = "fts.index.version";
const META_FTS_CONFIG: &str = "fts.index.config";
const META_FTS_COMMITTED_WRITES: &str = "fts.index.committed_writes";

const DEFAULT_FTS_MODE: &str = "all_string_props";
const TXT_SCORE_CACHE_LIMIT: usize = 128;
const TXT_SCORE_TOP_K: usize = 10_000;
const INDEX_WRITER_HEAP_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FtsIndexConfig {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FtsSidecarMeta {
    version: String,
    config: FtsIndexConfig,
    #[serde(default)]
    flushed_writes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TxtScoreCacheKey {
    property: String,
    query: String,
}

pub struct FtsIndex {
    sidecar_dir: PathBuf,
    config: FtsIndexConfig,
    dirty: bool,
    index: Index,
    writer: IndexWriter,
    reader: IndexReader,
    node_id_field: Field,
    property_field: Field,
    text_field: Field,
    txt_score_cache: Mutex<LruCache<TxtScoreCacheKey, Arc<HashMap<u64, f32>>>>,
}

impl FtsIndex {
    pub fn open_or_rebuild(db: &Database, redb_path: &Path) -> Result<Option<Self>> {
        let sidecar_dir = redb_path.with_extension("redb.tantivy");
        let Some(config) = read_config(db)? else {
            return Ok(None);
        };

        let committed_writes = read_committed_writes(db)?;
        let sidecar_meta = read_sidecar_meta(&sidecar_dir);
        let meta_ok = sidecar_meta
            .as_ref()
            .map(|m| m.version == FTS_INDEX_VERSION && m.config == config)
            .unwrap_or(false);
        let flushed_writes = sidecar_meta.map(|m| m.flushed_writes).unwrap_or(0);
        let up_to_date = committed_writes == flushed_writes;

        if meta_ok && up_to_date {
            if let Ok(index) = Index::open_in_dir(&sidecar_dir) {
                if let Ok(existing) =
                    Self::open_existing(sidecar_dir.clone(), config.clone(), index)
                {
                    return Ok(Some(existing));
                }
            }
        }

        // Sidecar missing/corrupt: rebuild from redb, but don't block DB open if it fails.
        let rebuilt = match Self::rebuild(db, &sidecar_dir, config) {
            Ok(index) => index,
            Err(_) => return Ok(None),
        };
        Ok(Some(rebuilt))
    }

    fn open_existing(sidecar_dir: PathBuf, config: FtsIndexConfig, index: Index) -> Result<Self> {
        let schema = index.schema();
        let node_id_field = schema
            .get_field("node_id")
            .map_err(|_| Error::Other("fts schema missing node_id field".to_string()))?;
        let property_field = schema
            .get_field("property")
            .map_err(|_| Error::Other("fts schema missing property field".to_string()))?;
        let text_field = schema
            .get_field("text")
            .map_err(|_| Error::Other("fts schema missing text field".to_string()))?;

        let writer = index
            .writer(INDEX_WRITER_HEAP_BYTES)
            .map_err(|e| Error::Other(format!("failed to create tantivy writer: {e}")))?;
        let reader = index
            .reader_builder()
            .try_into()
            .map_err(|e| Error::Other(format!("failed to create tantivy reader: {e}")))?;

        Ok(Self {
            sidecar_dir,
            config,
            dirty: false,
            index,
            writer,
            reader,
            node_id_field,
            property_field,
            text_field,
            txt_score_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(TXT_SCORE_CACHE_LIMIT).expect("cache limit must be > 0"),
            )),
        })
    }

    fn rebuild(db: &Database, sidecar_dir: &Path, config: FtsIndexConfig) -> Result<Self> {
        prepare_sidecar_dir_for_rebuild(sidecar_dir)?;
        std::fs::create_dir_all(sidecar_dir).map_err(|e| Error::Other(e.to_string()))?;

        let (index, node_id_field, property_field, text_field) = create_index(sidecar_dir)?;
        let writer = index
            .writer(INDEX_WRITER_HEAP_BYTES)
            .map_err(|e| Error::Other(format!("failed to create tantivy writer: {e}")))?;
        let reader = index
            .reader_builder()
            .try_into()
            .map_err(|e| Error::Other(format!("failed to create tantivy reader: {e}")))?;

        let mut fts = Self {
            sidecar_dir: sidecar_dir.to_path_buf(),
            config,
            dirty: false,
            index,
            writer,
            reader,
            node_id_field,
            property_field,
            text_field,
            txt_score_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(TXT_SCORE_CACHE_LIMIT).expect("cache limit must be > 0"),
            )),
        };

        if fts.rebuild_index_data(db).is_err() {
            return Err(Error::Other("failed to rebuild fts index".to_string()));
        }
        let committed_writes = read_committed_writes(db)?;
        fts.flush(committed_writes)?;
        Ok(fts)
    }

    fn rebuild_index_data(&mut self, db: &Database) -> Result<()> {
        let tx = db
            .redb
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;

        // Legacy string table first (v1.x)
        if let Ok(table) = tx.open_table(TABLE_NODE_PROPS) {
            for entry in table.iter().map_err(|e| Error::Other(e.to_string()))? {
                let (node_id, json) = entry.map_err(|e| Error::Other(e.to_string()))?;
                let Ok(props) =
                    serde_json::from_str::<HashMap<String, serde_json::Value>>(json.value())
                else {
                    continue;
                };
                let _ = self.upsert_from_props(node_id.value(), &props);
            }
        }

        // Binary FlexBuffers table (v2.0)
        let table = tx
            .open_table(TABLE_NODE_PROPS_BINARY)
            .map_err(|e| Error::Other(e.to_string()))?;
        for entry in table.iter().map_err(|e| Error::Other(e.to_string()))? {
            let (node_id, binary) = entry.map_err(|e| Error::Other(e.to_string()))?;
            let Ok(props) = crate::storage::property::deserialize_properties(binary.value()) else {
                continue;
            };
            let _ = self.upsert_from_props(node_id.value(), &props);
        }

        Ok(())
    }

    pub fn upsert_from_props(
        &mut self,
        node_id: u64,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        self.writer
            .delete_term(Term::from_field_u64(self.node_id_field, node_id));

        for (property, value) in props {
            let serde_json::Value::String(text) = value else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let doc = doc!(
                self.node_id_field => node_id,
                self.property_field => property.as_str(),
                self.text_field => text.as_str(),
            );
            self.writer.add_document(doc).map_err(|e| {
                Error::Other(format!(
                    "failed to add tantivy document for node {node_id}: {e}"
                ))
            })?;
        }

        self.dirty = true;
        self.clear_txt_score_cache();
        Ok(())
    }

    pub fn delete_node(&mut self, node_id: u64) -> Result<()> {
        self.writer
            .delete_term(Term::from_field_u64(self.node_id_field, node_id));
        self.dirty = true;
        self.clear_txt_score_cache();
        Ok(())
    }

    pub fn txt_score(&self, node_id: u64, property: &str, query: &str) -> Result<f32> {
        let scores = self.scores_for_query(property, query)?;
        Ok(*scores.get(&node_id).unwrap_or(&0.0))
    }

    pub(crate) fn scores_for_query(
        &self,
        property: &str,
        query: &str,
    ) -> Result<Arc<HashMap<u64, f32>>> {
        if property.is_empty() || query.is_empty() {
            return Ok(Arc::new(HashMap::new()));
        }

        let key = TxtScoreCacheKey {
            property: property.to_string(),
            query: query.to_string(),
        };

        let cached = {
            let mut cache = self
                .txt_score_cache
                .lock()
                .map_err(|_| Error::Other("txt_score cache lock poisoned".to_string()))?;
            cache.get(&key).cloned()
        };

        if let Some(scores) = cached {
            return Ok(scores);
        }

        let scores = Arc::new(self.search_scores(property, query)?);
        let mut cache = self
            .txt_score_cache
            .lock()
            .map_err(|_| Error::Other("txt_score cache lock poisoned".to_string()))?;
        cache.put(key, Arc::clone(&scores));
        Ok(scores)
    }

    fn search_scores(&self, property: &str, query: &str) -> Result<HashMap<u64, f32>> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.text_field]);
        let Ok(text_query) = parser.parse_query(query) else {
            return Ok(HashMap::new());
        };

        let term = Term::from_field_text(self.property_field, property);
        let prop_query = TermQuery::new(term, IndexRecordOption::Basic);
        let query = BooleanQuery::new(vec![
            (Occur::Must, Box::new(prop_query)),
            (Occur::Must, text_query),
        ]);

        let limit = TXT_SCORE_TOP_K.min(searcher.num_docs() as usize);
        if limit == 0 {
            return Ok(HashMap::new());
        }
        let hits = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| Error::Other(format!("tantivy search failed: {e}")))?;

        let mut out = HashMap::with_capacity(hits.len());
        for (score, addr) in hits {
            let doc: TantivyDocument = searcher
                .doc(addr)
                .map_err(|e| Error::Other(format!("tantivy doc fetch failed: {e}")))?;
            let Some(node_id) = doc.get_first(self.node_id_field).and_then(|v| v.as_u64()) else {
                continue;
            };
            out.insert(node_id, score);
        }
        Ok(out)
    }

    pub fn flush(&mut self, flushed_writes: u64) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        self.writer
            .commit()
            .map_err(|e| Error::Other(format!("failed to commit tantivy index: {e}")))?;
        self.reader
            .reload()
            .map_err(|e| Error::Other(format!("failed to reload tantivy reader: {e}")))?;
        write_sidecar_meta(&self.sidecar_dir, &self.config, flushed_writes)?;

        self.dirty = false;
        Ok(())
    }

    fn clear_txt_score_cache(&self) {
        let Ok(mut cache) = self.txt_score_cache.lock() else {
            return;
        };
        cache.clear();
    }
}

pub(crate) fn write_config(db: &Database, config: &FtsIndexConfig) -> Result<()> {
    let tx = db
        .redb
        .begin_write()
        .map_err(|e| Error::Other(e.to_string()))?;
    {
        let mut table = tx
            .open_table(TABLE_META)
            .map_err(|e| Error::Other(e.to_string()))?;
        table
            .insert(META_FTS_VERSION, FTS_INDEX_VERSION)
            .map_err(|e| Error::Other(e.to_string()))?;
        let json = serde_json::to_string(config)
            .map_err(|e| Error::Other(format!("failed to encode fts config: {e}")))?;
        table
            .insert(META_FTS_CONFIG, json.as_str())
            .map_err(|e| Error::Other(e.to_string()))?;
        if table
            .get(META_FTS_COMMITTED_WRITES)
            .map_err(|e| Error::Other(e.to_string()))?
            .is_none()
        {
            table
                .insert(META_FTS_COMMITTED_WRITES, "0")
                .map_err(|e| Error::Other(e.to_string()))?;
        }
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
            .remove(META_FTS_VERSION)
            .map_err(|e| Error::Other(e.to_string()))?;
        table
            .remove(META_FTS_CONFIG)
            .map_err(|e| Error::Other(e.to_string()))?;
        table
            .remove(META_FTS_COMMITTED_WRITES)
            .map_err(|e| Error::Other(e.to_string()))?;
    }
    tx.commit().map_err(|e| Error::Other(e.to_string()))?;
    Ok(())
}

pub(crate) fn read_committed_writes(db: &Database) -> Result<u64> {
    let tx = db
        .redb
        .begin_read()
        .map_err(|e| Error::Other(e.to_string()))?;
    let table = tx
        .open_table(TABLE_META)
        .map_err(|e| Error::Other(e.to_string()))?;

    let Some(version) = table
        .get(META_FTS_VERSION)
        .map_err(|e| Error::Other(e.to_string()))?
        .map(|v| v.value().to_string())
    else {
        return Ok(0);
    };
    if version != FTS_INDEX_VERSION {
        return Ok(0);
    }

    Ok(table
        .get(META_FTS_COMMITTED_WRITES)
        .map_err(|e| Error::Other(e.to_string()))?
        .and_then(|v| v.value().parse::<u64>().ok())
        .unwrap_or(0))
}

pub(crate) fn bump_committed_writes_in_txn(tx: &WriteTransaction, delta: u64) -> Result<()> {
    let mut table = tx
        .open_table(TABLE_META)
        .map_err(|e| Error::Other(e.to_string()))?;
    let Some(version) = table
        .get(META_FTS_VERSION)
        .map_err(|e| Error::Other(e.to_string()))?
        .map(|v| v.value().to_string())
    else {
        return Ok(());
    };
    if version != FTS_INDEX_VERSION {
        return Ok(());
    }

    let current = table
        .get(META_FTS_COMMITTED_WRITES)
        .map_err(|e| Error::Other(e.to_string()))?
        .and_then(|v| v.value().parse::<u64>().ok())
        .unwrap_or(0);
    let next = current.saturating_add(delta);
    let next_str = next.to_string();
    table
        .insert(META_FTS_COMMITTED_WRITES, next_str.as_str())
        .map_err(|e| Error::Other(e.to_string()))?;
    Ok(())
}

fn read_config(db: &Database) -> Result<Option<FtsIndexConfig>> {
    let tx = db
        .redb
        .begin_read()
        .map_err(|e| Error::Other(e.to_string()))?;
    let table = tx
        .open_table(TABLE_META)
        .map_err(|e| Error::Other(e.to_string()))?;

    let Some(version) = table
        .get(META_FTS_VERSION)
        .map_err(|e| Error::Other(e.to_string()))?
        .map(|v| v.value().to_string())
    else {
        return Ok(None);
    };
    if version != FTS_INDEX_VERSION {
        return Ok(None);
    }

    let Some(config_json) = table
        .get(META_FTS_CONFIG)
        .map_err(|e| Error::Other(e.to_string()))?
        .map(|v| v.value().to_string())
    else {
        return Ok(Some(FtsIndexConfig {
            mode: DEFAULT_FTS_MODE.to_string(),
        }));
    };

    let mut config: FtsIndexConfig = serde_json::from_str(&config_json)
        .map_err(|e| Error::Other(format!("invalid fts index config: {e}")))?;
    if config.mode.is_empty() {
        config.mode = DEFAULT_FTS_MODE.to_string();
    }
    Ok(Some(config))
}

fn create_index(dir: &Path) -> Result<(Index, Field, Field, Field)> {
    let mut schema_builder = Schema::builder();
    let node_id_field = schema_builder.add_u64_field("node_id", INDEXED | STORED);
    let property_field = schema_builder.add_text_field("property", STRING | STORED);
    let text_field = schema_builder.add_text_field("text", TEXT);
    let schema = schema_builder.build();

    let index = Index::create_in_dir(dir, schema)
        .map_err(|e| Error::Other(format!("failed to create tantivy index: {e}")))?;
    Ok((index, node_id_field, property_field, text_field))
}

fn sidecar_meta_path(sidecar_dir: &Path) -> PathBuf {
    sidecar_dir.join("nervusdb.fts.meta.json")
}

fn read_sidecar_meta(sidecar_dir: &Path) -> Option<FtsSidecarMeta> {
    let path = sidecar_meta_path(sidecar_dir);
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_sidecar_meta(
    sidecar_dir: &Path,
    config: &FtsIndexConfig,
    flushed_writes: u64,
) -> Result<()> {
    let path = sidecar_meta_path(sidecar_dir);
    let meta = FtsSidecarMeta {
        version: FTS_INDEX_VERSION.to_string(),
        config: config.clone(),
        flushed_writes,
    };
    let bytes = serde_json::to_vec(&meta)
        .map_err(|e| Error::Other(format!("failed to encode fts meta: {e}")))?;
    std::fs::write(path, bytes).map_err(|e| Error::Other(e.to_string()))?;
    Ok(())
}

fn prepare_sidecar_dir_for_rebuild(sidecar_dir: &Path) -> Result<()> {
    if !sidecar_dir.exists() {
        return Ok(());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let corrupt_path = sidecar_dir.with_extension(format!("tantivy.corrupt.{now}"));
    std::fs::rename(sidecar_dir, corrupt_path).map_err(|e| Error::Other(e.to_string()))?;
    Ok(())
}
