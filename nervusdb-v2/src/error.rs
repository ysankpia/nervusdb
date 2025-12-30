use std::fmt;

/// The error type for NervusDB operations.
#[derive(Debug)]
pub enum Error {
    /// IO error interacting with the filesystem.
    Io(std::io::Error),
    /// Error returned by the storage engine.
    Storage(String),
    /// Error during query execution.
    Query(String),
    /// Other errors.
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Storage(e) => write!(f, "Storage error: {}", e),
            Error::Query(e) => write!(f, "Query error: {}", e),
            Error::Other(e) => write!(f, "Error: {}", e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

// Convert storage errors to string to hide internal types
impl From<nervusdb_v2_storage::Error> for Error {
    fn from(e: nervusdb_v2_storage::Error) -> Self {
        match e {
            nervusdb_v2_storage::Error::Io(e) => Error::Io(e),
            _ => Error::Storage(e.to_string()),
        }
    }
}

// Convert query errors to string to hide internal types
impl From<nervusdb_v2_query::Error> for Error {
    fn from(e: nervusdb_v2_query::Error) -> Self {
        match e {
            nervusdb_v2_query::Error::Io(e) => Error::Io(e),
            _ => Error::Query(e.to_string()),
        }
    }
}

/// A specialized Result type for NervusDB operations.
pub type Result<T> = std::result::Result<T, Error>;
