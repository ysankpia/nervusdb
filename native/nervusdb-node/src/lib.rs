use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::Mutex;

use napi::Result as NapiResult;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use nervusdb_core::{Database, Fact, Options, QueryCriteria, StringId, Triple};

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

fn convert_string_id(value: Option<u32>) -> Option<StringId> {
    value.map(|id| id as StringId)
}

fn triple_to_output(triple: Triple) -> NapiResult<TripleOutput> {
    Ok(TripleOutput {
        subject_id: u32::try_from(triple.subject_id).map_err(|_| {
            napi::Error::new(Status::GenericFailure, "subject id overflow")
        })?,
        predicate_id: u32::try_from(triple.predicate_id).map_err(|_| {
            napi::Error::new(Status::GenericFailure, "predicate id overflow")
        })?,
        object_id: u32::try_from(triple.object_id).map_err(|_| {
            napi::Error::new(Status::GenericFailure, "object id overflow")
        })?,
    })
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
        let triples = db.query(query);
        triples
            .into_iter()
            .map(triple_to_output)
            .collect::<NapiResult<Vec<_>>>()
    }

    #[napi]
    pub fn hydrate(
        &self,
        dictionary: Vec<String>,
        triples: Vec<TripleInput>,
    ) -> NapiResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "database mutex poisoned"))?;
        let db = guard
            .as_mut()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "database already closed"))?;
        let triples = triples
            .into_iter()
            .map(|t| (t.subject_id as StringId, t.predicate_id as StringId, t.object_id as StringId))
            .collect();
        db.hydrate(dictionary, triples).map_err(map_error)
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
