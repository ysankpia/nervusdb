use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

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
#[serde(tag = "type", content = "data")]
enum TemporalLogEntry {
    Episode(StoredEpisode),
    Entity(StoredEntity),
    Alias(StoredAlias),
    Fact(StoredFact),
    Link(EpisodeLinkRecord),
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

#[derive(Debug)]
pub struct TemporalStore {
    log_path: PathBuf,
    data: TemporalData,
    indices: TemporalIndices,
}

impl TemporalStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json_path = path.as_ref().to_owned();
        let log_path = json_path.with_extension("log");

        // Migration: if json exists but log doesn't, migrate
        if json_path.exists() && !log_path.exists() {
            Self::migrate_json_to_log(&json_path, &log_path)?;
        }

        let mut store = Self {
            log_path,
            data: TemporalData::default(),
            indices: TemporalIndices::default(),
        };

        store.replay_log()?;

        Ok(store)
    }

    fn migrate_json_to_log(json_path: &Path, log_path: &Path) -> Result<()> {
        let raw = fs::read_to_string(json_path)
            .map_err(|err| Error::Other(format!("failed to read legacy json: {err}")))?;
        let data: TemporalData = serde_json::from_str(&raw)
            .map_err(|err| Error::Other(format!("failed to parse legacy json: {err}")))?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .map_err(|err| Error::Other(format!("failed to create log file: {err}")))?;
        let mut writer = std::io::BufWriter::new(file);

        for episode in data.episodes {
            let entry = TemporalLogEntry::Episode(episode);
            serde_json::to_writer(&mut writer, &entry)
                .map_err(|err| Error::Other(format!("failed to write migration entry: {err}")))?;
            writer
                .write_all(b"\n")
                .map_err(|err| Error::Other(format!("failed to write newline: {err}")))?;
        }
        for entity in data.entities {
            let entry = TemporalLogEntry::Entity(entity);
            serde_json::to_writer(&mut writer, &entry)
                .map_err(|err| Error::Other(format!("failed to write migration entry: {err}")))?;
            writer
                .write_all(b"\n")
                .map_err(|err| Error::Other(format!("failed to write newline: {err}")))?;
        }
        for alias in data.aliases {
            let entry = TemporalLogEntry::Alias(alias);
            serde_json::to_writer(&mut writer, &entry)
                .map_err(|err| Error::Other(format!("failed to write migration entry: {err}")))?;
            writer
                .write_all(b"\n")
                .map_err(|err| Error::Other(format!("failed to write newline: {err}")))?;
        }
        for fact in data.facts {
            let entry = TemporalLogEntry::Fact(fact);
            serde_json::to_writer(&mut writer, &entry)
                .map_err(|err| Error::Other(format!("failed to write migration entry: {err}")))?;
            writer
                .write_all(b"\n")
                .map_err(|err| Error::Other(format!("failed to write newline: {err}")))?;
        }
        for link in data.links {
            let entry = TemporalLogEntry::Link(link);
            serde_json::to_writer(&mut writer, &entry)
                .map_err(|err| Error::Other(format!("failed to write migration entry: {err}")))?;
            writer
                .write_all(b"\n")
                .map_err(|err| Error::Other(format!("failed to write newline: {err}")))?;
        }
        writer
            .flush()
            .map_err(|err| Error::Other(format!("failed to flush migration: {err}")))?;

        // Rename old json to .json.bak to indicate migration complete
        let bak_path = json_path.with_extension("json.bak");
        fs::rename(json_path, bak_path)
            .map_err(|err| Error::Other(format!("failed to rename legacy json: {err}")))?;

        Ok(())
    }

    fn replay_log(&mut self) -> Result<()> {
        if !self.log_path.exists() {
            return Ok(());
        }

        let file = fs::File::open(&self.log_path)
            .map_err(|err| Error::Other(format!("failed to open log file: {err}")))?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line =
                line.map_err(|err| Error::Other(format!("failed to read log line: {err}")))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: TemporalLogEntry = serde_json::from_str(&line)
                .map_err(|err| Error::Other(format!("failed to parse log entry: {err}")))?;
            self.apply_entry(entry);
        }
        Ok(())
    }

    fn apply_entry(&mut self, entry: TemporalLogEntry) {
        match entry {
            TemporalLogEntry::Episode(e) => {
                if e.episode_id > self.data.counters.episode {
                    self.data.counters.episode = e.episode_id;
                }
                self.data.episodes.push(e);
            }
            TemporalLogEntry::Entity(e) => {
                if e.entity_id > self.data.counters.entity {
                    self.data.counters.entity = e.entity_id;
                }
                // Handle updates (version increment) by replacing existing
                if let Some(idx) = self
                    .data
                    .entities
                    .iter()
                    .position(|x| x.entity_id == e.entity_id)
                {
                    self.data.entities[idx] = e;
                } else {
                    self.data.entities.push(e);
                }
            }
            TemporalLogEntry::Alias(a) => {
                if a.alias_id > self.data.counters.alias {
                    self.data.counters.alias = a.alias_id;
                }
                self.data.aliases.push(a);
            }
            TemporalLogEntry::Fact(f) => {
                if f.fact_id > self.data.counters.fact {
                    self.data.counters.fact = f.fact_id;
                }
                // Handle updates
                if let Some(idx) = self.data.facts.iter().position(|x| x.fact_id == f.fact_id) {
                    self.data.facts[idx] = f;
                    // Note: Index update for modify is tricky if subject/object changes,
                    // but Fact is append-only logically. We assume subject/object don't change for same ID.
                } else {
                    let idx = self.data.facts.len();
                    self.indices
                        .subject_lookup
                        .entry(f.subject_entity_id)
                        .or_default()
                        .push(idx);
                    if let Some(obj_id) = f.object_entity_id {
                        self.indices
                            .object_lookup
                            .entry(obj_id)
                            .or_default()
                            .push(idx);
                    }
                    self.data.facts.push(f);
                }
            }
            TemporalLogEntry::Link(l) => {
                if l.link_id > self.data.counters.link {
                    self.data.counters.link = l.link_id;
                }
                self.data.links.push(l);
            }
        }
    }

    fn append(&mut self, entry: TemporalLogEntry) -> Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .map_err(|err| Error::Other(format!("failed to open log for appending: {err}")))?;
        let mut writer = std::io::BufWriter::new(file);
        serde_json::to_writer(&mut writer, &entry)
            .map_err(|err| Error::Other(format!("failed to serialize entry: {err}")))?;
        writer
            .write_all(b"\n")
            .map_err(|err| Error::Other(format!("failed to write newline: {err}")))?;
        writer
            .flush()
            .map_err(|err| Error::Other(format!("failed to flush log: {err}")))?;

        self.apply_entry(entry);
        Ok(())
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

    pub fn add_episode(&mut self, input: EpisodeInput) -> Result<StoredEpisode> {
        let occurred_at_dt = OffsetDateTime::parse(&input.occurred_at, &Rfc3339)
            .map_err(|err| Error::Other(format!("invalid occurred_at: {err}")))?;
        let occurred_at = canonicalize_timestamp(occurred_at_dt);
        let trace_hash = self.compute_trace_hash(&input, &occurred_at)?;
        if let Some(existing) = self
            .data
            .episodes
            .iter()
            .find(|episode| episode.trace_hash == trace_hash)
            .cloned()
        {
            return Ok(existing);
        }
        let episode = StoredEpisode {
            episode_id: self.next_episode_id(),
            source_type: input.source_type,
            payload: input.payload,
            occurred_at,
            ingested_at: canonicalize_timestamp(OffsetDateTime::now_utc()),
            trace_hash,
        };
        self.append(TemporalLogEntry::Episode(episode.clone()))?;
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

        if let Some(index) = self
            .data
            .entities
            .iter()
            .position(|entity| entity.fingerprint == fingerprint)
        {
            let mut entity = self.data.entities[index].clone();
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
                self.append(TemporalLogEntry::Entity(entity.clone()))?;
            }

            if let Some(alias_text) = alias {
                self.ensure_alias(entity.entity_id, alias_text, confidence)?;
            }
            return Ok(entity);
        }

        let entity = StoredEntity {
            entity_id: self.next_entity_id(),
            kind: kind.to_string(),
            canonical_name: canonical_name.to_string(),
            fingerprint,
            first_seen: occurred_at.clone(),
            last_seen: occurred_at.clone(),
            version: 1,
        };
        self.append(TemporalLogEntry::Entity(entity.clone()))?;
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
    ) -> Result<()> {
        if self.data.aliases.iter().any(|alias| {
            alias.entity_id == entity_id && alias.alias_text.eq_ignore_ascii_case(&alias_text)
        }) {
            return Ok(());
        }
        let alias = StoredAlias {
            alias_id: self.next_alias_id(),
            entity_id,
            alias_text,
            confidence: confidence.unwrap_or(1.0),
        };
        self.append(TemporalLogEntry::Alias(alias))?;
        Ok(())
    }

    pub fn upsert_fact(&mut self, input: FactWriteInput) -> Result<StoredFact> {
        let valid_from = canonicalize_timestamp(parse_timestamp(input.valid_from.as_deref())?);
        let valid_to = if let Some(value) = input.valid_to {
            Some(canonicalize_timestamp(
                OffsetDateTime::parse(&value, &Rfc3339)
                    .map_err(|err| Error::Other(format!("invalid valid_to: {err}")))?,
            ))
        } else {
            None
        };
        let key = (
            input.subject_entity_id,
            input.predicate_key.clone(),
            input.object_entity_id,
        );
        if let Some(index) = self.data.facts.iter().position(|fact| {
            fact.subject_entity_id == key.0
                && fact.predicate_key == key.1
                && fact.object_entity_id == key.2
                && fact.valid_to.is_none()
        }) {
            let mut fact = self.data.facts[index].clone();
            let mut changed = false;
            if valid_from < fact.valid_from {
                fact.valid_from = valid_from.clone();
                changed = true;
            }
            if let Some(to) = valid_to {
                if fact.valid_to.as_ref().map_or(true, |current| &to < current) {
                    fact.valid_to = Some(to);
                    changed = true;
                }
            }
            if let Some(value) = input.object_value {
                fact.object_value = Some(value);
                changed = true;
            }
            if let Some(conf) = input.confidence {
                if (fact.confidence - conf).abs() > f64::EPSILON {
                    fact.confidence = conf;
                    changed = true;
                }
            }

            if changed {
                self.append(TemporalLogEntry::Fact(fact.clone()))?;
            }
            return Ok(fact);
        }

        let fact = StoredFact {
            fact_id: self.next_fact_id(),
            subject_entity_id: input.subject_entity_id,
            predicate_key: input.predicate_key,
            object_entity_id: input.object_entity_id,
            object_value: input.object_value,
            valid_from,
            valid_to,
            confidence: input.confidence.unwrap_or(1.0),
            source_episode_id: input.source_episode_id,
        };
        self.append(TemporalLogEntry::Fact(fact.clone()))?;
        Ok(fact)
    }

    pub fn link_episode(
        &mut self,
        episode_id: u64,
        options: EpisodeLinkOptions,
    ) -> Result<EpisodeLinkRecord> {
        if options.entity_id.is_none() && options.fact_id.is_none() {
            return Err(Error::Other(
                "episode link must reference an entity or fact".to_string(),
            ));
        }
        if let Some(existing) = self
            .data
            .links
            .iter()
            .find(|link| {
                link.episode_id == episode_id
                    && link.entity_id == options.entity_id
                    && link.fact_id == options.fact_id
                    && link.role.eq_ignore_ascii_case(&options.role)
            })
            .cloned()
        {
            return Ok(existing);
        }
        let record = EpisodeLinkRecord {
            link_id: self.next_link_id(),
            episode_id,
            entity_id: options.entity_id,
            fact_id: options.fact_id,
            role: options.role,
        };
        self.append(TemporalLogEntry::Link(record.clone()))?;
        Ok(record)
    }

    pub fn query_timeline(&self, query: &TimelineQuery) -> Vec<StoredFact> {
        let as_of = query
            .as_of
            .as_ref()
            .and_then(|ts| OffsetDateTime::parse(ts, &Rfc3339).ok());
        let between = query.between.as_ref().and_then(|(start, end)| {
            let start_dt = OffsetDateTime::parse(start, &Rfc3339).ok()?;
            let end_dt = OffsetDateTime::parse(end, &Rfc3339).ok()?;
            Some((start_dt, end_dt))
        });
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
            // Role check is implicit by lookup, but double check for safety if needed (omitted for speed)

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
    use tempfile::tempdir;

    fn store_path(dir: &Path) -> PathBuf {
        dir.join("store.temporal.json")
    }

    #[test]
    fn episode_deduplication() {
        let dir = tempdir().unwrap();
        let path = store_path(dir.path());
        let mut store = TemporalStore::open(&path).unwrap();
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
    fn ensure_entity_alias() {
        let dir = tempdir().unwrap();
        let path = store_path(dir.path());
        let mut store = TemporalStore::open(&path).unwrap();
        let entity = store
            .ensure_entity(
                "agent",
                "alice",
                EnsureEntityOptions {
                    alias: Some("Alice".into()),
                    confidence: Some(0.8),
                    occurred_at: Some("2025-01-01T00:00:00Z".into()),
                    version_increment: false,
                },
            )
            .unwrap();
        let aliases = store.get_aliases();
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].entity_id, entity.entity_id);
    }

    #[test]
    fn upsert_fact_and_query() {
        let dir = tempdir().unwrap();
        let path = store_path(dir.path());
        let mut store = TemporalStore::open(&path).unwrap();
        let alice = store
            .ensure_entity(
                "agent",
                "alice",
                EnsureEntityOptions {
                    alias: Some("Alice".into()),
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
                    alias: Some("Bob".into()),
                    occurred_at: Some("2025-01-01T00:00:00Z".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let episode = store
            .add_episode(EpisodeInput {
                source_type: "conversation".into(),
                payload: Value::String("hello".into()),
                occurred_at: "2025-01-01T00:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();
        let fact = store
            .upsert_fact(FactWriteInput {
                subject_entity_id: alice.entity_id,
                predicate_key: "mentions".into(),
                object_entity_id: Some(bob.entity_id),
                object_value: None,
                valid_from: Some("2025-01-01T00:00:00Z".into()),
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

        let results = store.query_timeline(&TimelineQuery {
            entity_id: alice.entity_id,
            predicate_key: Some("mentions".into()),
            role: Some(TimelineRole::Subject),
            ..Default::default()
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].object_entity_id, Some(bob.entity_id));

        let back = store.trace_back(fact.fact_id);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].episode_id, episode.episode_id);
    }
}
