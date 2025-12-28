//! Temporal Store v2 - Multi-table Architecture
//!
//! 解决 v1 的致命缺陷：
//! - v1: 整个数据集序列化为单行 JSON Blob，O(N) 读写
//! - v2: 每个实体类型独立表，O(1) 读写，支持并发
//!
//! 表结构设计：
//! - tm_counters: 全局计数器 (单行)
//! - tm_episodes: Episode 记录 (id -> bincode)
//! - tm_entities: Entity 记录 (id -> bincode)
//! - tm_aliases: Alias 记录 (id -> bincode)
//! - tm_facts: Fact 记录 (id -> bincode)
//! - tm_links: Episode Link 记录 (id -> bincode)
//!
//! 索引表：
//! - tm_idx_trace_hash: trace_hash -> episode_id (去重)
//! - tm_idx_fingerprint: fingerprint -> entity_id (去重)
//! - tm_idx_fact_subject: subject_entity_id -> fact_ids (时间线查询)
//! - tm_idx_fact_object: object_entity_id -> fact_ids (反向查询)

use std::collections::HashSet;
use std::sync::Arc;

use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

// ============================================================================
// Error Types
// ============================================================================

/// Error type for temporal store operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] redb::Error),
    #[error("Transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),
    #[error("Table error: {0}")]
    Table(#[from] redb::TableError),
    #[error("Storage error: {0}")]
    Storage(#[from] redb::StorageError),
    #[error("Commit error: {0}")]
    Commit(#[from] redb::CommitError),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// ============================================================================
// Table Definitions
// ============================================================================

// 主数据表 - 使用 bincode 序列化
const TABLE_COUNTERS: TableDefinition<&str, u64> = TableDefinition::new("tm_counters");
const TABLE_EPISODES: TableDefinition<u64, &[u8]> = TableDefinition::new("tm_episodes");
const TABLE_ENTITIES: TableDefinition<u64, &[u8]> = TableDefinition::new("tm_entities");
const TABLE_ALIASES: TableDefinition<u64, &[u8]> = TableDefinition::new("tm_aliases");
const TABLE_FACTS: TableDefinition<u64, &[u8]> = TableDefinition::new("tm_facts");
const TABLE_LINKS: TableDefinition<u64, &[u8]> = TableDefinition::new("tm_links");

// 索引表 - 用于快速查找
const TABLE_IDX_TRACE_HASH: TableDefinition<&str, u64> = TableDefinition::new("tm_idx_trace_hash");
const TABLE_IDX_FINGERPRINT: TableDefinition<&str, u64> =
    TableDefinition::new("tm_idx_fingerprint");
// 复合索引：subject_entity_id:fact_id -> () (用于遍历)
const TABLE_IDX_FACT_SUBJECT: TableDefinition<(u64, u64), ()> =
    TableDefinition::new("tm_idx_fact_subject");
const TABLE_IDX_FACT_OBJECT: TableDefinition<(u64, u64), ()> =
    TableDefinition::new("tm_idx_fact_object");
// Fact 去重索引：subject_id:predicate:object_id:object_value_hash -> fact_id
const TABLE_IDX_FACT_DEDUP: TableDefinition<&str, u64> = TableDefinition::new("tm_idx_fact_dedup");

// Legacy 表 - 用于迁移检测
const TABLE_TEMPORAL_BLOB_LEGACY: TableDefinition<u64, &str> =
    TableDefinition::new("temporal_blob");

// ============================================================================
// Data Types (与 v1 兼容)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeInput {
    pub source_type: String,
    pub payload: Value,
    pub occurred_at: String,
    pub trace_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEpisode {
    pub episode_id: u64,
    pub source_type: String,
    pub payload: Value,
    pub occurred_at: String,
    pub ingested_at: String,
    pub trace_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEntity {
    pub entity_id: u64,
    pub kind: String,
    pub canonical_name: String,
    pub fingerprint: String,
    pub first_seen: String,
    pub last_seen: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAlias {
    pub alias_id: u64,
    pub entity_id: u64,
    pub alias_text: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnsureEntityOptions {
    pub alias: Option<String>,
    pub confidence: Option<f64>,
    pub occurred_at: Option<String>,
    pub version_increment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactWriteInput {
    pub subject_entity_id: u64,
    pub predicate_key: String,
    pub object_entity_id: Option<u64>,
    pub object_value: Option<Value>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub confidence: Option<f64>,
    pub source_episode_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFact {
    pub fact_id: u64,
    pub subject_entity_id: u64,
    pub predicate_key: String,
    pub object_entity_id: Option<u64>,
    pub object_value: Option<Value>,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub source_episode_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeLinkRecord {
    pub link_id: u64,
    pub episode_id: u64,
    pub entity_id: Option<u64>,
    pub fact_id: Option<u64>,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimelineRole {
    Subject,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimelineQuery {
    pub entity_id: u64,
    pub predicate_key: Option<String>,
    pub role: Option<TimelineRole>,
    pub as_of: Option<String>,
    pub between: Option<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeLinkOptions {
    pub entity_id: Option<u64>,
    pub fact_id: Option<u64>,
    pub role: String,
}

// Legacy 数据结构 (用于迁移)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LegacyCounters {
    episode: u64,
    entity: u64,
    alias: u64,
    fact: u64,
    link: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LegacyTemporalData {
    counters: LegacyCounters,
    episodes: Vec<StoredEpisode>,
    entities: Vec<StoredEntity>,
    aliases: Vec<StoredAlias>,
    facts: Vec<StoredFact>,
    links: Vec<EpisodeLinkRecord>,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_timestamp(value: Option<&str>) -> Result<OffsetDateTime> {
    match value {
        Some(ts) => OffsetDateTime::parse(ts, &Rfc3339)
            .map_err(|err| Error::Other(format!("invalid timestamp {ts}: {err}"))),
        None => Ok(OffsetDateTime::now_utc()),
    }
}

fn canonicalize_timestamp(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .expect("formatting RFC3339 timestamp should not fail")
}

fn compute_trace_hash(input: &EpisodeInput, occurred_at: &str) -> Result<String> {
    if let Some(hash) = &input.trace_hash {
        return Ok(hash.clone());
    }
    let payload = serde_json::to_string(&input.payload)
        .map_err(|err| Error::Other(format!("failed to serialize episode payload: {err}")))?;
    let mut hasher = Sha256::new();
    hasher.update(&input.source_type);
    hasher.update(occurred_at);
    hasher.update(payload);
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

fn compute_fact_dedup_key(fact: &FactWriteInput) -> String {
    let object_hash = match &fact.object_value {
        Some(v) => {
            let mut hasher = Sha256::new();
            hasher.update(v.to_string());
            format!("{:x}", hasher.finalize())
        }
        None => "null".to_string(),
    };
    format!(
        "{}:{}:{}:{}",
        fact.subject_entity_id,
        fact.predicate_key,
        fact.object_entity_id
            .map_or("null".to_string(), |id| id.to_string()),
        object_hash
    )
}

// ============================================================================
// Temporal Store v2
// ============================================================================

#[derive(Debug)]
pub struct TemporalStoreV2 {
    db: Arc<redb::Database>,
}

impl TemporalStoreV2 {
    /// 打开 Temporal Store，自动检测并迁移 Legacy 数据
    pub fn open(db: Arc<redb::Database>) -> Result<Self> {
        let store = Self { db };
        store.init_tables()?;
        store.migrate_if_needed()?;
        Ok(store)
    }

    /// 初始化所有表
    fn init_tables(&self) -> Result<()> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let _ = write_txn
                .open_table(TABLE_COUNTERS)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_EPISODES)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_ENTITIES)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_ALIASES)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_FACTS)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_LINKS)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_IDX_TRACE_HASH)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_IDX_FINGERPRINT)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_IDX_FACT_SUBJECT)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_IDX_FACT_OBJECT)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_IDX_FACT_DEDUP)
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    /// 检测并迁移 Legacy JSON Blob 数据
    fn migrate_if_needed(&self) -> Result<()> {
        // 检查是否有 Legacy 数据
        let legacy_data = {
            let read_txn = self
                .db
                .begin_read()
                .map_err(|e| Error::Other(e.to_string()))?;

            // 尝试打开 legacy 表
            let legacy_table = match read_txn.open_table(TABLE_TEMPORAL_BLOB_LEGACY) {
                Ok(t) => t,
                Err(_) => return Ok(()), // 没有 legacy 表，无需迁移
            };

            // 检查是否有数据
            match legacy_table
                .get(1)
                .map_err(|e| Error::Other(e.to_string()))?
            {
                Some(val) => {
                    let data: LegacyTemporalData = serde_json::from_str(val.value())
                        .map_err(|e| Error::Other(format!("failed to parse legacy data: {e}")))?;
                    if data.episodes.is_empty() && data.entities.is_empty() && data.facts.is_empty()
                    {
                        return Ok(()); // 空数据，无需迁移
                    }
                    Some(data)
                }
                None => return Ok(()), // 没有数据，无需迁移
            }
        };

        // 检查 v2 表是否已有数据（避免重复迁移）
        {
            let read_txn = self
                .db
                .begin_read()
                .map_err(|e| Error::Other(e.to_string()))?;
            let counters = read_txn
                .open_table(TABLE_COUNTERS)
                .map_err(|e| Error::Other(e.to_string()))?;
            if counters
                .get("episode")
                .map_err(|e| Error::Other(e.to_string()))?
                .is_some()
            {
                // v2 表已有数据，跳过迁移
                return Ok(());
            }
        }

        // 执行迁移
        if let Some(data) = legacy_data {
            self.migrate_legacy_data(data)?;
        }

        Ok(())
    }

    /// 迁移 Legacy 数据到 v2 表结构
    fn migrate_legacy_data(&self, data: LegacyTemporalData) -> Result<()> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        {
            // 迁移计数器
            let mut counters = write_txn
                .open_table(TABLE_COUNTERS)
                .map_err(|e| Error::Other(e.to_string()))?;
            counters
                .insert("episode", data.counters.episode)
                .map_err(|e| Error::Other(e.to_string()))?;
            counters
                .insert("entity", data.counters.entity)
                .map_err(|e| Error::Other(e.to_string()))?;
            counters
                .insert("alias", data.counters.alias)
                .map_err(|e| Error::Other(e.to_string()))?;
            counters
                .insert("fact", data.counters.fact)
                .map_err(|e| Error::Other(e.to_string()))?;
            counters
                .insert("link", data.counters.link)
                .map_err(|e| Error::Other(e.to_string()))?;

            // 迁移 Episodes
            let mut episodes = write_txn
                .open_table(TABLE_EPISODES)
                .map_err(|e| Error::Other(e.to_string()))?;
            let mut idx_trace = write_txn
                .open_table(TABLE_IDX_TRACE_HASH)
                .map_err(|e| Error::Other(e.to_string()))?;
            for episode in &data.episodes {
                let bytes = rmp_serde::to_vec(episode)
                    .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                episodes
                    .insert(episode.episode_id, bytes.as_slice())
                    .map_err(|e| Error::Other(e.to_string()))?;
                idx_trace
                    .insert(episode.trace_hash.as_str(), episode.episode_id)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            // 迁移 Entities
            let mut entities = write_txn
                .open_table(TABLE_ENTITIES)
                .map_err(|e| Error::Other(e.to_string()))?;
            let mut idx_fp = write_txn
                .open_table(TABLE_IDX_FINGERPRINT)
                .map_err(|e| Error::Other(e.to_string()))?;
            for entity in &data.entities {
                let bytes = rmp_serde::to_vec(entity)
                    .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                entities
                    .insert(entity.entity_id, bytes.as_slice())
                    .map_err(|e| Error::Other(e.to_string()))?;
                idx_fp
                    .insert(entity.fingerprint.as_str(), entity.entity_id)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            // 迁移 Aliases
            let mut aliases = write_txn
                .open_table(TABLE_ALIASES)
                .map_err(|e| Error::Other(e.to_string()))?;
            for alias in &data.aliases {
                let bytes = rmp_serde::to_vec(alias)
                    .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                aliases
                    .insert(alias.alias_id, bytes.as_slice())
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            // 迁移 Facts
            let mut facts = write_txn
                .open_table(TABLE_FACTS)
                .map_err(|e| Error::Other(e.to_string()))?;
            let mut idx_subj = write_txn
                .open_table(TABLE_IDX_FACT_SUBJECT)
                .map_err(|e| Error::Other(e.to_string()))?;
            let mut idx_obj = write_txn
                .open_table(TABLE_IDX_FACT_OBJECT)
                .map_err(|e| Error::Other(e.to_string()))?;
            let mut idx_dedup = write_txn
                .open_table(TABLE_IDX_FACT_DEDUP)
                .map_err(|e| Error::Other(e.to_string()))?;
            for fact in &data.facts {
                let bytes = rmp_serde::to_vec(fact)
                    .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                facts
                    .insert(fact.fact_id, bytes.as_slice())
                    .map_err(|e| Error::Other(e.to_string()))?;

                // 索引
                idx_subj
                    .insert((fact.subject_entity_id, fact.fact_id), ())
                    .map_err(|e| Error::Other(e.to_string()))?;
                if let Some(obj_id) = fact.object_entity_id {
                    idx_obj
                        .insert((obj_id, fact.fact_id), ())
                        .map_err(|e| Error::Other(e.to_string()))?;
                }

                // 去重索引
                let dedup_key = format!(
                    "{}:{}:{}:{}",
                    fact.subject_entity_id,
                    fact.predicate_key,
                    fact.object_entity_id
                        .map_or("null".to_string(), |id| id.to_string()),
                    fact.object_value
                        .as_ref()
                        .map_or("null".to_string(), |v| v.to_string())
                );
                idx_dedup
                    .insert(dedup_key.as_str(), fact.fact_id)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            // 迁移 Links
            let mut links = write_txn
                .open_table(TABLE_LINKS)
                .map_err(|e| Error::Other(e.to_string()))?;
            for link in &data.links {
                let bytes = rmp_serde::to_vec(link)
                    .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                links
                    .insert(link.link_id, bytes.as_slice())
                    .map_err(|e| Error::Other(e.to_string()))?;
            }
        }

        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(())
    }

    // ========================================================================
    // Counter Operations
    // ========================================================================

    fn next_id(&self, txn: &redb::WriteTransaction, key: &str) -> Result<u64> {
        let mut table = txn
            .open_table(TABLE_COUNTERS)
            .map_err(|e| Error::Other(e.to_string()))?;
        let current = table
            .get(key)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value())
            .unwrap_or(0);
        let next = current + 1;
        table
            .insert(key, next)
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(next)
    }

    // ========================================================================
    // Episode Operations
    // ========================================================================

    pub fn add_episode(&self, input: EpisodeInput) -> Result<StoredEpisode> {
        let occurred_at_dt = OffsetDateTime::parse(&input.occurred_at, &Rfc3339)
            .map_err(|err| Error::Other(format!("invalid occurred_at: {err}")))?;
        let occurred_at = canonicalize_timestamp(occurred_at_dt);
        let trace_hash = compute_trace_hash(&input, &occurred_at)?;

        // 检查去重
        {
            let read_txn = self
                .db
                .begin_read()
                .map_err(|e| Error::Other(e.to_string()))?;
            let idx = read_txn
                .open_table(TABLE_IDX_TRACE_HASH)
                .map_err(|e| Error::Other(e.to_string()))?;
            if let Some(existing_id) = idx
                .get(trace_hash.as_str())
                .map_err(|e| Error::Other(e.to_string()))?
            {
                // 返回已存在的 Episode
                let episodes = read_txn
                    .open_table(TABLE_EPISODES)
                    .map_err(|e| Error::Other(e.to_string()))?;
                if let Some(data) = episodes
                    .get(existing_id.value())
                    .map_err(|e| Error::Other(e.to_string()))?
                {
                    let episode: StoredEpisode = rmp_serde::from_slice(data.value())
                        .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                    return Ok(episode);
                }
            }
        }

        // 创建新 Episode
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let episode = {
            let episode_id = self.next_id(&write_txn, "episode")?;
            let episode = StoredEpisode {
                episode_id,
                source_type: input.source_type,
                payload: input.payload,
                occurred_at,
                ingested_at: canonicalize_timestamp(OffsetDateTime::now_utc()),
                trace_hash: trace_hash.clone(),
            };

            let bytes = rmp_serde::to_vec(&episode)
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;

            let mut episodes = write_txn
                .open_table(TABLE_EPISODES)
                .map_err(|e| Error::Other(e.to_string()))?;
            episodes
                .insert(episode_id, bytes.as_slice())
                .map_err(|e| Error::Other(e.to_string()))?;

            let mut idx = write_txn
                .open_table(TABLE_IDX_TRACE_HASH)
                .map_err(|e| Error::Other(e.to_string()))?;
            idx.insert(trace_hash.as_str(), episode_id)
                .map_err(|e| Error::Other(e.to_string()))?;

            episode
        };

        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(episode)
    }

    // ========================================================================
    // Entity Operations
    // ========================================================================

    pub fn ensure_entity(
        &self,
        kind: &str,
        canonical_name: &str,
        options: EnsureEntityOptions,
    ) -> Result<StoredEntity> {
        let fingerprint = format!("{}:{}", kind, canonical_name.to_ascii_lowercase());
        let occurred_dt = parse_timestamp(options.occurred_at.as_deref())?;
        let occurred_at = canonicalize_timestamp(occurred_dt);

        // 检查是否已存在
        {
            let read_txn = self
                .db
                .begin_read()
                .map_err(|e| Error::Other(e.to_string()))?;
            let idx = read_txn
                .open_table(TABLE_IDX_FINGERPRINT)
                .map_err(|e| Error::Other(e.to_string()))?;

            if let Some(existing_id) = idx
                .get(fingerprint.as_str())
                .map_err(|e| Error::Other(e.to_string()))?
            {
                let entity_id = existing_id.value();
                let entities = read_txn
                    .open_table(TABLE_ENTITIES)
                    .map_err(|e| Error::Other(e.to_string()))?;

                if let Some(data) = entities
                    .get(entity_id)
                    .map_err(|e| Error::Other(e.to_string()))?
                {
                    let mut entity: StoredEntity = rmp_serde::from_slice(data.value())
                        .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;

                    // 检查是否需要更新
                    let mut changed = false;
                    if options.version_increment {
                        entity.version += 1;
                        changed = true;
                    }
                    if occurred_at < entity.first_seen {
                        entity.first_seen = occurred_at.clone();
                        changed = true;
                    }
                    if occurred_at > entity.last_seen {
                        entity.last_seen = occurred_at.clone();
                        changed = true;
                    }

                    if changed {
                        // 更新实体
                        drop(entities);
                        drop(idx);
                        drop(read_txn);

                        let write_txn = self
                            .db
                            .begin_write()
                            .map_err(|e| Error::Other(e.to_string()))?;
                        {
                            let mut entities = write_txn
                                .open_table(TABLE_ENTITIES)
                                .map_err(|e| Error::Other(e.to_string()))?;
                            let bytes = rmp_serde::to_vec(&entity)
                                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                            entities
                                .insert(entity_id, bytes.as_slice())
                                .map_err(|e| Error::Other(e.to_string()))?;
                        }
                        write_txn
                            .commit()
                            .map_err(|e| Error::Other(e.to_string()))?;
                    }

                    // 处理 alias
                    if let Some(alias_text) = options.alias {
                        self.ensure_alias(entity.entity_id, alias_text, options.confidence)?;
                    }

                    return Ok(entity);
                }
            }
        }

        // 创建新 Entity
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let entity = {
            let entity_id = self.next_id(&write_txn, "entity")?;
            let entity = StoredEntity {
                entity_id,
                kind: kind.to_string(),
                canonical_name: canonical_name.to_string(),
                fingerprint: fingerprint.clone(),
                first_seen: occurred_at.clone(),
                last_seen: occurred_at,
                version: 1,
            };

            let bytes = rmp_serde::to_vec(&entity)
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;

            let mut entities = write_txn
                .open_table(TABLE_ENTITIES)
                .map_err(|e| Error::Other(e.to_string()))?;
            entities
                .insert(entity_id, bytes.as_slice())
                .map_err(|e| Error::Other(e.to_string()))?;

            let mut idx = write_txn
                .open_table(TABLE_IDX_FINGERPRINT)
                .map_err(|e| Error::Other(e.to_string()))?;
            idx.insert(fingerprint.as_str(), entity_id)
                .map_err(|e| Error::Other(e.to_string()))?;

            entity
        };

        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;

        // 处理 alias
        if let Some(alias_text) = options.alias {
            self.ensure_alias(entity.entity_id, alias_text, options.confidence)?;
        }

        Ok(entity)
    }

    fn ensure_alias(
        &self,
        entity_id: u64,
        alias_text: String,
        confidence: Option<f64>,
    ) -> Result<StoredAlias> {
        // 检查是否已存在
        {
            let read_txn = self
                .db
                .begin_read()
                .map_err(|e| Error::Other(e.to_string()))?;
            let aliases = read_txn
                .open_table(TABLE_ALIASES)
                .map_err(|e| Error::Other(e.to_string()))?;

            // 遍历查找（后续可优化为索引）
            for item in aliases.iter().map_err(|e| Error::Other(e.to_string()))? {
                let (_, data) = item.map_err(|e| Error::Other(e.to_string()))?;
                let alias: StoredAlias = rmp_serde::from_slice(data.value())
                    .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                if alias.entity_id == entity_id && alias.alias_text == alias_text {
                    return Ok(alias);
                }
            }
        }

        // 创建新 Alias
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let alias = {
            let alias_id = self.next_id(&write_txn, "alias")?;
            let alias = StoredAlias {
                alias_id,
                entity_id,
                alias_text,
                confidence: confidence.unwrap_or(1.0),
            };

            let bytes = rmp_serde::to_vec(&alias)
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;

            let mut aliases = write_txn
                .open_table(TABLE_ALIASES)
                .map_err(|e| Error::Other(e.to_string()))?;
            aliases
                .insert(alias_id, bytes.as_slice())
                .map_err(|e| Error::Other(e.to_string()))?;

            alias
        };

        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(alias)
    }

    // ========================================================================
    // Fact Operations
    // ========================================================================

    pub fn upsert_fact(&self, input: FactWriteInput) -> Result<StoredFact> {
        let valid_from_ts = parse_timestamp(input.valid_from.as_deref())?;
        let valid_from = canonicalize_timestamp(valid_from_ts);
        let valid_to = if let Some(ts) = &input.valid_to {
            Some(canonicalize_timestamp(parse_timestamp(Some(ts.as_str()))?))
        } else {
            None
        };
        let confidence = input.confidence.unwrap_or(1.0);

        let dedup_key = compute_fact_dedup_key(&input);

        // 检查去重
        {
            let read_txn = self
                .db
                .begin_read()
                .map_err(|e| Error::Other(e.to_string()))?;
            let idx = read_txn
                .open_table(TABLE_IDX_FACT_DEDUP)
                .map_err(|e| Error::Other(e.to_string()))?;

            if let Some(existing_id) = idx
                .get(dedup_key.as_str())
                .map_err(|e| Error::Other(e.to_string()))?
            {
                let facts = read_txn
                    .open_table(TABLE_FACTS)
                    .map_err(|e| Error::Other(e.to_string()))?;
                if let Some(data) = facts
                    .get(existing_id.value())
                    .map_err(|e| Error::Other(e.to_string()))?
                {
                    let fact: StoredFact = rmp_serde::from_slice(data.value())
                        .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                    return Ok(fact);
                }
            }
        }

        // 创建新 Fact
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let fact = {
            let fact_id = self.next_id(&write_txn, "fact")?;
            let fact = StoredFact {
                fact_id,
                subject_entity_id: input.subject_entity_id,
                predicate_key: input.predicate_key,
                object_entity_id: input.object_entity_id,
                object_value: input.object_value,
                valid_from,
                valid_to,
                confidence,
                source_episode_id: input.source_episode_id,
            };

            let bytes = rmp_serde::to_vec(&fact)
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;

            // 写入主表
            let mut facts = write_txn
                .open_table(TABLE_FACTS)
                .map_err(|e| Error::Other(e.to_string()))?;
            facts
                .insert(fact_id, bytes.as_slice())
                .map_err(|e| Error::Other(e.to_string()))?;

            // 写入索引
            let mut idx_subj = write_txn
                .open_table(TABLE_IDX_FACT_SUBJECT)
                .map_err(|e| Error::Other(e.to_string()))?;
            idx_subj
                .insert((fact.subject_entity_id, fact_id), ())
                .map_err(|e| Error::Other(e.to_string()))?;

            if let Some(obj_id) = fact.object_entity_id {
                let mut idx_obj = write_txn
                    .open_table(TABLE_IDX_FACT_OBJECT)
                    .map_err(|e| Error::Other(e.to_string()))?;
                idx_obj
                    .insert((obj_id, fact_id), ())
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            // 写入去重索引
            let mut idx_dedup = write_txn
                .open_table(TABLE_IDX_FACT_DEDUP)
                .map_err(|e| Error::Other(e.to_string()))?;
            idx_dedup
                .insert(dedup_key.as_str(), fact_id)
                .map_err(|e| Error::Other(e.to_string()))?;

            fact
        };

        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(fact)
    }

    // ========================================================================
    // Link Operations
    // ========================================================================

    pub fn link_episode(
        &self,
        episode_id: u64,
        options: EpisodeLinkOptions,
    ) -> Result<EpisodeLinkRecord> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let record = {
            let link_id = self.next_id(&write_txn, "link")?;
            let record = EpisodeLinkRecord {
                link_id,
                episode_id,
                entity_id: options.entity_id,
                fact_id: options.fact_id,
                role: options.role,
            };

            let bytes = rmp_serde::to_vec(&record)
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;

            let mut links = write_txn
                .open_table(TABLE_LINKS)
                .map_err(|e| Error::Other(e.to_string()))?;
            links
                .insert(link_id, bytes.as_slice())
                .map_err(|e| Error::Other(e.to_string()))?;

            record
        };

        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(record)
    }

    // ========================================================================
    // Query Operations
    // ========================================================================

    /// 时间线查询 - O(k) 其中 k 是匹配的 facts 数量
    pub fn query_timeline(&self, query: &TimelineQuery) -> Result<Vec<StoredFact>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;

        let facts_table = read_txn
            .open_table(TABLE_FACTS)
            .map_err(|e| Error::Other(e.to_string()))?;

        let between = query.between.as_ref().and_then(|(start, end)| {
            let start = OffsetDateTime::parse(start, &Rfc3339).ok()?;
            let end = OffsetDateTime::parse(end, &Rfc3339).ok()?;
            Some((start, end))
        });

        let as_of = query
            .as_of
            .as_ref()
            .and_then(|ts| OffsetDateTime::parse(ts, &Rfc3339).ok());

        let mut results = Vec::new();

        // 根据 role 选择索引
        let fact_ids: Vec<u64> = match query.role {
            Some(TimelineRole::Object) => {
                let idx = read_txn
                    .open_table(TABLE_IDX_FACT_OBJECT)
                    .map_err(|e| Error::Other(e.to_string()))?;
                // Range scan: (entity_id, 0) .. (entity_id, u64::MAX)
                let start = (query.entity_id, 0u64);
                let end = (query.entity_id, u64::MAX);
                idx.range(start..=end)
                    .map_err(|e| Error::Other(e.to_string()))?
                    .map(|item| {
                        let (key_guard, _) = item.unwrap();
                        let (_, fact_id) = key_guard.value();
                        fact_id
                    })
                    .collect()
            }
            _ => {
                let idx = read_txn
                    .open_table(TABLE_IDX_FACT_SUBJECT)
                    .map_err(|e| Error::Other(e.to_string()))?;
                let start = (query.entity_id, 0u64);
                let end = (query.entity_id, u64::MAX);
                idx.range(start..=end)
                    .map_err(|e| Error::Other(e.to_string()))?
                    .map(|item| {
                        let (key_guard, _) = item.unwrap();
                        let (_, fact_id) = key_guard.value();
                        fact_id
                    })
                    .collect()
            }
        };

        // 获取并过滤 facts
        for fact_id in fact_ids {
            if let Some(data) = facts_table
                .get(fact_id)
                .map_err(|e| Error::Other(e.to_string()))?
            {
                let fact: StoredFact = rmp_serde::from_slice(data.value())
                    .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;

                // 过滤 predicate_key
                if let Some(key) = &query.predicate_key
                    && &fact.predicate_key != key
                {
                    continue;
                }

                // 过滤时间范围
                if let Some((start, end)) = between {
                    let from = match OffsetDateTime::parse(&fact.valid_from, &Rfc3339) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                    let to = fact
                        .valid_to
                        .as_ref()
                        .and_then(|ts| OffsetDateTime::parse(ts, &Rfc3339).ok())
                        .unwrap_or(OffsetDateTime::from_unix_timestamp(i64::MAX).unwrap());
                    if to < start || from > end {
                        continue;
                    }
                }

                // 过滤 as_of
                if let Some(as_of_dt) = as_of {
                    let from = match OffsetDateTime::parse(&fact.valid_from, &Rfc3339) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                    let to = fact
                        .valid_to
                        .as_ref()
                        .and_then(|ts| OffsetDateTime::parse(ts, &Rfc3339).ok())
                        .unwrap_or(OffsetDateTime::from_unix_timestamp(i64::MAX).unwrap());
                    if !(from <= as_of_dt && as_of_dt < to) {
                        continue;
                    }
                }

                results.push(fact);
            }
        }

        Ok(results)
    }

    /// 追溯查询 - 查找与 Fact 关联的 Episodes
    pub fn trace_back(&self, fact_id: u64) -> Result<Vec<StoredEpisode>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;

        let links = read_txn
            .open_table(TABLE_LINKS)
            .map_err(|e| Error::Other(e.to_string()))?;
        let episodes_table = read_txn
            .open_table(TABLE_EPISODES)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut episode_ids = HashSet::new();
        for item in links.iter().map_err(|e| Error::Other(e.to_string()))? {
            let (_, data) = item.map_err(|e| Error::Other(e.to_string()))?;
            let link: EpisodeLinkRecord = rmp_serde::from_slice(data.value())
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
            if link.fact_id == Some(fact_id) {
                episode_ids.insert(link.episode_id);
            }
        }

        let mut results = Vec::new();
        for id in episode_ids {
            if let Some(data) = episodes_table
                .get(id)
                .map_err(|e| Error::Other(e.to_string()))?
            {
                let episode: StoredEpisode = rmp_serde::from_slice(data.value())
                    .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
                results.push(episode);
            }
        }

        Ok(results)
    }

    // ========================================================================
    // List Operations (for compatibility)
    // ========================================================================

    pub fn get_episodes(&self) -> Result<Vec<StoredEpisode>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = read_txn
            .open_table(TABLE_EPISODES)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut results = Vec::new();
        for item in table.iter().map_err(|e| Error::Other(e.to_string()))? {
            let (_, data) = item.map_err(|e| Error::Other(e.to_string()))?;
            let episode: StoredEpisode = rmp_serde::from_slice(data.value())
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
            results.push(episode);
        }
        Ok(results)
    }

    pub fn get_entities(&self) -> Result<Vec<StoredEntity>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = read_txn
            .open_table(TABLE_ENTITIES)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut results = Vec::new();
        for item in table.iter().map_err(|e| Error::Other(e.to_string()))? {
            let (_, data) = item.map_err(|e| Error::Other(e.to_string()))?;
            let entity: StoredEntity = rmp_serde::from_slice(data.value())
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
            results.push(entity);
        }
        Ok(results)
    }

    pub fn get_aliases(&self) -> Result<Vec<StoredAlias>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = read_txn
            .open_table(TABLE_ALIASES)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut results = Vec::new();
        for item in table.iter().map_err(|e| Error::Other(e.to_string()))? {
            let (_, data) = item.map_err(|e| Error::Other(e.to_string()))?;
            let alias: StoredAlias = rmp_serde::from_slice(data.value())
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
            results.push(alias);
        }
        Ok(results)
    }

    pub fn get_facts(&self) -> Result<Vec<StoredFact>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = read_txn
            .open_table(TABLE_FACTS)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut results = Vec::new();
        for item in table.iter().map_err(|e| Error::Other(e.to_string()))? {
            let (_, data) = item.map_err(|e| Error::Other(e.to_string()))?;
            let fact: StoredFact = rmp_serde::from_slice(data.value())
                .map_err(|e| Error::Other(format!("msgpack error: {e}")))?;
            results.push(fact);
        }
        Ok(results)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::tempdir;

    fn open_store() -> (tempfile::TempDir, TemporalStoreV2) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("store.redb");
        let db = Arc::new(Database::create(&path).unwrap());
        let store = TemporalStoreV2::open(db).unwrap();
        (dir, store)
    }

    #[test]
    fn test_episode_crud() {
        let (_dir, store) = open_store();

        let episode = store
            .add_episode(EpisodeInput {
                source_type: "conversation".into(),
                payload: Value::String("hello".into()),
                occurred_at: "2025-01-01T00:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();

        assert_eq!(episode.episode_id, 1);
        assert_eq!(episode.source_type, "conversation");

        // 去重测试
        let episode2 = store
            .add_episode(EpisodeInput {
                source_type: "conversation".into(),
                payload: Value::String("hello".into()),
                occurred_at: "2025-01-01T00:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();

        assert_eq!(episode.episode_id, episode2.episode_id);
    }

    #[test]
    fn test_entity_crud() {
        let (_dir, store) = open_store();

        let entity = store
            .ensure_entity(
                "person",
                "Alice",
                EnsureEntityOptions {
                    alias: Some("alice".into()),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(entity.entity_id, 1);
        assert_eq!(entity.kind, "person");
        assert_eq!(entity.canonical_name, "Alice");

        // 去重 + 版本递增测试
        let entity2 = store
            .ensure_entity(
                "person",
                "Alice",
                EnsureEntityOptions {
                    version_increment: true,
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(entity.entity_id, entity2.entity_id);
        assert_eq!(entity2.version, 2);
    }

    #[test]
    fn test_fact_and_timeline() {
        let (_dir, store) = open_store();

        let alice = store
            .ensure_entity("person", "Alice", Default::default())
            .unwrap();
        let bob = store
            .ensure_entity("person", "Bob", Default::default())
            .unwrap();

        let episode = store
            .add_episode(EpisodeInput {
                source_type: "chat".into(),
                payload: Value::String("hi".into()),
                occurred_at: "2025-01-01T12:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();

        let fact = store
            .upsert_fact(FactWriteInput {
                subject_entity_id: alice.entity_id,
                predicate_key: "knows".into(),
                object_entity_id: Some(bob.entity_id),
                object_value: None,
                valid_from: Some("2025-01-01T12:00:00Z".into()),
                valid_to: None,
                confidence: None,
                source_episode_id: episode.episode_id,
            })
            .unwrap();

        assert_eq!(fact.fact_id, 1);

        // 时间线查询
        let timeline = store
            .query_timeline(&TimelineQuery {
                entity_id: alice.entity_id,
                predicate_key: Some("knows".into()),
                role: Some(TimelineRole::Subject),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].fact_id, fact.fact_id);

        // 去重测试
        let fact2 = store
            .upsert_fact(FactWriteInput {
                subject_entity_id: alice.entity_id,
                predicate_key: "knows".into(),
                object_entity_id: Some(bob.entity_id),
                object_value: None,
                valid_from: Some("2025-01-01T12:00:00Z".into()),
                valid_to: None,
                confidence: None,
                source_episode_id: episode.episode_id,
            })
            .unwrap();

        assert_eq!(fact.fact_id, fact2.fact_id);
    }

    #[test]
    fn test_trace_back() {
        let (_dir, store) = open_store();

        let alice = store
            .ensure_entity("person", "Alice", Default::default())
            .unwrap();

        let episode = store
            .add_episode(EpisodeInput {
                source_type: "chat".into(),
                payload: Value::String("hi".into()),
                occurred_at: "2025-01-01T12:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();

        let fact = store
            .upsert_fact(FactWriteInput {
                subject_entity_id: alice.entity_id,
                predicate_key: "said".into(),
                object_entity_id: None,
                object_value: Some(Value::String("hello".into())),
                valid_from: None,
                valid_to: None,
                confidence: None,
                source_episode_id: episode.episode_id,
            })
            .unwrap();

        store
            .link_episode(
                episode.episode_id,
                EpisodeLinkOptions {
                    entity_id: Some(alice.entity_id),
                    fact_id: Some(fact.fact_id),
                    role: "author".into(),
                },
            )
            .unwrap();

        let episodes = store.trace_back(fact.fact_id).unwrap();
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].episode_id, episode.episode_id);
    }

    #[test]
    fn test_persistence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("store.redb");

        // 第一次打开，写入数据
        {
            let db = Arc::new(Database::create(&path).unwrap());
            let store = TemporalStoreV2::open(db).unwrap();

            store
                .ensure_entity("person", "Alice", Default::default())
                .unwrap();
            store
                .add_episode(EpisodeInput {
                    source_type: "test".into(),
                    payload: Value::Null,
                    occurred_at: "2025-01-01T00:00:00Z".into(),
                    trace_hash: None,
                })
                .unwrap();
        }

        // 第二次打开，验证数据
        {
            let db = Arc::new(Database::create(&path).unwrap());
            let store = TemporalStoreV2::open(db).unwrap();

            let entities = store.get_entities().unwrap();
            assert_eq!(entities.len(), 1);
            assert_eq!(entities[0].canonical_name, "Alice");

            let episodes = store.get_episodes().unwrap();
            assert_eq!(episodes.len(), 1);
        }
    }
}
