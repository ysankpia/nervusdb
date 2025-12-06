use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::{Error, Result};
use redb::{ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const TABLE_TEMPORAL_BLOB: TableDefinition<u64, &str> = TableDefinition::new("temporal_blob");

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Counters {
    episode: u64,
    entity: u64,
    alias: u64,
    fact: u64,
    link: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TemporalData {
    counters: Counters,
    episodes: Vec<StoredEpisode>,
    entities: Vec<StoredEntity>,
    aliases: Vec<StoredAlias>,
    facts: Vec<StoredFact>,
    links: Vec<EpisodeLinkRecord>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsureEntityOptions {
    pub alias: Option<String>,
    pub confidence: Option<f64>,
    pub occurred_at: Option<String>,
    pub version_increment: bool,
}

impl Default for EnsureEntityOptions {
    fn default() -> Self {
        Self {
            alias: None,
            confidence: None,
            occurred_at: None,
            version_increment: false,
        }
    }
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

#[derive(Debug, Default, Clone)]
struct TemporalIndices {
    subject_lookup: HashMap<u64, Vec<usize>>,
    object_lookup: HashMap<u64, Vec<usize>>,
}

impl TemporalIndices {
    fn rebuild(&mut self, facts: &[StoredFact]) {
        self.subject_lookup.clear();
        self.object_lookup.clear();
        for (idx, fact) in facts.iter().enumerate() {
            self.subject_lookup
                .entry(fact.subject_entity_id)
                .or_default()
                .push(idx);
            if let Some(object_id) = fact.object_entity_id {
                self.object_lookup.entry(object_id).or_default().push(idx);
            }
        }
    }
}

#[derive(Debug)]
pub struct TemporalStore {
    db: Arc<redb::Database>,
    data: TemporalData,
    indices: TemporalIndices,
    trace_index: HashMap<String, u64>,
    fingerprint_index: HashMap<String, u64>,
}

impl TemporalStore {
    pub fn open(db: Arc<redb::Database>) -> Result<Self> {
        let mut store = Self {
            db,
            data: TemporalData::default(),
            indices: TemporalIndices::default(),
            trace_index: HashMap::new(),
            fingerprint_index: HashMap::new(),
        };
        store.init_tables()?;
        store.load_from_disk()?;
        store.indices.rebuild(&store.data.facts);
        store.trace_index = store
            .data
            .episodes
            .iter()
            .map(|e| (e.trace_hash.clone(), e.episode_id))
            .collect();
        store.fingerprint_index = store
            .data
            .entities
            .iter()
            .map(|e| (e.fingerprint.clone(), e.entity_id))
            .collect();
        Ok(store)
    }

    fn init_tables(&self) -> Result<()> {
        let write_txn = self
            .db
            .as_ref()
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let _ = write_txn
                .open_table(TABLE_TEMPORAL_BLOB)
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    fn load_from_disk(&mut self) -> Result<()> {
        let txn = self
            .db
            .as_ref()
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = txn
            .open_table(TABLE_TEMPORAL_BLOB)
            .map_err(|e| Error::Other(e.to_string()))?;
        if let Some(val) = table.get(1).map_err(|e| Error::Other(e.to_string()))? {
            self.data = serde_json::from_str(val.value())
                .map_err(|err| Error::Other(format!("failed to parse temporal blob: {err}")))?;
        }
        Ok(())
    }

    fn persist(&self) -> Result<()> {
        let blob = serde_json::to_string(&self.data)
            .map_err(|err| Error::Other(format!("failed to serialize temporal state: {err}")))?;
        let write_txn = self
            .db
            .as_ref()
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut table = write_txn
                .open_table(TABLE_TEMPORAL_BLOB)
                .map_err(|e| Error::Other(e.to_string()))?;
            table
                .insert(1, blob.as_str())
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    fn compute_trace_hash(&self, input: &EpisodeInput, occurred_at: &str) -> Result<String> {
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

    fn next_episode_id(&mut self) -> u64 {
        self.data.counters.episode += 1;
        self.data.counters.episode
    }

    fn next_entity_id(&mut self) -> u64 {
        self.data.counters.entity += 1;
        self.data.counters.entity
    }

    fn next_alias_id(&mut self) -> u64 {
        self.data.counters.alias += 1;
        self.data.counters.alias
    }

    fn next_fact_id(&mut self) -> u64 {
        self.data.counters.fact += 1;
        self.data.counters.fact
    }

    fn next_link_id(&mut self) -> u64 {
        self.data.counters.link += 1;
        self.data.counters.link
    }

    pub fn add_episode(&mut self, input: EpisodeInput) -> Result<StoredEpisode> {
        let occurred_at_dt = OffsetDateTime::parse(&input.occurred_at, &Rfc3339)
            .map_err(|err| Error::Other(format!("invalid occurred_at: {err}")))?;
        let occurred_at = canonicalize_timestamp(occurred_at_dt);
        let trace_hash = self.compute_trace_hash(&input, &occurred_at)?;
        if let Some(existing_id) = self.trace_index.get(&trace_hash) {
            if let Some(existing) = self
                .data
                .episodes
                .iter()
                .find(|e| e.episode_id == *existing_id)
            {
                return Ok(existing.clone());
            }
        }

        let episode = StoredEpisode {
            episode_id: self.next_episode_id(),
            source_type: input.source_type,
            payload: input.payload,
            occurred_at,
            ingested_at: canonicalize_timestamp(OffsetDateTime::now_utc()),
            trace_hash: trace_hash.clone(),
        };
        self.data.episodes.push(episode.clone());
        self.trace_index.insert(trace_hash, episode.episode_id);
        self.persist()?;
        Ok(episode)
    }

    pub fn ensure_entity(
        &mut self,
        kind: &str,
        canonical_name: &str,
        options: EnsureEntityOptions,
    ) -> Result<StoredEntity> {
        let EnsureEntityOptions {
            alias,
            confidence,
            occurred_at,
            version_increment,
        } = options;

        let fingerprint = format!("{}:{}", kind, canonical_name.to_ascii_lowercase());
        let occurred_dt = parse_timestamp(occurred_at.as_deref())?;
        let occurred_at = canonicalize_timestamp(occurred_dt);

        if let Some(id) = self.fingerprint_index.get(&fingerprint) {
            if let Some(idx) = self
                .data
                .entities
                .iter()
                .position(|entity| entity.entity_id == *id)
            {
                let mut entity = self.data.entities[idx].clone();
                let mut changed = false;
                if version_increment {
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
                    self.data.entities[idx] = entity.clone();
                    self.persist()?;
                }
                if let Some(alias_text) = alias {
                    self.ensure_alias(entity.entity_id, alias_text, confidence)?;
                }
                return Ok(entity);
            }
        }

        let entity = StoredEntity {
            entity_id: self.next_entity_id(),
            kind: kind.to_string(),
            canonical_name: canonical_name.to_string(),
            fingerprint: fingerprint.clone(),
            first_seen: occurred_at.clone(),
            last_seen: occurred_at,
            version: 1,
        };
        self.data.entities.push(entity.clone());
        self.fingerprint_index.insert(fingerprint, entity.entity_id);
        self.persist()?;

        if let Some(alias_text) = alias {
            self.ensure_alias(entity.entity_id, alias_text, confidence)?;
        }

        Ok(entity)
    }

    fn ensure_alias(
        &mut self,
        entity_id: u64,
        alias_text: String,
        confidence: Option<f64>,
    ) -> Result<StoredAlias> {
        if let Some(existing) = self
            .data
            .aliases
            .iter()
            .find(|a| a.entity_id == entity_id && a.alias_text == alias_text)
        {
            return Ok(existing.clone());
        }

        let alias = StoredAlias {
            alias_id: self.next_alias_id(),
            entity_id,
            alias_text,
            confidence: confidence.unwrap_or(1.0),
        };
        self.data.aliases.push(alias.clone());
        self.persist()?;
        Ok(alias)
    }

    pub fn upsert_fact(&mut self, input: FactWriteInput) -> Result<StoredFact> {
        let FactWriteInput {
            subject_entity_id,
            predicate_key,
            object_entity_id,
            object_value,
            valid_from,
            valid_to,
            confidence,
            source_episode_id,
        } = input;

        let valid_from_ts = parse_timestamp(valid_from.as_deref())?;
        let valid_from = canonicalize_timestamp(valid_from_ts);
        let valid_to = if let Some(ts) = valid_to {
            Some(canonicalize_timestamp(parse_timestamp(Some(ts.as_str()))?))
        } else {
            None
        };
        let confidence = confidence.unwrap_or(1.0);

        if let Some(existing) = self
            .data
            .facts
            .iter()
            .find(|fact| {
                fact.subject_entity_id == subject_entity_id
                    && fact.predicate_key == predicate_key
                    && fact.object_entity_id == object_entity_id
                    && fact.object_value == object_value
            })
            .cloned()
        {
            return Ok(existing);
        }

        let fact = StoredFact {
            fact_id: self.next_fact_id(),
            subject_entity_id,
            predicate_key,
            object_entity_id,
            object_value,
            valid_from,
            valid_to,
            confidence,
            source_episode_id,
        };
        self.data.facts.push(fact.clone());
        self.indices.rebuild(&self.data.facts);
        self.persist()?;
        Ok(fact)
    }

    pub fn link_episode(
        &mut self,
        episode_id: u64,
        options: EpisodeLinkOptions,
    ) -> Result<EpisodeLinkRecord> {
        let record = EpisodeLinkRecord {
            link_id: self.next_link_id(),
            episode_id,
            entity_id: options.entity_id,
            fact_id: options.fact_id,
            role: options.role,
        };
        self.data.links.push(record.clone());
        self.persist()?;
        Ok(record)
    }

    pub fn query_timeline(&self, query: &TimelineQuery) -> Vec<StoredFact> {
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

        let candidate_indices: Box<dyn Iterator<Item = &usize>> = match query.role {
            Some(TimelineRole::Object) => {
                if let Some(indices) = self.indices.object_lookup.get(&query.entity_id) {
                    Box::new(indices.iter())
                } else {
                    Box::new(std::iter::empty())
                }
            }
            _ => {
                if let Some(indices) = self.indices.subject_lookup.get(&query.entity_id) {
                    Box::new(indices.iter())
                } else {
                    Box::new(std::iter::empty())
                }
            }
        };

        for &idx in candidate_indices {
            let fact = &self.data.facts[idx];
            if query
                .predicate_key
                .as_ref()
                .map_or(false, |key| &fact.predicate_key != key)
            {
                continue;
            }

            if let Some((start, end)) = between {
                let from = OffsetDateTime::parse(&fact.valid_from, &Rfc3339);
                let to = fact
                    .valid_to
                    .as_ref()
                    .and_then(|ts| OffsetDateTime::parse(ts, &Rfc3339).ok())
                    .unwrap_or(OffsetDateTime::from_unix_timestamp(i64::MAX).unwrap());
                if from.is_err() {
                    continue;
                }
                let from = from.unwrap();
                if to < start || from > end {
                    continue;
                }
            }
            if let Some(as_of_dt) = as_of {
                let from = OffsetDateTime::parse(&fact.valid_from, &Rfc3339);
                if from.is_err() {
                    continue;
                }
                let from = from.unwrap();
                let to = fact
                    .valid_to
                    .as_ref()
                    .and_then(|ts| OffsetDateTime::parse(ts, &Rfc3339).ok())
                    .unwrap_or(OffsetDateTime::from_unix_timestamp(i64::MAX).unwrap());
                if !(from <= as_of_dt && as_of_dt < to) {
                    continue;
                }
            }
            results.push(fact.clone());
        }
        results
    }

    pub fn trace_back(&self, fact_id: u64) -> Vec<StoredEpisode> {
        let mut episode_ids = HashSet::new();
        for link in &self.data.links {
            if link.fact_id == Some(fact_id) {
                episode_ids.insert(link.episode_id);
            }
        }
        self.data
            .episodes
            .iter()
            .filter(|episode| episode_ids.contains(&episode.episode_id))
            .cloned()
            .collect()
    }

    pub fn get_entities(&self) -> &[StoredEntity] {
        &self.data.entities
    }

    pub fn get_aliases(&self) -> &[StoredAlias] {
        &self.data.aliases
    }

    pub fn get_facts(&self) -> &[StoredFact] {
        &self.data.facts
    }

    pub fn get_episodes(&self) -> &[StoredEpisode] {
        &self.data.episodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::tempdir;

    fn open_store() -> (tempfile::TempDir, TemporalStore) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("store.redb");
        let db = Arc::new(Database::create(&path).unwrap());
        let store = TemporalStore::open(db).unwrap();
        (dir, store)
    }

    #[test]
    fn episode_deduplication() {
        let (_dir, mut store) = open_store();
        let episode1 = store
            .add_episode(EpisodeInput {
                source_type: "conversation".into(),
                payload: Value::String("hello world".into()),
                occurred_at: "2025-01-01T00:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();
        let episode2 = store
            .add_episode(EpisodeInput {
                source_type: "conversation".into(),
                payload: Value::String("hello world".into()),
                occurred_at: "2025-01-01T00:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();
        assert_eq!(episode1.episode_id, episode2.episode_id);
    }

    #[test]
    fn ensure_entity_dedup_and_alias() {
        let (_dir, mut store) = open_store();
        let entity = store
            .ensure_entity(
                "agent",
                "alice",
                EnsureEntityOptions {
                    alias: Some("Alice".into()),
                    ..Default::default()
                },
            )
            .unwrap();

        let again = store
            .ensure_entity(
                "agent",
                "alice",
                EnsureEntityOptions {
                    version_increment: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(entity.entity_id, again.entity_id);
        assert!(
            store
                .get_aliases()
                .iter()
                .any(|a| a.entity_id == entity.entity_id)
        );
    }

    #[test]
    fn fact_and_timeline() {
        let (_dir, mut store) = open_store();
        let alice = store
            .ensure_entity(
                "agent",
                "alice",
                EnsureEntityOptions {
                    occurred_at: Some("2025-01-01T00:00:00Z".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let bob = store
            .ensure_entity(
                "agent",
                "bob",
                EnsureEntityOptions {
                    occurred_at: Some("2025-01-01T00:00:00Z".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let episode = store
            .add_episode(EpisodeInput {
                source_type: "conversation".into(),
                payload: Value::String("hi".into()),
                occurred_at: "2025-01-01T00:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();
        let fact = store
            .upsert_fact(FactWriteInput {
                subject_entity_id: alice.entity_id,
                predicate_key: "knows".into(),
                object_entity_id: Some(bob.entity_id),
                object_value: None,
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

        let timeline = store.query_timeline(&TimelineQuery {
            entity_id: alice.entity_id,
            predicate_key: Some("knows".into()),
            role: Some(TimelineRole::Subject),
            ..Default::default()
        });
        assert_eq!(timeline.len(), 1);
        let episodes = store.trace_back(fact.fact_id);
        assert_eq!(episodes.len(), 1);
    }
}
