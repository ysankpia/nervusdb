//! Minimal Write-Ahead Log with on-disk durability guarantees.

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::triple::Triple;

#[derive(Debug, Clone, Copy)]
pub enum WalRecordType {
    AddTriple,
}

impl WalRecordType {
    const fn to_byte(self) -> u8 {
        match self {
            WalRecordType::AddTriple => 1,
        }
    }
}

#[derive(Debug)]
pub struct WalEntry {
    pub record_type: WalRecordType,
    pub triple: Triple,
}

#[derive(Debug)]
pub struct WriteAheadLog {
    path: PathBuf,
    buffer: Vec<WalEntry>,
}

impl WriteAheadLog {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(Self {
            path: path.as_ref().to_owned(),
            buffer: Vec::new(),
        })
    }

    pub fn append(&mut self, entry: WalEntry) {
        self.buffer.push(entry);
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let mut writer = BufWriter::new(file);
        for entry in &self.buffer {
            writer.write_all(&encode_entry(entry))?;
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;

        self.buffer.clear();
        Ok(())
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

fn encode_entry(entry: &WalEntry) -> [u8; 25] {
    let mut buf = [0u8; 25];
    buf[0] = entry.record_type.to_byte();
    buf[1..].copy_from_slice(&entry.triple.to_bytes());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn flush_writes_entries_to_disk() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("wal.log");
        let mut wal = WriteAheadLog::open(wal_path.clone()).unwrap();

        let triple = Triple::new(1, 2, 3);
        wal.append(WalEntry {
            record_type: WalRecordType::AddTriple,
            triple,
        });

        wal.flush().unwrap();

        let bytes = std::fs::read(&wal_path).unwrap();
        assert_eq!(bytes.len(), 25);
        assert_eq!(bytes[0], WalRecordType::AddTriple.to_byte());

        let mut triple_bytes = [0u8; 24];
        triple_bytes.copy_from_slice(&bytes[1..25]);
        assert_eq!(Triple::from_bytes(triple_bytes), triple);
    }

    #[test]
    fn flush_appends_multiple_batches() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("nested").join("wal.log");
        let mut wal = WriteAheadLog::open(wal_path.clone()).unwrap();

        wal.append(WalEntry {
            record_type: WalRecordType::AddTriple,
            triple: Triple::new(1, 1, 1),
        });
        wal.flush().unwrap();

        wal.append(WalEntry {
            record_type: WalRecordType::AddTriple,
            triple: Triple::new(2, 2, 2),
        });
        wal.flush().unwrap();

        let bytes = std::fs::read(&wal_path).unwrap();
        assert_eq!(bytes.len(), 25 * 2);
    }
}
