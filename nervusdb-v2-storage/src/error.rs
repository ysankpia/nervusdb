use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid file magic")]
    InvalidMagic,

    #[error("unsupported page size: {0}")]
    UnsupportedPageSize(u64),

    #[error("page id {0} out of range")]
    PageIdOutOfRange(u64),

    #[error("page {0} not allocated")]
    PageNotAllocated(u64),

    #[error("wal record too large: {0}")]
    WalRecordTooLarge(u32),

    #[error("wal checksum mismatch at offset {offset}")]
    WalChecksumMismatch { offset: u64 },

    #[error("wal protocol error: {0}")]
    WalProtocol(&'static str),

    #[error("storage corrupted: {0}")]
    StorageCorrupted(&'static str),
}
