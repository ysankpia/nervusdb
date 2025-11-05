use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::Mutex;

use napi::Result as NapiResult;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use nervusdb_core::{Database, Fact, Options};

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
pub struct AddFactOutput {
    pub subject_id: u32,
    pub predicate_id: u32,
    pub object_id: u32,
}

#[napi]
impl DatabaseHandle {
    #[napi]
    pub fn add_fact(
        &self,
        subject: String,
        predicate: String,
        object: String,
    ) -> NapiResult<AddFactOutput> {
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
        Ok(AddFactOutput {
            subject_id: u32::try_from(triple.subject_id)
                .map_err(|_| napi::Error::new(Status::GenericFailure, "subject id overflow"))?,
            predicate_id: u32::try_from(triple.predicate_id)
                .map_err(|_| napi::Error::new(Status::GenericFailure, "predicate id overflow"))?,
            object_id: u32::try_from(triple.object_id)
                .map_err(|_| napi::Error::new(Status::GenericFailure, "object id overflow"))?,
        })
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
