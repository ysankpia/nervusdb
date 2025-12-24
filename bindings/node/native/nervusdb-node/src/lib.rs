use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use napi::bindgen_prelude::{BigInt, *};
use napi::Result as NapiResult;
use napi::{JsBoolean, JsNumber, JsString, JsUnknown, ValueType};
use napi_derive::napi;

use nervusdb_core::{Database, Fact, Options, QueryCriteria, StringId, Triple};
#[cfg(feature = "temporal")]
use nervusdb_core::{
    EnsureEntityOptions as CoreEnsureEntityOptions, EpisodeInput as CoreEpisodeInput,
    EpisodeLinkOptions as CoreEpisodeLinkOptions, EpisodeLinkRecord,
    FactWriteInput as CoreFactWriteInput, StoredEntity, StoredEpisode, StoredFact, TimelineQuery,
    TimelineRole,
};
#[cfg(feature = "temporal")]
use serde_json::Value;

fn map_error(err: nervusdb_core::Error) -> napi::Error {
    napi::Error::new(Status::GenericFailure, format!("{err}"))
}

fn debug_logging_enabled() -> bool {
    match std::env::var("NERVUSDB_DEBUG_NATIVE") {
        Ok(val) => val == "1" || val.eq_ignore_ascii_case("true"),
        Err(_) => false,
    }
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
    pub subject_id: BigInt,
    pub predicate_id: BigInt,
    pub object_id: BigInt,
}

#[napi(object)]
pub struct TripleInput {
    pub subject_id: BigInt,
    pub predicate_id: BigInt,
    pub object_id: BigInt,
}

#[napi(object)]
#[derive(Default)]
pub struct QueryCriteriaInput {
    pub subject_id: Option<BigInt>,
    pub predicate_id: Option<BigInt>,
    pub object_id: Option<BigInt>,
}

#[napi(object)]
pub struct CursorId {
    pub id: BigInt,
}

#[napi(object)]
pub struct CursorBatch {
    pub triples: Vec<TripleOutput>,
    pub done: bool,
}

#[napi(object)]
pub struct FactOutput {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub subject_id: BigInt,
    pub predicate_id: BigInt,
    pub object_id: BigInt,
}

#[napi(object)]
pub struct FactCursorBatch {
    pub facts: Vec<FactOutput>,
    pub done: bool,
}

// ---------------------------------------------------------------------------
// Statement API (SQLite-style row iterator) for Node
// ---------------------------------------------------------------------------

type CoreValue = nervusdb_core::query::executor::Value;

const VALUE_NULL: i32 = 0;
const VALUE_TEXT: i32 = 1;
const VALUE_FLOAT: i32 = 2;
const VALUE_BOOL: i32 = 3;
const VALUE_NODE: i32 = 4;
const VALUE_RELATIONSHIP: i32 = 5;

struct StatementInner {
    columns: Vec<String>,
    rows: Vec<Vec<CoreValue>>,
    next_row: usize,
    current_row: Option<usize>,
}

#[napi]
pub struct StatementHandle {
    inner: Option<StatementInner>,
}

impl StatementHandle {
    fn inner(&self) -> NapiResult<&StatementInner> {
        self.inner
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "statement already finalized"))
    }

    fn inner_mut(&mut self) -> NapiResult<&mut StatementInner> {
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "statement already finalized"))
    }

    fn cell(&self, column: i32) -> NapiResult<Option<&CoreValue>> {
        let stmt = self.inner()?;
        let idx = match usize::try_from(column) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        let row_idx = match stmt.current_row {
            Some(v) => v,
            None => return Ok(None),
        };
        Ok(stmt.rows.get(row_idx).and_then(|row| row.get(idx)))
    }
}

#[napi]
impl StatementHandle {
    #[napi]
    pub fn step(&mut self) -> NapiResult<bool> {
        let stmt = self.inner_mut()?;
        stmt.current_row = None;
        if stmt.next_row >= stmt.rows.len() {
            return Ok(false);
        }
        stmt.current_row = Some(stmt.next_row);
        stmt.next_row += 1;
        Ok(true)
    }

    #[napi(js_name = "columnCount")]
    pub fn column_count(&self) -> NapiResult<u32> {
        let stmt = self.inner()?;
        Ok(u32::try_from(stmt.columns.len()).unwrap_or(u32::MAX))
    }

    #[napi(js_name = "columnName")]
    pub fn column_name(&self, column: i32) -> NapiResult<Option<String>> {
        let stmt = self.inner()?;
        let idx = match usize::try_from(column) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        Ok(stmt.columns.get(idx).cloned())
    }

    #[napi(js_name = "columnType")]
    pub fn column_type(&self, column: i32) -> NapiResult<i32> {
        match self.cell(column)? {
            Some(CoreValue::String(_)) => Ok(VALUE_TEXT),
            Some(CoreValue::Float(_)) => Ok(VALUE_FLOAT),
            Some(CoreValue::Boolean(_)) => Ok(VALUE_BOOL),
            Some(CoreValue::Null) => Ok(VALUE_NULL),
            Some(CoreValue::Vector(_)) => Ok(VALUE_TEXT),
            Some(CoreValue::Node(_)) => Ok(VALUE_NODE),
            Some(CoreValue::Relationship(_)) => Ok(VALUE_RELATIONSHIP),
            None => Ok(VALUE_NULL),
        }
    }

    #[napi(js_name = "columnText")]
    pub fn column_text(&self, column: i32) -> NapiResult<Option<String>> {
        match self.cell(column)? {
            Some(CoreValue::String(value)) => Ok(Some(value.clone())),
            Some(CoreValue::Vector(items)) => {
                let json = serde_json::Value::Array(
                    items
                        .iter()
                        .map(|f| {
                            serde_json::Number::from_f64(*f as f64)
                                .map(serde_json::Value::Number)
                                .unwrap_or(serde_json::Value::Null)
                        })
                        .collect(),
                );
                Ok(Some(json.to_string()))
            }
            _ => Ok(None),
        }
    }

    #[napi(js_name = "columnFloat")]
    pub fn column_float(&self, column: i32) -> NapiResult<Option<f64>> {
        match self.cell(column)? {
            Some(CoreValue::Float(value)) => Ok(Some(*value)),
            _ => Ok(None),
        }
    }

    #[napi(js_name = "columnBool")]
    pub fn column_bool(&self, column: i32) -> NapiResult<Option<bool>> {
        match self.cell(column)? {
            Some(CoreValue::Boolean(value)) => Ok(Some(*value)),
            _ => Ok(None),
        }
    }

    #[napi(js_name = "columnNodeId")]
    pub fn column_node_id(&self, column: i32) -> NapiResult<Option<BigInt>> {
        match self.cell(column)? {
            Some(CoreValue::Node(value)) => Ok(Some(BigInt::from(*value))),
            _ => Ok(None),
        }
    }

    #[napi(js_name = "columnRelationship")]
    pub fn column_relationship(&self, column: i32) -> NapiResult<Option<TripleOutput>> {
        match self.cell(column)? {
            Some(CoreValue::Relationship(value)) => Ok(Some(triple_to_output(*value))),
            _ => Ok(None),
        }
    }

    #[napi(js_name = "finalize")]
    pub fn finalize(&mut self) -> NapiResult<()> {
        self.inner = None;
        Ok(())
    }
}

// Graph Algorithm Output Types

#[napi(object)]
pub struct PathResultOutput {
    pub path: Vec<BigInt>,
    pub cost: f64,
    pub hops: u32,
}

#[napi(object)]
pub struct PageRankEntryOutput {
    pub node_id: BigInt,
    pub score: f64,
}

#[napi(object)]
pub struct PageRankResultOutput {
    pub scores: Vec<PageRankEntryOutput>,
    pub iterations: u32,
    pub converged: bool,
}

#[cfg(feature = "temporal")]
#[napi(object)]
pub struct TimelineQueryInput {
    pub entity_id: String,
    pub predicate_key: Option<String>,
    pub role: Option<String>,
    pub as_of: Option<String>,
    pub between_start: Option<String>,
    pub between_end: Option<String>,
}

#[cfg(feature = "temporal")]
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

#[cfg(feature = "temporal")]
#[napi(object)]
pub struct TimelineEpisodeOutput {
    pub episode_id: String,
    pub source_type: String,
    pub payload: String,
    pub occurred_at: String,
    pub ingested_at: String,
    pub trace_hash: String,
}

#[cfg(feature = "temporal")]
#[napi(object)]
pub struct TemporalEpisodeInput {
    pub source_type: String,
    pub payload_json: String,
    pub occurred_at: String,
    pub trace_hash: Option<String>,
}

#[cfg(feature = "temporal")]
#[napi(object)]
pub struct TemporalEnsureEntityInput {
    pub kind: String,
    pub canonical_name: String,
    pub alias: Option<String>,
    pub confidence: Option<f64>,
    pub occurred_at: Option<String>,
    pub version_increment: Option<bool>,
}

#[cfg(feature = "temporal")]
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

#[cfg(feature = "temporal")]
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

#[cfg(feature = "temporal")]
#[napi(object)]
pub struct TemporalLinkInput {
    pub episode_id: String,
    pub entity_id: Option<String>,
    pub fact_id: Option<String>,
    pub role: String,
}

#[cfg(feature = "temporal")]
#[napi(object)]
pub struct TemporalLinkOutput {
    pub link_id: String,
    pub episode_id: String,
    pub entity_id: Option<String>,
    pub fact_id: Option<String>,
    pub role: String,
}

fn convert_string_id(value: Option<BigInt>) -> Option<StringId> {
    value.map(|bigint| {
        let (_, val, _) = bigint.get_u64();
        val
    })
}

fn js_value_to_serde(value: JsUnknown) -> NapiResult<serde_json::Value> {
    use serde_json::Value as JsonValue;

    match value.get_type()? {
        ValueType::Null | ValueType::Undefined => Ok(JsonValue::Null),
        ValueType::Boolean => {
            let bool_value = unsafe { value.cast::<JsBoolean>() }.get_value()?;
            Ok(JsonValue::Bool(bool_value))
        }
        ValueType::Number => {
            let num_value = unsafe { value.cast::<JsNumber>() }.get_double()?;
            serde_json::Number::from_f64(num_value)
                .map(JsonValue::Number)
                .ok_or_else(|| {
                    napi::Error::new(
                        Status::InvalidArg,
                        "Failed to convert numeric parameter to JSON value",
                    )
                })
        }
        ValueType::String => {
            let js_string = unsafe { value.cast::<JsString>() };
            let utf8 = js_string.into_utf8()?;
            let owned = utf8.as_str()?.to_owned();
            Ok(JsonValue::String(owned))
        }
        _ => Err(napi::Error::new(
            Status::InvalidArg,
            "executeQuery parameters currently support only string, number, boolean, or null values",
        )),
    }
}

fn triples_from_inputs(inputs: Vec<TripleInput>) -> Vec<Triple> {
    inputs
        .into_iter()
        .map(|input| {
            let (_, subject, _) = input.subject_id.get_u64();
            let (_, predicate, _) = input.predicate_id.get_u64();
            let (_, object, _) = input.object_id.get_u64();
            Triple::new(subject, predicate, object)
        })
        .collect()
}

fn triple_to_output(triple: Triple) -> TripleOutput {
    TripleOutput {
        subject_id: BigInt::from(triple.subject_id),
        predicate_id: BigInt::from(triple.predicate_id),
        object_id: BigInt::from(triple.object_id),
    }
}

fn resolve_str_cached(
    db: &Database,
    cache: &mut HashMap<u64, String>,
    id: u64,
) -> NapiResult<String> {
    if let Some(value) = cache.get(&id) {
        return Ok(value.clone());
    }
    match db.resolve_str(id).map_err(map_error)? {
        Some(value) => {
            cache.insert(id, value.clone());
            Ok(value)
        }
        None => Err(napi::Error::new(
            Status::GenericFailure,
            format!("failed to resolve string id: {id}"),
        )),
    }
}

fn triple_to_fact_output(
    db: &Database,
    cache: &mut HashMap<u64, String>,
    triple: Triple,
) -> NapiResult<FactOutput> {
    let subject = resolve_str_cached(db, cache, triple.subject_id)?;
    let predicate = resolve_str_cached(db, cache, triple.predicate_id)?;
    let object = resolve_str_cached(db, cache, triple.object_id)?;
    Ok(FactOutput {
        subject,
        predicate,
        object,
        subject_id: BigInt::from(triple.subject_id),
        predicate_id: BigInt::from(triple.predicate_id),
        object_id: BigInt::from(triple.object_id),
    })
}

#[cfg(feature = "temporal")]
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

#[cfg(feature = "temporal")]
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

#[cfg(feature = "temporal")]
fn parse_id(value: &str, field: &str) -> NapiResult<u64> {
    value.parse::<u64>().map_err(|err| {
        napi::Error::new(
            Status::GenericFailure,
            format!("invalid {field} id '{value}': {err}"),
        )
    })
}

#[cfg(feature = "temporal")]
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

#[cfg(feature = "temporal")]
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

#[cfg(feature = "temporal")]
fn link_to_output(record: EpisodeLinkRecord) -> TemporalLinkOutput {
    TemporalLinkOutput {
        link_id: record.link_id.to_string(),
        episode_id: record.episode_id.to_string(),
        entity_id: record.entity_id.map(|id| id.to_string()),
        fact_id: record.fact_id.map(|id| id.to_string()),
        role: record.role,
    }
}

#[cfg(feature = "temporal")]
fn parse_optional_id(value: Option<String>, field: &str) -> NapiResult<Option<u64>> {
    match value {
        Some(raw) => parse_id(&raw, field).map(Some),
        None => Ok(None),
    }
}

#[cfg(feature = "temporal")]
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

#[cfg(feature = "temporal")]
fn parse_payload_json(raw: &str) -> NapiResult<Value> {
    serde_json::from_str(raw).map_err(|err| {
        napi::Error::new(
            Status::GenericFailure,
            format!("invalid episode payload JSON: {err}"),
        )
    })
}

#[cfg(feature = "temporal")]
#[napi]
impl DatabaseHandle {
    #[cfg(feature = "temporal")]
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
            .add_episode(CoreEpisodeInput {
                source_type: input.source_type,
                payload,
                occurred_at: input.occurred_at,
                trace_hash: input.trace_hash,
            })
            .map_err(map_error)?;
        Ok(episode_to_output(episode))
    }

    #[cfg(feature = "temporal")]
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
            .ensure_entity(&input.kind, &input.canonical_name, options)
            .map_err(map_error)?;
        Ok(entity_to_output(entity))
    }

    #[cfg(feature = "temporal")]
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

    #[cfg(feature = "temporal")]
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

    #[cfg(feature = "temporal")]
    #[napi(js_name = "temporalListEntities")]
    pub fn temporal_list_entities(&self) -> NapiResult<Vec<TemporalEntityOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let entities = db.temporal_store().get_entities().map_err(map_error)?;

        Ok(entities.into_iter().map(entity_to_output).collect())
    }

    #[cfg(feature = "temporal")]
    #[napi(js_name = "temporalListEpisodes")]
    pub fn temporal_list_episodes(&self) -> NapiResult<Vec<TimelineEpisodeOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let episodes = db.temporal_store().get_episodes().map_err(map_error)?;

        Ok(episodes.into_iter().map(episode_to_output).collect())
    }

    #[cfg(feature = "temporal")]
    #[napi(js_name = "temporalListFacts")]
    pub fn temporal_list_facts(&self) -> NapiResult<Vec<TimelineFactOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let facts = db.temporal_store().get_facts().map_err(map_error)?;

        Ok(facts.into_iter().map(fact_to_output).collect())
    }
}

#[napi]
impl DatabaseHandle {
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
        Ok(triple_to_output(triple))
    }

    #[napi]
    pub fn delete_fact(
        &self,
        subject: String,
        predicate: String,
        object: String,
    ) -> NapiResult<bool> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        db.delete_fact(Fact::new(
            subject.as_str(),
            predicate.as_str(),
            object.as_str(),
        ))
        .map_err(map_error)
    }

    #[napi(js_name = "batchAddFacts")]
    pub fn batch_add_facts(&self, triples: Vec<TripleInput>) -> NapiResult<BigInt> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        if triples.is_empty() {
            return Ok(BigInt::from(0u64));
        }
        let core_triples = triples_from_inputs(triples);
        let inserted = db.batch_insert(&core_triples).map_err(map_error)?;
        Ok(BigInt::from(inserted as u64))
    }

    #[napi(js_name = "batchDeleteFacts")]
    pub fn batch_delete_facts(&self, triples: Vec<TripleInput>) -> NapiResult<BigInt> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        if triples.is_empty() {
            return Ok(BigInt::from(0u64));
        }
        let core_triples = triples_from_inputs(triples);
        let deleted = db.batch_delete(&core_triples).map_err(map_error)?;
        Ok(BigInt::from(deleted as u64))
    }

    #[napi]
    pub fn intern(&self, value: String) -> NapiResult<BigInt> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let id = db.intern(&value).map_err(map_error)?;
        Ok(BigInt::from(id))
    }

    #[napi]
    pub fn resolve_id(&self, value: String) -> NapiResult<Option<BigInt>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let id = db.resolve_id(&value).map_err(map_error)?;
        Ok(id.map(BigInt::from))
    }

    #[napi]
    pub fn resolve_str(&self, id: BigInt) -> NapiResult<Option<String>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, val, _) = id.get_u64();
        db.resolve_str(val).map_err(map_error)
    }

    #[napi(js_name = "getDictionarySize")]
    pub fn dictionary_size(&self) -> NapiResult<BigInt> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let size = db.dictionary_size().map_err(map_error)?;
        Ok(BigInt::from(size))
    }

    #[napi(js_name = "executeQuery")]
    pub fn execute_query(
        &self,
        query: String,
        params: Option<Object>,
    ) -> NapiResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let params_map: Option<HashMap<String, serde_json::Value>> = match params {
            Some(obj) => {
                let mut map: HashMap<String, serde_json::Value> = HashMap::new();
                for key in Object::keys(&obj)? {
                    let maybe_value = obj.get::<_, JsUnknown>(&key)?;
                    let js_value = maybe_value.ok_or_else(|| {
                        napi::Error::new(
                            Status::GenericFailure,
                            format!(
                                "executeQuery parameter `{}` was undefined; parameters must be JSON-serializable",
                                key
                            ),
                        )
                    })?;
                    let value = js_value_to_serde(js_value)?;
                    map.insert(key, value);
                }
                if debug_logging_enabled() {
                    eprintln!("[nervusdb-native] executeQuery params: {:?}", map);
                }
                Some(map)
            }
            None => None,
        };

        if query.contains('$') && params_map.as_ref().map(|m| m.is_empty()).unwrap_or(true) {
            return Err(napi::Error::new(
                Status::GenericFailure,
                "executeQuery was called with parameterized query but no params were provided",
            ));
        }

        let results = db
            .execute_query_with_params(&query, params_map)
            .map_err(map_error)?;

        // Convert internal Value to serde_json::Value
        let json_results = results
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|(k, v)| {
                        let json_val = match v {
                            nervusdb_core::query::executor::Value::String(s) => {
                                serde_json::Value::String(s)
                            }
                            nervusdb_core::query::executor::Value::Float(f) => serde_json::json!(f),
                            nervusdb_core::query::executor::Value::Boolean(b) => {
                                serde_json::Value::Bool(b)
                            }
                            nervusdb_core::query::executor::Value::Null => serde_json::Value::Null,
                            nervusdb_core::query::executor::Value::Vector(v) => {
                                serde_json::Value::Array(
                                    v.into_iter()
                                        .map(|f| {
                                            serde_json::Number::from_f64(f as f64)
                                                .map(serde_json::Value::Number)
                                                .unwrap_or(serde_json::Value::Null)
                                        })
                                        .collect(),
                                )
                            }
                            nervusdb_core::query::executor::Value::Node(id) => {
                                serde_json::json!({ "id": id })
                            }
                            nervusdb_core::query::executor::Value::Relationship(id) => {
                                serde_json::json!({ "id": id })
                            }
                        };
                        (k, json_val)
                    })
                    .collect()
            })
            .collect();

        Ok(json_results)
    }

    #[napi(js_name = "prepareV2")]
    pub fn prepare_v2(&self, query: String, params: Option<Object>) -> NapiResult<StatementHandle> {
        use nervusdb_core::query::ast::Clause;
        use nervusdb_core::query::ast::Expression;
        use nervusdb_core::query::parser::Parser;

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let params_map: Option<HashMap<String, serde_json::Value>> = match params {
            Some(obj) => {
                let mut map: HashMap<String, serde_json::Value> = HashMap::new();
                for key in Object::keys(&obj)? {
                    let maybe_value = obj.get::<_, JsUnknown>(&key)?;
                    let js_value = maybe_value.ok_or_else(|| {
                        napi::Error::new(
                            Status::GenericFailure,
                            format!(
                                "prepareV2 parameter `{}` was undefined; parameters must be JSON-serializable",
                                key
                            ),
                        )
                    })?;
                    let value = js_value_to_serde(js_value)?;
                    map.insert(key, value);
                }
                Some(map)
            }
            None => None,
        };

        if query.contains('$') && params_map.as_ref().map(|m| m.is_empty()).unwrap_or(true) {
            return Err(napi::Error::new(
                Status::GenericFailure,
                "prepareV2 was called with parameterized query but no params were provided",
            ));
        }

        let results = db
            .execute_query_with_params(&query, params_map)
            .map_err(map_error)?;

        fn infer_projection_alias(expr: &Expression) -> String {
            match expr {
                Expression::Variable(name) => name.clone(),
                Expression::PropertyAccess(pa) => format!("{}.{}", pa.variable, pa.property),
                _ => "col".to_string(),
            }
        }

        let mut projection_names: Vec<String> = Vec::new();
        match Parser::parse(query.as_str()) {
            Ok(ast) => {
                for clause in ast.clauses {
                    if let Clause::Return(r) = clause {
                        projection_names = r
                            .items
                            .into_iter()
                            .map(|item| {
                                item.alias
                                    .unwrap_or_else(|| infer_projection_alias(&item.expression))
                            })
                            .collect();
                        break;
                    }
                }
            }
            Err(err) => return Err(napi::Error::new(Status::InvalidArg, err.to_string())),
        }

        if !projection_names.is_empty() {
            let mut seen = std::collections::HashSet::new();
            for name in &projection_names {
                if !seen.insert(name) {
                    return Err(napi::Error::new(
                        Status::InvalidArg,
                        format!("duplicate column name: {name}; use explicit aliases"),
                    ));
                }
            }
        }

        let columns: Vec<String> = if !projection_names.is_empty() {
            projection_names
        } else if results.is_empty() {
            Vec::new()
        } else {
            let mut keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for row in &results {
                keys.extend(row.keys().cloned());
            }
            keys.into_iter().collect()
        };

        let mut rows: Vec<Vec<CoreValue>> = Vec::with_capacity(results.len());
        for row in results {
            let mut out_row = Vec::with_capacity(columns.len());
            for col in &columns {
                out_row.push(row.get(col).cloned().unwrap_or(CoreValue::Null));
            }
            rows.push(out_row);
        }

        Ok(StatementHandle {
            inner: Some(StatementInner {
                columns,
                rows,
                next_row: 0,
                current_row: None,
            }),
        })
    }

    #[napi(js_name = "setNodeProperty")]
    pub fn set_node_property(&self, node_id: BigInt, json: String) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, id, _) = node_id.get_u64();
        db.set_node_property(id, &json).map_err(map_error)
    }

    /// Set node properties directly from JS object (v1.1 - bypasses JSON string)
    #[napi(js_name = "setNodePropertyDirect")]
    pub fn set_node_property_direct(
        &self,
        node_id: BigInt,
        properties: serde_json::Value,
    ) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, id, _) = node_id.get_u64();

        // Convert serde_json::Value to HashMap and serialize to FlexBuffers
        let props: std::collections::HashMap<String, serde_json::Value> = match properties {
            serde_json::Value::Object(map) => map.into_iter().collect(),
            _ => {
                return Err(napi::Error::new(
                    Status::InvalidArg,
                    "properties must be an object",
                ))
            }
        };
        let binary =
            nervusdb_core::storage::property::serialize_properties(&props).map_err(map_error)?;
        db.set_node_property_binary(id, &binary).map_err(map_error)
    }

    #[napi(js_name = "getNodeProperty")]
    pub fn get_node_property(&self, node_id: BigInt) -> NapiResult<Option<String>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, id, _) = node_id.get_u64();
        db.get_node_property(id).map_err(map_error)
    }

    /// Get node properties directly as JS object (v1.1 - bypasses JSON string)
    #[napi(js_name = "getNodePropertyDirect")]
    pub fn get_node_property_direct(
        &self,
        node_id: BigInt,
    ) -> NapiResult<Option<serde_json::Value>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, id, _) = node_id.get_u64();

        if let Some(binary) = db.get_node_property_binary(id).map_err(map_error)? {
            let props = nervusdb_core::storage::property::deserialize_properties(&binary)
                .map_err(map_error)?;
            Ok(Some(serde_json::Value::Object(props.into_iter().collect())))
        } else {
            Ok(None)
        }
    }

    #[napi(js_name = "setEdgeProperty")]
    pub fn set_edge_property(
        &self,
        subject_id: BigInt,
        predicate_id: BigInt,
        object_id: BigInt,
        json: String,
    ) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, s, _) = subject_id.get_u64();
        let (_, p, _) = predicate_id.get_u64();
        let (_, o, _) = object_id.get_u64();
        db.set_edge_property(s, p, o, &json).map_err(map_error)
    }

    /// Set edge properties directly from JS object (v1.1 - bypasses JSON string)
    #[napi(js_name = "setEdgePropertyDirect")]
    pub fn set_edge_property_direct(
        &self,
        subject_id: BigInt,
        predicate_id: BigInt,
        object_id: BigInt,
        properties: serde_json::Value,
    ) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, s, _) = subject_id.get_u64();
        let (_, p, _) = predicate_id.get_u64();
        let (_, o, _) = object_id.get_u64();

        let props: std::collections::HashMap<String, serde_json::Value> = match properties {
            serde_json::Value::Object(map) => map.into_iter().collect(),
            _ => {
                return Err(napi::Error::new(
                    Status::InvalidArg,
                    "properties must be an object",
                ))
            }
        };
        let binary =
            nervusdb_core::storage::property::serialize_properties(&props).map_err(map_error)?;
        db.set_edge_property_binary(s, p, o, &binary)
            .map_err(map_error)
    }

    #[napi(js_name = "getEdgeProperty")]
    pub fn get_edge_property(
        &self,
        subject_id: BigInt,
        predicate_id: BigInt,
        object_id: BigInt,
    ) -> NapiResult<Option<String>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, s, _) = subject_id.get_u64();
        let (_, p, _) = predicate_id.get_u64();
        let (_, o, _) = object_id.get_u64();
        db.get_edge_property(s, p, o).map_err(map_error)
    }

    /// Get edge properties directly as JS object (v1.1 - bypasses JSON string)
    #[napi(js_name = "getEdgePropertyDirect")]
    pub fn get_edge_property_direct(
        &self,
        subject_id: BigInt,
        predicate_id: BigInt,
        object_id: BigInt,
    ) -> NapiResult<Option<serde_json::Value>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, s, _) = subject_id.get_u64();
        let (_, p, _) = predicate_id.get_u64();
        let (_, o, _) = object_id.get_u64();

        if let Some(binary) = db.get_edge_property_binary(s, p, o).map_err(map_error)? {
            let props = nervusdb_core::storage::property::deserialize_properties(&binary)
                .map_err(map_error)?;
            Ok(Some(serde_json::Value::Object(props.into_iter().collect())))
        } else {
            Ok(None)
        }
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
        Ok(triples.into_iter().map(triple_to_output).collect())
    }

    #[napi(js_name = "queryFacts")]
    pub fn query_facts(&self, criteria: Option<QueryCriteriaInput>) -> NapiResult<Vec<FactOutput>> {
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
        let mut cache: HashMap<u64, String> = HashMap::new();
        let mut out = Vec::with_capacity(triples.len());
        for triple in triples {
            out.push(triple_to_fact_output(db, &mut cache, triple)?);
        }
        Ok(out)
    }
}

#[cfg(feature = "temporal")]
#[napi]
impl DatabaseHandle {
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
}

#[napi]
impl DatabaseHandle {
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
                let (_, s, _) = t.subject_id.get_u64();
                let (_, p, _) = t.predicate_id.get_u64();
                let (_, o, _) = t.object_id.get_u64();
                (s, p, o)
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
        Ok(CursorId {
            id: BigInt::from(id),
        })
    }

    #[napi(js_name = "readCursor")]
    pub fn cursor_next(&self, cursor_id: BigInt, batch_size: u32) -> NapiResult<CursorBatch> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, id, _) = cursor_id.get_u64();
        let (triples, done) = db.cursor_next(id, batch_size as usize).map_err(map_error)?;
        let mapped = triples.into_iter().map(triple_to_output).collect();
        Ok(CursorBatch {
            triples: mapped,
            done,
        })
    }

    #[napi(js_name = "readCursorFacts")]
    pub fn cursor_next_facts(
        &self,
        cursor_id: BigInt,
        batch_size: u32,
    ) -> NapiResult<FactCursorBatch> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, id, _) = cursor_id.get_u64();
        let (triples, done) = db.cursor_next(id, batch_size as usize).map_err(map_error)?;

        let mut cache: HashMap<u64, String> = HashMap::new();
        let mut facts = Vec::with_capacity(triples.len());
        for triple in triples {
            facts.push(triple_to_fact_output(db, &mut cache, triple)?);
        }

        Ok(FactCursorBatch { facts, done })
    }

    #[napi(js_name = "closeCursor")]
    pub fn cursor_close(&self, cursor_id: BigInt) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let (_, id, _) = cursor_id.get_u64();
        db.close_cursor(id).map_err(map_error)
    }

    #[napi(js_name = "beginTransaction")]
    pub fn begin_transaction(&self) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        db.begin_transaction().map_err(map_error)
    }

    #[napi(js_name = "commitTransaction")]
    pub fn commit_transaction(&self) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        db.commit_transaction().map_err(map_error)
    }

    #[napi(js_name = "abortTransaction")]
    pub fn abort_transaction(&self) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        db.abort_transaction().map_err(map_error)
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

    // =========================================================================
    // Graph Algorithms
    // =========================================================================

    /// BFS shortest path (unweighted)
    #[napi(js_name = "bfsShortestPath")]
    pub fn bfs_shortest_path(
        &self,
        start_id: BigInt,
        end_id: BigInt,
        predicate_id: Option<BigInt>,
        max_hops: Option<u32>,
        bidirectional: Option<bool>,
    ) -> NapiResult<Option<PathResultOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let (_, start, _) = start_id.get_u64();
        let (_, end, _) = end_id.get_u64();
        let pred = predicate_id.map(|p| {
            let (_, val, _) = p.get_u64();
            val
        });
        let max = max_hops.unwrap_or(100) as usize;
        let bidir = bidirectional.unwrap_or(false);

        use nervusdb_core::algorithms::bfs_shortest_path;
        match bfs_shortest_path(db.get_store(), start, end, pred, max, bidir) {
            Some(result) => Ok(Some(PathResultOutput {
                path: result.path.into_iter().map(BigInt::from).collect(),
                cost: result.cost,
                hops: result.hops as u32,
            })),
            None => Ok(None),
        }
    }

    /// Dijkstra shortest path (weighted, uniform weight = 1.0)
    #[napi(js_name = "dijkstraShortestPath")]
    pub fn dijkstra_shortest_path(
        &self,
        start_id: BigInt,
        end_id: BigInt,
        predicate_id: Option<BigInt>,
        max_hops: Option<u32>,
    ) -> NapiResult<Option<PathResultOutput>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let (_, start, _) = start_id.get_u64();
        let (_, end, _) = end_id.get_u64();
        let pred = predicate_id.map(|p| {
            let (_, val, _) = p.get_u64();
            val
        });
        let max = max_hops.unwrap_or(100) as usize;

        use nervusdb_core::algorithms::dijkstra_shortest_path;
        let weight_fn = |_s: u64, _p: u64, _o: u64| 1.0;

        match dijkstra_shortest_path(db.get_store(), start, end, pred, weight_fn, max) {
            Some(result) => Ok(Some(PathResultOutput {
                path: result.path.into_iter().map(BigInt::from).collect(),
                cost: result.cost,
                hops: result.hops as u32,
            })),
            None => Ok(None),
        }
    }

    /// PageRank algorithm
    #[napi(js_name = "pagerank")]
    pub fn pagerank(
        &self,
        predicate_id: Option<BigInt>,
        damping: Option<f64>,
        max_iterations: Option<u32>,
        tolerance: Option<f64>,
    ) -> NapiResult<PageRankResultOutput> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_ref()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;

        let pred = predicate_id.map(|p| {
            let (_, val, _) = p.get_u64();
            val
        });

        use nervusdb_core::algorithms::{pagerank, PageRankOptions};
        let options = PageRankOptions {
            damping: damping.unwrap_or(0.85),
            max_iterations: max_iterations.unwrap_or(100) as usize,
            tolerance: tolerance.unwrap_or(1e-6),
        };

        let result = pagerank(db.get_store(), pred, options);

        let scores: Vec<PageRankEntryOutput> = result
            .scores
            .into_iter()
            .map(|(node_id, score)| PageRankEntryOutput {
                node_id: BigInt::from(node_id),
                score,
            })
            .collect();

        Ok(PageRankResultOutput {
            scores,
            iterations: result.iterations as u32,
            converged: result.converged,
        })
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
