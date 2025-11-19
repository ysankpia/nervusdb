use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::Mutex;

use napi::Result as NapiResult;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use nervusdb_core::{
    Database, EnsureEntityOptions as CoreEnsureEntityOptions, EpisodeInput as CoreEpisodeInput,
    EpisodeLinkOptions as CoreEpisodeLinkOptions, EpisodeLinkRecord, Fact,
    FactWriteInput as CoreFactWriteInput, Options, QueryCriteria, StoredEntity, StoredEpisode,
    StoredFact, StringId, TimelineQuery, TimelineRole, Triple,
};
use serde_json::Value;

fn map_error(err: nervusdb_core::Error) -> napi::Error {
    napi::Error::new(Status::GenericFailure, format!("{err}"))
}

#[napi(object)]
pub struct OpenOptions {
    pub data_path: String,
}

#[napi]
pub struct DatabaseHandle {
    inner: Mutex<Option<Database>>,
}

#[napi(object)]
pub struct TripleOutput {
    pub subject_id: u32,
    pub predicate_id: u32,
    pub object_id: u32,
}

#[napi(object)]
pub struct TripleInput {
    pub subject_id: u32,
    pub predicate_id: u32,
    pub object_id: u32,
}

#[napi(object)]
#[derive(Default)]
pub struct QueryCriteriaInput {
    pub subject_id: Option<u32>,
    pub predicate_id: Option<u32>,
    pub object_id: Option<u32>,
}

#[napi(object)]
pub struct CursorId {
    pub id: u32,
}

#[napi(object)]
pub struct CursorBatch {
    pub triples: Vec<TripleOutput>,
    pub done: bool,
}

#[napi(object)]
pub struct TimelineQueryInput {
    pub entity_id: String,
    pub predicate_key: Option<String>,
    pub role: Option<String>,
    pub as_of: Option<String>,
    pub between_start: Option<String>,
    pub between_end: Option<String>,
}

#[napi(object)]
pub struct TimelineFactOutput {
    pub fact_id: String,
    pub subject_entity_id: String,
    pub predicate_key: String,
    pub object_entity_id: Option<String>,
    pub object_value: Option<String>,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub source_episode_id: String,
}

#[napi(object)]
pub struct TimelineEpisodeOutput {
    pub episode_id: String,
    pub source_type: String,
    pub payload: String,
    pub occurred_at: String,
    pub ingested_at: String,
    pub trace_hash: String,
}

#[napi(object)]
pub struct TemporalEpisodeInput {
    pub source_type: String,
    pub payload_json: String,
    pub occurred_at: String,
    pub trace_hash: Option<String>,
}

#[napi(object)]
pub struct TemporalEnsureEntityInput {
    pub kind: String,
    pub canonical_name: String,
    pub alias: Option<String>,
    pub confidence: Option<f64>,
    pub occurred_at: Option<String>,
    pub version_increment: Option<bool>,
}

#[napi(object)]
pub struct TemporalEntityOutput {
    pub entity_id: String,
    pub kind: String,
    pub canonical_name: String,
    pub fingerprint: String,
    pub first_seen: String,
    pub last_seen: String,
    pub version: String,
}

#[napi(object)]
pub struct TemporalFactWriteInput {
    pub subject_entity_id: String,
    pub predicate_key: String,
    pub object_entity_id: Option<String>,
    pub object_value_json: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub confidence: Option<f64>,
    pub source_episode_id: String,
}

#[napi(object)]
pub struct TemporalLinkInput {
    pub episode_id: String,
    pub entity_id: Option<String>,
    pub fact_id: Option<String>,
    pub role: String,
}

#[napi(object)]
pub struct TemporalLinkOutput {
    pub link_id: String,
    pub episode_id: String,
    pub entity_id: Option<String>,
    pub fact_id: Option<String>,
    pub role: String,
}

fn convert_string_id(value: Option<u32>) -> Option<StringId> {
    value.map(|id| id as StringId)
}

fn triple_to_output(triple: Triple) -> NapiResult<TripleOutput> {
    Ok(TripleOutput {
        subject_id: u32::try_from(triple.subject_id)
            .map_err(|_| napi::Error::new(Status::GenericFailure, "subject id overflow"))?,
        predicate_id: u32::try_from(triple.predicate_id)
            .map_err(|_| napi::Error::new(Status::GenericFailure, "predicate id overflow"))?,
        object_id: u32::try_from(triple.object_id)
            .map_err(|_| napi::Error::new(Status::GenericFailure, "object id overflow"))?,
    })
}

fn fact_to_output(fact: StoredFact) -> TimelineFactOutput {
    TimelineFactOutput {
        fact_id: fact.fact_id.to_string(),
        subject_entity_id: fact.subject_entity_id.to_string(),
        predicate_key: fact.predicate_key,
        object_entity_id: fact.object_entity_id.map(|id| id.to_string()),
        object_value: fact
            .object_value
            .and_then(|value| serde_json::to_string(&value).ok()),
        valid_from: fact.valid_from,
        valid_to: fact.valid_to,
        confidence: fact.confidence,
        source_episode_id: fact.source_episode_id.to_string(),
    }
}

fn parse_timeline_role(value: Option<String>) -> NapiResult<Option<TimelineRole>> {
    match value.map(|s| s.to_ascii_lowercase()) {
        None => Ok(None),
        Some(ref role) if role == "subject" => Ok(Some(TimelineRole::Subject)),
        Some(ref role) if role == "object" => Ok(Some(TimelineRole::Object)),
        Some(role) => Err(napi::Error::new(
            Status::GenericFailure,
            format!("invalid timeline role: {role}"),
        )),
    }
}

fn parse_id(value: &str, field: &str) -> NapiResult<u64> {
    value.parse::<u64>().map_err(|err| {
        napi::Error::new(
            Status::GenericFailure,
            format!("invalid {field} id '{value}': {err}"),
        )
    })
}

fn episode_to_output(episode: StoredEpisode) -> TimelineEpisodeOutput {
    TimelineEpisodeOutput {
        episode_id: episode.episode_id.to_string(),
        source_type: episode.source_type,
        payload: serde_json::to_string(&episode.payload).unwrap_or_else(|_| "{}".into()),
        occurred_at: episode.occurred_at,
        ingested_at: episode.ingested_at,
        trace_hash: episode.trace_hash,
    }
}

fn entity_to_output(entity: StoredEntity) -> TemporalEntityOutput {
    TemporalEntityOutput {
        entity_id: entity.entity_id.to_string(),
        kind: entity.kind,
        canonical_name: entity.canonical_name,
        fingerprint: entity.fingerprint,
        first_seen: entity.first_seen,
        last_seen: entity.last_seen,
        version: entity.version.to_string(),
    }
}

fn link_to_output(record: EpisodeLinkRecord) -> TemporalLinkOutput {
    TemporalLinkOutput {
        link_id: record.link_id.to_string(),
        episode_id: record.episode_id.to_string(),
        entity_id: record.entity_id.map(|id| id.to_string()),
        fact_id: record.fact_id.map(|id| id.to_string()),
        role: record.role,
    }
}

fn parse_optional_id(value: Option<String>, field: &str) -> NapiResult<Option<u64>> {
    match value {
        Some(raw) => parse_id(&raw, field).map(Some),
        None => Ok(None),
    }
}

fn parse_json_value(raw: Option<String>, field: &str) -> NapiResult<Option<Value>> {
    match raw {
        Some(value) => serde_json::from_str(&value).map(Some).map_err(|err| {
            napi::Error::new(
                Status::GenericFailure,
                format!("invalid JSON in {field}: {err}"),
            )
        }),
        None => Ok(None),
    }
}

fn parse_payload_json(raw: &str) -> NapiResult<Value> {
    serde_json::from_str(raw).map_err(|err| {
        napi::Error::new(
            Status::GenericFailure,
            format!("invalid episode payload JSON: {err}"),
        )
    })
}

#[napi]
impl DatabaseHandle {
    #[napi(js_name = "temporalAddEpisode")]
    pub fn temporal_add_episode(
        &self,
        input: TemporalEpisodeInput,
    ) -> NapiResult<TimelineEpisodeOutput> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let payload = parse_payload_json(&input.payload_json)?;
        let episode = db
            .temporal_store_mut()
            .expect("temporal store not available")
            .add_episode(CoreEpisodeInput {
                source_type: input.source_type,
                payload,
                occurred_at: input.occurred_at,
                trace_hash: input.trace_hash,
            })
            .map_err(map_error)?;
        Ok(episode_to_output(episode))
    }

    #[napi(js_name = "temporalEnsureEntity")]
    pub fn temporal_ensure_entity(
        &self,
        input: TemporalEnsureEntityInput,
    ) -> NapiResult<TemporalEntityOutput> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let options = CoreEnsureEntityOptions {
            alias: input.alias,
            confidence: input.confidence,
            occurred_at: input.occurred_at,
            version_increment: input.version_increment.unwrap_or(false),
        };

        let entity = db
            .temporal_store_mut()
            .expect("temporal store not available")
            .ensure_entity(&input.kind, &input.canonical_name, options)
            .map_err(map_error)?;
        Ok(entity_to_output(entity))
    }

    #[napi(js_name = "temporalUpsertFact")]
    pub fn temporal_upsert_fact(
        &self,
        input: TemporalFactWriteInput,
    ) -> NapiResult<TimelineFactOutput> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let subject_entity_id = parse_id(&input.subject_entity_id, "subject entity")?;
        let object_entity_id = parse_optional_id(input.object_entity_id, "object entity")?;
        let source_episode_id = parse_id(&input.source_episode_id, "source episode")?;
        let object_value = parse_json_value(input.object_value_json, "fact.object_value_json")?;

        let fact = db
            .temporal_store_mut()
            .expect("temporal store not available")
            .upsert_fact(CoreFactWriteInput {
                subject_entity_id,
                predicate_key: input.predicate_key,
                object_entity_id,
                object_value,
                valid_from: input.valid_from,
                valid_to: input.valid_to,
                confidence: input.confidence,
                source_episode_id,
            })
            .map_err(map_error)?;
        Ok(fact_to_output(fact))
    }

    #[napi(js_name = "temporalLinkEpisode")]
    pub fn temporal_link_episode(
        &self,
        input: TemporalLinkInput,
    ) -> NapiResult<TemporalLinkOutput> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let episode_id = parse_id(&input.episode_id, "episode")?;
        let entity_id = parse_optional_id(input.entity_id, "entity")?;
        let fact_id = parse_optional_id(input.fact_id, "fact")?;

        let record = db
            .temporal_store_mut()
            .expect("temporal store not available")
            .link_episode(
                episode_id,
                CoreEpisodeLinkOptions {
                    entity_id,
                    fact_id,
                    role: input.role,
                },
            )
            .map_err(map_error)?;
        Ok(link_to_output(record))
    }

    #[napi(js_name = "temporalListEntities")]
    pub fn temporal_list_entities(&self) -> NapiResult<Vec<TemporalEntityOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        Ok(db
            .temporal_store()
            .expect("temporal store not available")
            .get_entities()
            .iter()
            .cloned()
            .map(entity_to_output)
            .collect())
    }

    #[napi(js_name = "temporalListEpisodes")]
    pub fn temporal_list_episodes(&self) -> NapiResult<Vec<TimelineEpisodeOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        Ok(db
            .temporal_store()
            .expect("temporal store not available")
            .get_episodes()
            .iter()
            .cloned()
            .map(episode_to_output)
            .collect())
    }

    #[napi(js_name = "temporalListFacts")]
    pub fn temporal_list_facts(&self) -> NapiResult<Vec<TimelineFactOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        Ok(db
            .temporal_store()
            .expect("temporal store not available")
            .get_facts()
            .iter()
            .cloned()
            .map(fact_to_output)
            .collect())
    }

    #[napi]
    pub fn add_fact(
        &self,
        subject: String,
        predicate: String,
        object: String,
    ) -> NapiResult<TripleOutput> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let triple = db
            .add_fact(Fact::new(
                subject.as_str(),
                predicate.as_str(),
                object.as_str(),
            ))
            .map_err(map_error)?;
        triple_to_output(triple)
    }

    #[napi]
    pub fn query(&self, criteria: Option<QueryCriteriaInput>) -> NapiResult<Vec<TripleOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let criteria = criteria.unwrap_or_default();
        let query = QueryCriteria {
            subject_id: convert_string_id(criteria.subject_id),
            predicate_id: convert_string_id(criteria.predicate_id),
            object_id: convert_string_id(criteria.object_id),
        };
        let triples: Vec<_> = db.query(query).collect();
        triples
            .into_iter()
            .map(triple_to_output)
            .collect::<NapiResult<Vec<_>>>()
    }

    #[napi]
    pub fn timeline_query(&self, input: TimelineQueryInput) -> NapiResult<Vec<TimelineFactOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let role = parse_timeline_role(input.role)?;
        let between = match (input.between_start, input.between_end) {
            (Some(start), Some(end)) => Some((start, end)),
            (None, None) => None,
            _ => {
                return Err(napi::Error::new(
                    Status::GenericFailure,
                    "between_start and between_end must be provided together",
                ));
            }
        };

        let entity_id = parse_id(&input.entity_id, "entity")?;

        let facts = db.timeline_query(TimelineQuery {
            entity_id,
            predicate_key: input.predicate_key,
            role,
            as_of: input.as_of,
            between,
        });

        Ok(facts.into_iter().map(fact_to_output).collect())
    }

    #[napi(js_name = "timelineTrace")]
    pub fn timeline_trace(&self, fact_id: String) -> NapiResult<Vec<TimelineEpisodeOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let fact_id = parse_id(&fact_id, "fact")?;
        let episodes = db.timeline_trace(fact_id);
        Ok(episodes.into_iter().map(episode_to_output).collect())
    }

    #[napi]
    pub fn hydrate(&self, dictionary: Vec<String>, triples: Vec<TripleInput>) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let triples = triples
            .into_iter()
            .map(|t| {
                (
                    t.subject_id as StringId,
                    t.predicate_id as StringId,
                    t.object_id as StringId,
                )
            })
            .collect();
        db.hydrate(dictionary, triples).map_err(map_error)
    }

    #[napi(js_name = "openCursor")]
    pub fn cursor_open(&self, criteria: Option<QueryCriteriaInput>) -> NapiResult<CursorId> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let criteria = criteria.unwrap_or_default();
        let query = QueryCriteria {
            subject_id: convert_string_id(criteria.subject_id),
            predicate_id: convert_string_id(criteria.predicate_id),
            object_id: convert_string_id(criteria.object_id),
        };
        let id = db.open_cursor(query).map_err(map_error)?;
        let cursor_id = u32::try_from(id).map_err(|_| {
            napi::Error::new(
                Status::GenericFailure,
                "cursor id overflow: exceeds 32 bits",
            )
        })?;
        Ok(CursorId { id: cursor_id })
    }

    #[napi(js_name = "readCursor")]
    pub fn cursor_next(&self, cursor_id: u32, batch_size: u32) -> NapiResult<CursorBatch> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (triples, done) = db
            .cursor_next(cursor_id as u64, batch_size as usize)
            .map_err(map_error)?;
        let mapped = triples
            .into_iter()
            .map(triple_to_output)
            .collect::<NapiResult<Vec<_>>>()?;
        Ok(CursorBatch {
            triples: mapped,
            done,
        })
    }

    #[napi(js_name = "closeCursor")]
    pub fn cursor_close(&self, cursor_id: u32) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        db.close_cursor(cursor_id as u64).map_err(map_error)
    }

    #[napi]
    pub fn close(&self) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        guard.take();
        Ok(())
    }
}

#[napi]
pub fn open(options: OpenOptions) -> NapiResult<DatabaseHandle> {
    let path = PathBuf::from(options.data_path);
    let db = Database::open(Options::new(path)).map_err(map_error)?;
    Ok(DatabaseHandle {
        inner: Mutex::new(Some(db)),
    })
}
