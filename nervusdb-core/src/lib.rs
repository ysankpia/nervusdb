//! NervusDB core Rust library providing the low level storage primitives.

mod dictionary;
mod error;
pub mod parser;
pub mod storage;
#[cfg(not(target_arch = "wasm32"))]
pub mod temporal;
pub mod triple;
#[cfg(not(target_arch = "wasm32"))]
pub mod wal;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use storage::{Hexastore, open_store};

pub use dictionary::{Dictionary, StringId};
pub use error::{Error, Result};
#[cfg(not(target_arch = "wasm32"))]
pub use temporal::{
    EnsureEntityOptions, EpisodeInput, EpisodeLinkOptions, EpisodeLinkRecord, FactWriteInput,
    StoredEntity, StoredEpisode, StoredFact, TemporalStore, TimelineQuery, TimelineRole,
};
pub use triple::{Fact, Triple};
#[cfg(not(target_arch = "wasm32"))]
pub use wal::{WalEntry, WalRecordType, WriteAheadLog};

/// Database configuration used when opening an instance.
#[derive(Debug, Clone)]
pub struct Options {
    data_path: PathBuf,
}

impl Options {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            data_path: path.as_ref().to_owned(),
        }
    }
}

use std::io::Write; // For dict persistence

pub struct Database {
    dictionary: Dictionary,
    store: Box<dyn Hexastore + Send>,
    #[cfg(not(target_arch = "wasm32"))]
    temporal: TemporalStore,
    #[cfg(not(target_arch = "wasm32"))]
    wal: WriteAheadLog,
    #[cfg(not(target_arch = "wasm32"))]
    dict_log: Option<std::fs::File>,
    cursors: HashMap<u64, QueryCursor>,
    next_cursor_id: u64,
}

struct QueryCursor {
    iter: crate::storage::HexastoreIter,
    finished: bool,
}

impl QueryCursor {
    fn new(iter: crate::storage::HexastoreIter) -> Self {
        Self {
            iter,
            finished: false,
        }
    }

    fn next_batch(&mut self, batch_size: usize) -> (Vec<Triple>, bool) {
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size.max(1) {
            match self.iter.next() {
                Some(triple) => batch.push(triple),
                None => {
                    self.finished = true;
                    break;
                }
            }
        }
        let done = self.finished || batch.is_empty();
        (batch, done)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct QueryCriteria {
    pub subject_id: Option<StringId>,
    pub predicate_id: Option<StringId>,
    pub object_id: Option<StringId>,
}

impl Database {
    /// Opens a database at the specified path.
    ///
    /// The path should be a base path or directory.
    /// We will create:
    /// - path.redb (Graph storage)
    /// - path.temporal.log (Temporal storage)
    /// - path.wal (Write-Ahead Log)
    pub fn open(options: Options) -> Result<Self> {
        let path = options.data_path;
        // Ensure parent dir exists
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Other(e.to_string()))?;
        }

        let _redb_path = path.with_extension("redb");
        #[cfg(not(target_arch = "wasm32"))]
        let temporal_path = path.with_extension("temporal"); // TemporalStore::open expects a path and appends .log.
        // Let's pass the base path with .temporal extension so it creates .temporal.log
        #[cfg(not(target_arch = "wasm32"))]
        let wal_path = path.with_extension("wal");

        let store = open_store(&_redb_path)?;

        #[cfg(not(target_arch = "wasm32"))]
        let temporal = TemporalStore::open(&temporal_path)?;

        #[cfg(not(target_arch = "wasm32"))]
        let wal = WriteAheadLog::open(&wal_path)?;

        // Note: We are NOT loading the hexastore from WAL anymore because redb is persistent.
        // The WAL might still be useful for the Dictionary or TemporalStore if they were not persistent,
        // but TemporalStore IS persistent (AOL).
        // Dictionary is NOT persistent yet! We need to persist Dictionary.
        // Wait, Dictionary is in-memory. If we restart, Dictionary is empty.
        // We need to rebuild Dictionary or persist it.
        //
        // CRITICAL MISSING PIECE: Dictionary Persistence.
        // Phase 1 plan didn't explicitly mention Dictionary persistence, but "Disk-Based" implies full persistence.
        // If Dictionary is lost, the u64 IDs in redb are meaningless.
        //
        // For now, I will assume the WAL replays the dictionary?
        // The WAL stores `WalEntry::Insert(s, p, o)`.
        // Replaying WAL would re-insert strings into Dictionary.
        // BUT `redb` already has the triples. Re-inserting into `redb` is idempotent (we check existence).
        // So replaying WAL is actually the correct way to restore Dictionary state for now!
        //
        // So:
        // 1. Open redb (persistent triples).
        // 2. Open WAL.
        // 3. Replay WAL -> updates Dictionary (in-memory) AND tries to insert into redb (idempotent).
        //
        // This works, but it means startup time is O(Total Writes) to rebuild Dictionary.
        // Ideally Dictionary should also be in redb.
        // But for Phase 1, let's stick to the plan: "DiskHexastore".
        // I will keep WAL replay to restore Dictionary.

        let mut db = Self {
            dictionary: Dictionary::new(),
            store,
            #[cfg(not(target_arch = "wasm32"))]
            temporal,
            #[cfg(not(target_arch = "wasm32"))]
            wal,
            #[cfg(not(target_arch = "wasm32"))]
            dict_log: None,
            cursors: HashMap::new(),
            next_cursor_id: 1,
        };

        // Replay WAL to restore dictionary
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dict_path = path.with_extension("dict");
            if dict_path.exists() {
                let file =
                    std::fs::File::open(&dict_path).map_err(|e| Error::Other(e.to_string()))?;
                let reader = std::io::BufReader::new(file);
                use std::io::BufRead;
                for line in reader.lines() {
                    let line = line.map_err(|e| Error::Other(e.to_string()))?;
                    if !line.is_empty() {
                        db.dictionary.get_or_insert(&line);
                    }
                }
            }

            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&dict_path)
                .map_err(|e| Error::Other(e.to_string()))?;
            db.dict_log = Some(file);
        }

        Ok(db)
    }

    pub fn hydrate(
        &mut self,
        dictionary_values: Vec<String>,
        triples: Vec<(StringId, StringId, StringId)>,
    ) -> Result<()> {
        self.dictionary = Dictionary::from_vec(dictionary_values);
        // Note: We cannot easily clear DiskHexastore here without dropping the database.
        // For now, we assume hydrate is called on an empty database or appends to it.
        // Ideally, hydrate should be used only for initialization.

        for (subject_id, predicate_id, object_id) in triples {
            let triple = Triple::new(subject_id, predicate_id, object_id);
            self.store.insert(&triple)?;
        }

        self.reset_cursors();

        Ok(())
    }

    pub fn add_fact(&mut self, fact: Fact<'_>) -> Result<Triple> {
        // Check if we need to persist new dictionary entries
        #[cfg(not(target_arch = "wasm32"))]
        let start_len = self.dictionary.len();

        let subject = self.dictionary.get_or_insert(fact.subject);
        let predicate = self.dictionary.get_or_insert(fact.predicate);
        let object = self.dictionary.get_or_insert(fact.object);

        #[cfg(not(target_arch = "wasm32"))]
        if self.dictionary.len() > start_len {
            if let Some(ref mut file) = self.dict_log {
                if (subject as usize) >= start_len {
                    writeln!(file, "{}", fact.subject).map_err(|e| Error::Other(e.to_string()))?;
                }
                if (predicate as usize) >= start_len {
                    writeln!(file, "{}", fact.predicate)
                        .map_err(|e| Error::Other(e.to_string()))?;
                }
                if (object as usize) >= start_len {
                    writeln!(file, "{}", fact.object).map_err(|e| Error::Other(e.to_string()))?;
                }
            }
        }

        let triple = Triple::new(subject, predicate, object);

        self.store.insert(&triple)?;
        #[cfg(not(target_arch = "wasm32"))]
        self.wal.append(WalEntry {
            record_type: WalRecordType::AddTriple,
            triple,
        });
        Ok(triple)
    }

    pub fn all_triples(&self) -> Vec<Triple> {
        self.store.iter().collect()
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    pub fn query(&self, criteria: QueryCriteria) -> crate::storage::HexastoreIter {
        self.store.query(
            criteria.subject_id,
            criteria.predicate_id,
            criteria.object_id,
        )
    }

    pub fn open_cursor(&mut self, criteria: QueryCriteria) -> Result<u64> {
        let iter = self.query(criteria);
        let cursor_id = self.next_cursor_id;
        self.next_cursor_id = self.next_cursor_id.wrapping_add(1).max(1);
        self.cursors.insert(cursor_id, QueryCursor::new(iter));
        Ok(cursor_id)
    }

    pub fn cursor_next(
        &mut self,
        cursor_id: u64,
        batch_size: usize,
    ) -> Result<(Vec<Triple>, bool)> {
        let cursor = self
            .cursors
            .get_mut(&cursor_id)
            .ok_or(Error::InvalidCursor(cursor_id))?;
        let (batch, done) = cursor.next_batch(batch_size.max(1));
        if done {
            self.cursors.remove(&cursor_id);
        }
        Ok((batch, done))
    }

    pub fn close_cursor(&mut self, cursor_id: u64) -> Result<()> {
        self.cursors
            .remove(&cursor_id)
            .ok_or(Error::InvalidCursor(cursor_id))?;
        Ok(())
    }

    fn reset_cursors(&mut self) {
        self.cursors.clear();
        self.next_cursor_id = 1;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn temporal_store(&self) -> Option<&TemporalStore> {
        return Some(&self.temporal);
    }
    #[cfg(target_arch = "wasm32")]
    pub fn temporal_store(&self) -> Option<&()> {
        return None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn temporal_store_mut(&mut self) -> Option<&mut TemporalStore> {
        return Some(&mut self.temporal);
    }
    #[cfg(target_arch = "wasm32")]
    pub fn temporal_store_mut(&mut self) -> Option<&mut ()> {
        return None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn timeline_query(&self, query: TimelineQuery) -> Vec<StoredFact> {
        return self.temporal.query_timeline(&query);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn timeline_trace(&self, fact_id: u64) -> Vec<StoredEpisode> {
        return self.temporal.trace_back(fact_id);
    }

    pub fn execute_query(&self, query: &str) -> Result<Vec<Triple>> {
        parser::execute(self, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_and_insert() {
        let tmp = tempdir().unwrap();
        let mut db = Database::open(Options::new(tmp.path())).unwrap();
        let triple = db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();
        assert_eq!(db.all_triples(), vec![triple]);
        assert_eq!(
            db.dictionary().lookup_value(triple.subject_id).unwrap(),
            "alice"
        );

        let results: Vec<_> = db
            .query(QueryCriteria {
                subject_id: Some(triple.subject_id),
                predicate_id: None,
                object_id: None,
            })
            .collect();
        assert_eq!(results, vec![triple]);

        let cursor_id = db
            .open_cursor(QueryCriteria {
                subject_id: Some(triple.subject_id),
                predicate_id: None,
                object_id: None,
            })
            .unwrap();
        let (batch, done) = db.cursor_next(cursor_id, 10).unwrap();
        assert!(done);
        assert_eq!(batch, vec![triple]);
    }

    #[test]
    fn timeline_query_via_database() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("timeline-db");
        let mut db = Database::open(Options::new(&path)).unwrap();

        {
            let store = db
                .temporal_store_mut()
                .expect("temporal store not available");
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
                    payload: serde_json::json!({ "text": "hello" }),
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
        }

        let alice_id = db
            .temporal_store()
            .expect("temporal store not available")
            .get_entities()[0]
            .entity_id;
        let timeline = db.timeline_query(TimelineQuery {
            entity_id: alice_id,
            predicate_key: Some("mentions".into()),
            role: Some(TimelineRole::Subject),
            ..Default::default()
        });
        assert_eq!(timeline.len(), 1);

        let episodes = db.timeline_trace(timeline[0].fact_id);
        assert_eq!(episodes.len(), 1);
    }
}
