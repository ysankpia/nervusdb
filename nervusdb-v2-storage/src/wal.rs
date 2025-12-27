use crate::pager::{PageId, Pager};
use crate::property::PropertyValue;
use crate::{Error, PAGE_SIZE, Result};
use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
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
    CreateNode {
        external_id: u64,
        label_id: u32,
        internal_id: u32,
    },
    CreateEdge {
        src: u32,
        rel: u32,
        dst: u32,
    },
    TombstoneNode {
        node: u32,
    },
    TombstoneEdge {
        src: u32,
        rel: u32,
        dst: u32,
    },
    ManifestSwitch {
        epoch: u64,
        segments: Vec<SegmentPointer>,
    },
    Checkpoint {
        up_to_txid: u64,
        epoch: u64,
    },
    SetNodeProperty {
        node: u32,
        key: String,
        value: PropertyValue,
    },
    SetEdgeProperty {
        src: u32,
        rel: u32,
        dst: u32,
        key: String,
        value: PropertyValue,
    },
    RemoveNodeProperty {
        node: u32,
        key: String,
    },
    RemoveEdgeProperty {
        src: u32,
        rel: u32,
        dst: u32,
        key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentPointer {
    pub id: u64,
    pub meta_page_id: u64,
}

impl WalRecord {
    fn record_type(&self) -> u8 {
        match self {
            WalRecord::BeginTx { .. } => 1,
            WalRecord::CommitTx { .. } => 2,
            WalRecord::PageWrite { .. } => 3,
            WalRecord::PageFree { .. } => 4,
            WalRecord::CreateNode { .. } => 5,
            WalRecord::CreateEdge { .. } => 6,
            WalRecord::TombstoneNode { .. } => 7,
            WalRecord::TombstoneEdge { .. } => 8,
            WalRecord::ManifestSwitch { .. } => 9,
            WalRecord::Checkpoint { .. } => 10,
            WalRecord::SetNodeProperty { .. } => 11,
            WalRecord::SetEdgeProperty { .. } => 12,
            WalRecord::RemoveNodeProperty { .. } => 13,
            WalRecord::RemoveEdgeProperty { .. } => 14,
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
            WalRecord::CreateNode {
                external_id,
                label_id,
                internal_id,
            } => {
                out.extend_from_slice(&external_id.to_le_bytes());
                out.extend_from_slice(&label_id.to_le_bytes());
                out.extend_from_slice(&internal_id.to_le_bytes());
            }
            WalRecord::CreateEdge { src, rel, dst }
            | WalRecord::TombstoneEdge { src, rel, dst } => {
                out.extend_from_slice(&src.to_le_bytes());
                out.extend_from_slice(&rel.to_le_bytes());
                out.extend_from_slice(&dst.to_le_bytes());
            }
            WalRecord::TombstoneNode { node } => {
                out.extend_from_slice(&node.to_le_bytes());
            }
            WalRecord::ManifestSwitch { epoch, segments } => {
                out.extend_from_slice(&epoch.to_le_bytes());
                let count: u32 = segments
                    .len()
                    .try_into()
                    .map_err(|_| Error::WalProtocol("too many segments"))
                    .unwrap();
                out.extend_from_slice(&count.to_le_bytes());
                for seg in segments {
                    out.extend_from_slice(&seg.id.to_le_bytes());
                    out.extend_from_slice(&seg.meta_page_id.to_le_bytes());
                }
            }
            WalRecord::Checkpoint { up_to_txid, epoch } => {
                out.extend_from_slice(&up_to_txid.to_le_bytes());
                out.extend_from_slice(&epoch.to_le_bytes());
            }
            WalRecord::SetNodeProperty { node, key, value } => {
                out.extend_from_slice(&node.to_le_bytes());
                let key_bytes = key.as_bytes();
                let key_len = u32::try_from(key_bytes.len())
                    .unwrap_or_else(|_| panic!("key too long: {} bytes", key_bytes.len()));
                out.extend_from_slice(&key_len.to_le_bytes());
                out.extend_from_slice(key_bytes);
                let value_bytes = value.encode();
                out.extend_from_slice(&value_bytes);
            }
            WalRecord::SetEdgeProperty {
                src,
                rel,
                dst,
                key,
                value,
            } => {
                out.extend_from_slice(&src.to_le_bytes());
                out.extend_from_slice(&rel.to_le_bytes());
                out.extend_from_slice(&dst.to_le_bytes());
                let key_bytes = key.as_bytes();
                let key_len = u32::try_from(key_bytes.len())
                    .unwrap_or_else(|_| panic!("key too long: {} bytes", key_bytes.len()));
                out.extend_from_slice(&key_len.to_le_bytes());
                out.extend_from_slice(key_bytes);
                let value_bytes = value.encode();
                out.extend_from_slice(&value_bytes);
            }
            WalRecord::RemoveNodeProperty { node, key } => {
                out.extend_from_slice(&node.to_le_bytes());
                let key_bytes = key.as_bytes();
                let key_len = u32::try_from(key_bytes.len())
                    .unwrap_or_else(|_| panic!("key too long: {} bytes", key_bytes.len()));
                out.extend_from_slice(&key_len.to_le_bytes());
                out.extend_from_slice(key_bytes);
            }
            WalRecord::RemoveEdgeProperty { src, rel, dst, key } => {
                out.extend_from_slice(&src.to_le_bytes());
                out.extend_from_slice(&rel.to_le_bytes());
                out.extend_from_slice(&dst.to_le_bytes());
                let key_bytes = key.as_bytes();
                let key_len = u32::try_from(key_bytes.len())
                    .unwrap_or_else(|_| panic!("key too long: {} bytes", key_bytes.len()));
                out.extend_from_slice(&key_len.to_le_bytes());
                out.extend_from_slice(key_bytes);
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
            5 => {
                if payload.len() != 8 + 4 + 4 {
                    return Err(Error::WalProtocol("invalid CreateNode payload length"));
                }
                let external_id = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                let label_id = u32::from_le_bytes(payload[8..12].try_into().unwrap());
                let internal_id = u32::from_le_bytes(payload[12..16].try_into().unwrap());
                Ok(WalRecord::CreateNode {
                    external_id,
                    label_id,
                    internal_id,
                })
            }
            6 => {
                if payload.len() != 12 {
                    return Err(Error::WalProtocol("invalid CreateEdge payload length"));
                }
                let src = u32::from_le_bytes(payload[0..4].try_into().unwrap());
                let rel = u32::from_le_bytes(payload[4..8].try_into().unwrap());
                let dst = u32::from_le_bytes(payload[8..12].try_into().unwrap());
                Ok(WalRecord::CreateEdge { src, rel, dst })
            }
            7 => {
                if payload.len() != 4 {
                    return Err(Error::WalProtocol("invalid TombstoneNode payload length"));
                }
                let node = u32::from_le_bytes(payload[0..4].try_into().unwrap());
                Ok(WalRecord::TombstoneNode { node })
            }
            8 => {
                if payload.len() != 12 {
                    return Err(Error::WalProtocol("invalid TombstoneEdge payload length"));
                }
                let src = u32::from_le_bytes(payload[0..4].try_into().unwrap());
                let rel = u32::from_le_bytes(payload[4..8].try_into().unwrap());
                let dst = u32::from_le_bytes(payload[8..12].try_into().unwrap());
                Ok(WalRecord::TombstoneEdge { src, rel, dst })
            }
            9 => {
                if payload.len() < 8 + 4 {
                    return Err(Error::WalProtocol("invalid ManifestSwitch payload length"));
                }
                let epoch = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                let count = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
                let mut offset = 12;
                let expected = 12 + count * 16;
                if payload.len() != expected {
                    return Err(Error::WalProtocol("invalid ManifestSwitch payload length"));
                }
                let mut segments = Vec::with_capacity(count);
                for _ in 0..count {
                    let id = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
                    let meta_page_id =
                        u64::from_le_bytes(payload[offset + 8..offset + 16].try_into().unwrap());
                    segments.push(SegmentPointer { id, meta_page_id });
                    offset += 16;
                }
                Ok(WalRecord::ManifestSwitch { epoch, segments })
            }
            10 => {
                if payload.len() != 16 {
                    return Err(Error::WalProtocol("invalid Checkpoint payload length"));
                }
                let up_to_txid = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                let epoch = u64::from_le_bytes(payload[8..16].try_into().unwrap());
                Ok(WalRecord::Checkpoint { up_to_txid, epoch })
            }
            11 => {
                // SetNodeProperty: [node: u32][key_len: u32][key: bytes][value: encoded]
                if payload.len() < 4 + 4 {
                    return Err(Error::WalProtocol("invalid SetNodeProperty payload length"));
                }
                let node = u32::from_le_bytes(payload[0..4].try_into().unwrap());
                let key_len = u32::from_le_bytes(payload[4..8].try_into().unwrap()) as usize;
                if payload.len() < 8 + key_len {
                    return Err(Error::WalProtocol("invalid SetNodeProperty payload length"));
                }
                let key = String::from_utf8(payload[8..8 + key_len].to_vec())
                    .map_err(|_| Error::WalProtocol("invalid UTF-8 in key"))?;
                let value_bytes = &payload[8 + key_len..];
                let value = PropertyValue::decode(value_bytes)
                    .map_err(|_| Error::WalProtocol("property decode error"))?;
                Ok(WalRecord::SetNodeProperty { node, key, value })
            }
            12 => {
                // SetEdgeProperty: [src: u32][rel: u32][dst: u32][key_len: u32][key: bytes][value: encoded]
                if payload.len() < 12 + 4 {
                    return Err(Error::WalProtocol("invalid SetEdgeProperty payload length"));
                }
                let src = u32::from_le_bytes(payload[0..4].try_into().unwrap());
                let rel = u32::from_le_bytes(payload[4..8].try_into().unwrap());
                let dst = u32::from_le_bytes(payload[8..12].try_into().unwrap());
                let key_len = u32::from_le_bytes(payload[12..16].try_into().unwrap()) as usize;
                if payload.len() < 16 + key_len {
                    return Err(Error::WalProtocol("invalid SetEdgeProperty payload length"));
                }
                let key = String::from_utf8(payload[16..16 + key_len].to_vec())
                    .map_err(|_| Error::WalProtocol("invalid UTF-8 in key"))?;
                let value_bytes = &payload[16 + key_len..];
                let value = PropertyValue::decode(value_bytes)
                    .map_err(|_| Error::WalProtocol("property decode error"))?;
                Ok(WalRecord::SetEdgeProperty {
                    src,
                    rel,
                    dst,
                    key,
                    value,
                })
            }
            13 => {
                // RemoveNodeProperty: [node: u32][key_len: u32][key: bytes]
                if payload.len() < 4 + 4 {
                    return Err(Error::WalProtocol(
                        "invalid RemoveNodeProperty payload length",
                    ));
                }
                let node = u32::from_le_bytes(payload[0..4].try_into().unwrap());
                let key_len = u32::from_le_bytes(payload[4..8].try_into().unwrap()) as usize;
                if payload.len() != 8 + key_len {
                    return Err(Error::WalProtocol(
                        "invalid RemoveNodeProperty payload length",
                    ));
                }
                let key = String::from_utf8(payload[8..8 + key_len].to_vec())
                    .map_err(|_| Error::WalProtocol("invalid UTF-8 in key"))?;
                Ok(WalRecord::RemoveNodeProperty { node, key })
            }
            14 => {
                // RemoveEdgeProperty: [src: u32][rel: u32][dst: u32][key_len: u32][key: bytes]
                if payload.len() < 12 + 4 {
                    return Err(Error::WalProtocol(
                        "invalid RemoveEdgeProperty payload length",
                    ));
                }
                let src = u32::from_le_bytes(payload[0..4].try_into().unwrap());
                let rel = u32::from_le_bytes(payload[4..8].try_into().unwrap());
                let dst = u32::from_le_bytes(payload[8..12].try_into().unwrap());
                let key_len = u32::from_le_bytes(payload[12..16].try_into().unwrap()) as usize;
                if payload.len() != 16 + key_len {
                    return Err(Error::WalProtocol(
                        "invalid RemoveEdgeProperty payload length",
                    ));
                }
                let key = String::from_utf8(payload[16..16 + key_len].to_vec())
                    .map_err(|_| Error::WalProtocol("invalid UTF-8 in key"))?;
                Ok(WalRecord::RemoveEdgeProperty { src, rel, dst, key })
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
                _ => return Err(Error::WalProtocol("non-page wal record in replay_into")),
            }

            stats.last_offset = offset;
        }

        Ok(stats)
    }

    pub fn replay_committed(&self) -> Result<Vec<CommittedTx>> {
        let mut reader = WalReader::open(&self.path)?;
        let mut out: Vec<CommittedTx> = Vec::new();

        let mut current_txid: Option<u64> = None;
        let mut pending: Vec<WalRecord> = Vec::new();

        while let Some((_offset, record)) = reader.next_record()? {
            match record {
                WalRecord::BeginTx { txid } => {
                    current_txid = Some(txid);
                    pending.clear();
                }
                WalRecord::CommitTx { txid } => {
                    if current_txid != Some(txid) {
                        return Err(Error::WalProtocol("CommitTx without matching BeginTx"));
                    }
                    out.push(CommittedTx {
                        txid,
                        ops: std::mem::take(&mut pending),
                    });
                    current_txid = None;
                }
                other => {
                    if current_txid.is_none() {
                        return Err(Error::WalProtocol("op outside tx"));
                    }
                    pending.push(other);
                }
            }
        }

        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct CommittedTx {
    pub txid: u64,
    pub ops: Vec<WalRecord>,
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
        _ => Err(Error::WalProtocol("unexpected wal record in apply_op")),
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

        const MAX_WAL_RECORD_LEN: u32 = 1024 * 1024; // 1MB
        if len > MAX_WAL_RECORD_LEN {
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
            // In crash scenarios, it's acceptable to have a torn final record.
            // Treat CRC mismatch as end-of-log and ignore the tail.
            return Ok(None);
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

    #[test]
    fn replay_committed_tolerates_aborted_tx_followed_by_new_begin() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        {
            let mut wal = Wal::open(&wal_path).unwrap();

            wal.append(&WalRecord::BeginTx { txid: 1 }).unwrap();
            wal.append(&WalRecord::CreateEdge {
                src: 1,
                rel: 1,
                dst: 2,
            })
            .unwrap();
            wal.fsync().unwrap();

            // Simulate a crash: txid=1 never commits, but later we see a new BeginTx.
            wal.append(&WalRecord::BeginTx { txid: 2 }).unwrap();
            wal.append(&WalRecord::CreateEdge {
                src: 2,
                rel: 1,
                dst: 3,
            })
            .unwrap();
            wal.append(&WalRecord::CommitTx { txid: 2 }).unwrap();
            wal.fsync().unwrap();
        }

        let wal = Wal::open(&wal_path).unwrap();
        let txs = wal.replay_committed().unwrap();
        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0].txid, 2);
        assert!(txs[0].ops.iter().any(|op| matches!(
            op,
            WalRecord::CreateEdge {
                src: 2,
                rel: 1,
                dst: 3
            }
        )));
    }

    #[test]
    fn replay_ignores_trailing_crc_mismatch() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("test.ndb");
        let wal_path = dir.path().join("test.wal");

        let begin2_offset;
        {
            let _pager = Pager::open(&ndb).unwrap();
            let mut wal = Wal::open(&wal_path).unwrap();

            let mut page = [0u8; PAGE_SIZE];
            page[0] = 0xCC;

            wal.append(&WalRecord::BeginTx { txid: 1 }).unwrap();
            wal.append(&WalRecord::PageWrite {
                page_id: 2,
                page: Box::new(page),
            })
            .unwrap();
            wal.append(&WalRecord::CommitTx { txid: 1 }).unwrap();

            begin2_offset = wal.append(&WalRecord::BeginTx { txid: 2 }).unwrap();
            wal.fsync().unwrap();
        }

        // Corrupt the CRC field of the last record.
        {
            let mut file = OpenOptions::new().write(true).open(&wal_path).unwrap();
            file.seek(SeekFrom::Start(begin2_offset + 4)).unwrap();
            file.write_all(&0u32.to_le_bytes()).unwrap();
            file.flush().unwrap();
        }

        let mut pager = Pager::open(&ndb).unwrap();
        let wal = Wal::open(&wal_path).unwrap();
        let stats = wal.replay_into(&mut pager).unwrap();
        assert_eq!(stats.committed_txs, 1);

        let page = pager.read_page(PageId::new(2)).unwrap();
        assert_eq!(page[0], 0xCC);
    }
}
