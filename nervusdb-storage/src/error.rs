use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("fjall error: {0}")]
    Fjall(#[from] fjall::Error),

    #[error("storage format mismatch: expected epoch {expected}, found {found}")]
    StorageFormatMismatch { expected: u64, found: u64 },

    #[error("storage corrupted: {0}")]
    StorageCorrupted(String),

    #[error("duplicate external id: {0}")]
    DuplicateExternalId(u64),

    #[error("node not found: {0}")]
    NodeNotFound(u32),

    #[error("property decode error: {0}")]
    PropertyDecode(String),
}
