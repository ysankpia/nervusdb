use crate::pager::{PageId, Pager};
use crate::{Error, PAGE_SIZE, Result};
use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalRecord {
    BeginTx {
        txid: u64,
    },
    CommitTx {
        txid: u64,
    },
    PageWrite {
        page_id: u64,
        page: Box<[u8; PAGE_SIZE]>,
    },
    PageFree {
        page_id: u64,
    },
}

impl WalRecord {
    fn record_type(&self) -> u8 {
        match self {
            WalRecord::BeginTx { .. } => 1,
            WalRecord::CommitTx { .. } => 2,
            WalRecord::PageWrite { .. } => 3,
            WalRecord::PageFree { .. } => 4,
        }
    }

    fn encode_body(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + 8 + PAGE_SIZE);
        out.push(self.record_type());
        match self {
            WalRecord::BeginTx { txid } | WalRecord::CommitTx { txid } => {
                out.extend_from_slice(&txid.to_le_bytes());
            }
            WalRecord::PageWrite { page_id, page } => {
                out.extend_from_slice(&page_id.to_le_bytes());
                out.extend_from_slice(page.as_ref());
            }
            WalRecord::PageFree { page_id } => {
                out.extend_from_slice(&page_id.to_le_bytes());
            }
        }
        out
    }

    fn decode_body(body: &[u8]) -> Result<Self> {
        if body.is_empty() {
            return Err(Error::WalProtocol("empty record body"));
        }
        let ty = body[0];
        let payload = &body[1..];

        match ty {
            1 => {
                let txid = read_u64(payload)?;
                Ok(WalRecord::BeginTx { txid })
            }
            2 => {
                let txid = read_u64(payload)?;
                Ok(WalRecord::CommitTx { txid })
            }
            3 => {
                if payload.len() != 8 + PAGE_SIZE {
                    return Err(Error::WalProtocol("invalid PageWrite payload length"));
                }
                let page_id = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                let mut page = Box::new([0u8; PAGE_SIZE]);
                page.as_mut_slice().copy_from_slice(&payload[8..]);
                Ok(WalRecord::PageWrite { page_id, page })
            }
            4 => {
                let page_id = read_u64(payload)?;
                Ok(WalRecord::PageFree { page_id })
            }
            _ => Err(Error::WalProtocol("unknown record type")),
        }
    }
}

#[derive(Debug)]
pub struct Wal {
    path: PathBuf,
    file: File,
}

impl Wal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        Ok(Self { path, file })
    }

    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&mut self, record: &WalRecord) -> Result<u64> {
        let body = record.encode_body();
        let len = u32::try_from(body.len()).map_err(|_| Error::WalRecordTooLarge(u32::MAX))?;
        let crc = crc32(&body);

        let offset = self.file.metadata()?.len();
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&len.to_le_bytes())?;
        self.file.write_all(&crc.to_le_bytes())?;
        self.file.write_all(&body)?;
        self.file.flush()?;
        Ok(offset)
    }

    pub fn fsync(&mut self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }

    pub fn replay_into(&self, pager: &mut Pager) -> Result<ReplayStats> {
        let mut reader = WalReader::open(&self.path)?;
        let mut stats = ReplayStats::default();

        let mut current_txid: Option<u64> = None;
        let mut pending: Vec<WalRecord> = Vec::new();

        while let Some((offset, record)) = reader.next_record()? {
            stats.records += 1;

            match record {
                WalRecord::BeginTx { txid } => {
                    if current_txid.is_some() {
                        return Err(Error::WalProtocol("nested BeginTx"));
                    }
                    current_txid = Some(txid);
                    pending.clear();
                }
                WalRecord::CommitTx { txid } => {
                    if current_txid != Some(txid) {
                        return Err(Error::WalProtocol("CommitTx without matching BeginTx"));
                    }

                    for op in pending.drain(..) {
                        apply_op(pager, op)?;
                    }

                    stats.committed_txs += 1;
                    current_txid = None;
                }
                WalRecord::PageWrite { .. } | WalRecord::PageFree { .. } => {
                    if current_txid.is_none() {
                        return Err(Error::WalProtocol("page op outside tx"));
                    }
                    pending.push(record);
                }
            }

            stats.last_offset = offset;
        }

        Ok(stats)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReplayStats {
    pub records: u64,
    pub committed_txs: u64,
    pub last_offset: u64,
}

fn apply_op(pager: &mut Pager, op: WalRecord) -> Result<()> {
    match op {
        WalRecord::PageWrite { page_id, page } => {
            let pid = PageId::new(page_id);
            pager.ensure_allocated(pid)?;
            pager.write_page(pid, page.as_ref())?;
            Ok(())
        }
        WalRecord::PageFree { page_id } => pager.free_page(PageId::new(page_id)),
        WalRecord::BeginTx { .. } | WalRecord::CommitTx { .. } => {
            Err(Error::WalProtocol("unexpected tx marker inside apply_op"))
        }
    }
}

struct WalReader {
    file: File,
    offset: u64,
}

impl WalReader {
    fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().read(true).open(path)?;
        Ok(Self { file, offset: 0 })
    }

    fn next_record(&mut self) -> Result<Option<(u64, WalRecord)>> {
        let record_offset = self.offset;

        let Some(len) = self.try_read_u32()? else {
            return Ok(None);
        };

        if len > (1 + 8 + PAGE_SIZE + 8) as u32 {
            return Err(Error::WalRecordTooLarge(len));
        }

        let Some(crc) = self.try_read_u32()? else {
            return Ok(None);
        };

        let mut body = vec![0u8; len as usize];
        if let Err(e) = self.file.read_exact(&mut body) {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(Error::Io(e));
        }

        let got_crc = crc32(&body);
        if got_crc != crc {
            return Err(Error::WalChecksumMismatch {
                offset: record_offset,
            });
        }

        self.offset += 4 + 4 + len as u64;

        let record = WalRecord::decode_body(&body)?;
        Ok(Some((record_offset, record)))
    }

    fn try_read_u32(&mut self) -> Result<Option<u32>> {
        let mut buf = [0u8; 4];
        match self.file.read_exact(&mut buf) {
            Ok(()) => Ok(Some(u32::from_le_bytes(buf))),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(Error::Io(e)),
        }
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

fn read_u64(payload: &[u8]) -> Result<u64> {
    if payload.len() != 8 {
        return Err(Error::WalProtocol("invalid u64 payload length"));
    }
    Ok(u64::from_le_bytes(payload.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn replay_applies_only_committed_tx() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("test.ndb");
        let wal_path = dir.path().join("test.wal");

        {
            let _pager = Pager::open(&ndb).unwrap();
            let mut wal = Wal::open(&wal_path).unwrap();

            let mut page = [0u8; PAGE_SIZE];
            page[0] = 0x11;
            page[PAGE_SIZE - 1] = 0x22;

            wal.append(&WalRecord::BeginTx { txid: 1 }).unwrap();
            wal.append(&WalRecord::PageWrite {
                page_id: 2,
                page: Box::new(page),
            })
            .unwrap();
            wal.append(&WalRecord::CommitTx { txid: 1 }).unwrap();
            wal.fsync().unwrap();
        }

        let mut pager = Pager::open(&ndb).unwrap();
        let wal = Wal::open(&wal_path).unwrap();
        let stats = wal.replay_into(&mut pager).unwrap();
        assert_eq!(stats.committed_txs, 1);

        let page = pager.read_page(PageId::new(2)).unwrap();
        assert_eq!(page[0], 0x11);
        assert_eq!(page[PAGE_SIZE - 1], 0x22);
    }

    #[test]
    fn replay_ignores_uncommitted_tx() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("test.ndb");
        let wal_path = dir.path().join("test.wal");

        {
            let _pager = Pager::open(&ndb).unwrap();
            let mut wal = Wal::open(&wal_path).unwrap();

            let mut page = [0u8; PAGE_SIZE];
            page[0] = 0xAA;

            wal.append(&WalRecord::BeginTx { txid: 1 }).unwrap();
            wal.append(&WalRecord::PageWrite {
                page_id: 2,
                page: Box::new(page),
            })
            .unwrap();
            wal.fsync().unwrap();
        }

        let mut pager = Pager::open(&ndb).unwrap();
        let wal = Wal::open(&wal_path).unwrap();
        let stats = wal.replay_into(&mut pager).unwrap();
        assert_eq!(stats.committed_txs, 0);

        assert!(pager.read_page(PageId::new(2)).is_err());
    }

    #[test]
    fn replay_ignores_trailing_partial_record() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("test.ndb");
        let wal_path = dir.path().join("test.wal");

        {
            let _pager = Pager::open(&ndb).unwrap();
            let mut wal = Wal::open(&wal_path).unwrap();

            let mut page = [0u8; PAGE_SIZE];
            page[0] = 0x7F;

            wal.append(&WalRecord::BeginTx { txid: 1 }).unwrap();
            wal.append(&WalRecord::PageWrite {
                page_id: 2,
                page: Box::new(page),
            })
            .unwrap();
            wal.append(&WalRecord::CommitTx { txid: 1 }).unwrap();

            let mut file = OpenOptions::new().append(true).open(&wal_path).unwrap();
            file.write_all(&[0x01, 0x02]).unwrap();
        }

        let mut pager = Pager::open(&ndb).unwrap();
        let wal = Wal::open(&wal_path).unwrap();
        let stats = wal.replay_into(&mut pager).unwrap();
        assert_eq!(stats.committed_txs, 1);

        let page = pager.read_page(PageId::new(2)).unwrap();
        assert_eq!(page[0], 0x7F);
    }
}
