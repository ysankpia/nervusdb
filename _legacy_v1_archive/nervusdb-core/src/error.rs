//! Basic error and result types shared across the core crate.

use std::io;

/// Result type used across the core crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Minimal error enumeration for the prototype storage engine.
#[derive(Debug)]
pub enum Error {
    /// Wrapper around standard I/O errors.
    Io(io::Error),
    /// Tried to look up a string that does not exist in the dictionary.
    UnknownString(String),
    /// Attempted to use an invalid cursor identifier.
    InvalidCursor(u64),
    /// Generic placeholder for unimplemented features.
    NotImplemented(&'static str),
    /// Miscellaneous error message.
    Other(String),
    /// Entity or Fact not found.
    NotFound,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "I/O error: {err}"),
            Error::UnknownString(s) => write!(f, "unknown string: {s}"),
            Error::InvalidCursor(id) => write!(f, "invalid cursor id: {id}"),
            Error::NotImplemented(msg) => write!(f, "not implemented: {msg}"),
            Error::Other(msg) => write!(f, "{msg}"),
            Error::NotFound => write!(f, "not found"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

#[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
impl From<nervusdb_temporal::Error> for Error {
    fn from(err: nervusdb_temporal::Error) -> Self {
        Error::Other(err.to_string())
    }
}
