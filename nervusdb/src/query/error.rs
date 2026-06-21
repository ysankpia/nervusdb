//! Error and result types for the v2 query crate.

use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceLimitKind {
    IntermediateRows,
    CollectionItems,
    Timeout,
    ApplyRowsPerOuter,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    NotImplemented(&'static str),
    ResourceLimitExceeded {
        kind: ResourceLimitKind,
        limit: usize,
        observed: usize,
        stage: String,
    },
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "I/O error: {err}"),
            Error::NotImplemented(msg) => write!(f, "not implemented: {msg}"),
            Error::ResourceLimitExceeded {
                kind,
                limit,
                observed,
                stage,
            } => write!(
                f,
                "execution error: ResourceLimitExceeded(kind={kind:?}, limit={limit}, observed={observed}, stage={stage})"
            ),
            Error::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<Box<dyn std::error::Error + Send + Sync + 'static>> for Error {
    fn from(err: Box<dyn std::error::Error + Send + Sync + 'static>) -> Self {
        Error::Other(err.to_string())
    }
}

impl Error {
    pub fn resource_limit_exceeded(
        kind: ResourceLimitKind,
        limit: usize,
        observed: usize,
        stage: impl Into<String>,
    ) -> Self {
        Self::ResourceLimitExceeded {
            kind,
            limit,
            observed,
            stage: stage.into(),
        }
    }
}
