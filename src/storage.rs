use crate::graph::GraphError;
use crate::page::{PageId, PAGE_SIZE};
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const WAL_MAGIC: &[u8; 4] = b"GWAL";

/// WAL 单帧头部长度：magic(4) + payload_len(4) + crc32(4)
pub const WAL_FRAME_HEADER_SIZE: u64 = 12;

/// 页级 WAL 日志记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalRecord {
    TxBegin {
        tx_id: u64,
    },
    PageWrite {
        tx_id: u64,
        page_id: PageId,
        crc32: u32,
        data: Vec<u8>,
    },
    TxCommit {
        tx_id: u64,
    },
    TxRollback {
        tx_id: u64,
    },
    Checkpoint,
}

fn payload_crc(payload: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(payload);
    hasher.finalize()
}

/// 页级 WAL 追加写入器。
///
/// 严格遵循「单数据文件 + 页级 WAL」两文件架构：它既承载事务提交的 redo 帧，
/// 也承载缓冲区溢出（STEAL）的未提交页镜像。每个 `PageWrite` 帧都可通过
/// `read_frame_at` 依偏移随机回读，因此任何被淘汰出缓冲池的物理页都能从 WAL
/// 原样恢复，无需第三个临时文件。
pub struct WalWriter {
    path: PathBuf,
    is_memory: bool,
    file: Mutex<Option<File>>,
    mem_frames: Mutex<HashMap<u64, Vec<u8>>>,
    mem_next: AtomicU64,
    /// 累计 fsync 次数（用于验证「一次 commit 恰好一次 fsync」的批量提交语义）
    fsync_count: AtomicU64,
    /// 累计写入的 WAL 帧数
    frames_written: AtomicU64,
}

impl WalWriter {
    pub fn open<P: AsRef<Path>>(path: P, is_memory: bool) -> Result<Arc<Self>, GraphError> {
        if is_memory {
            return Ok(Arc::new(Self {
                path: path.as_ref().to_path_buf(),
                is_memory: true,
                file: Mutex::new(None),
                mem_frames: Mutex::new(HashMap::new()),
                mem_next: AtomicU64::new(0),
                fsync_count: AtomicU64::new(0),
                frames_written: AtomicU64::new(0),
            }));
        }

        let file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(path.as_ref())?;

        Ok(Arc::new(Self {
            path: path.as_ref().to_path_buf(),
            is_memory: false,
            file: Mutex::new(Some(file)),
            mem_frames: Mutex::new(HashMap::new()),
            mem_next: AtomicU64::new(0),
            fsync_count: AtomicU64::new(0),
            frames_written: AtomicU64::new(0),
        }))
    }

    /// 累计 WAL fsync 次数
    pub fn fsync_count(&self) -> u64 {
        self.fsync_count.load(Ordering::Relaxed)
    }

    /// 累计写入的 WAL 帧数
    pub fn frames_written(&self) -> u64 {
        self.frames_written.load(Ordering::Relaxed)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_memory(&self) -> bool {
        self.is_memory
    }

    fn lock_file(&self) -> MutexGuard<'_, Option<File>> {
        self.file.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_mem(&self) -> MutexGuard<'_, HashMap<u64, Vec<u8>>> {
        self.mem_frames.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 追加单条 WAL 记录，返回该帧在 WAL 中的起始偏移（用于 O(1) 随机回读）
    pub fn append(&self, record: &WalRecord) -> Result<u64, GraphError> {
        let payload = bincode::serialize(record)
            .map_err(|e| GraphError::SerializationError(e.to_string()))?;
        let crc = payload_crc(&payload);

        if self.is_memory {
            let offset = self.mem_next.fetch_add(1, Ordering::SeqCst);
            self.lock_mem().insert(offset, payload);
            self.frames_written.fetch_add(1, Ordering::Relaxed);
            return Ok(offset);
        }

        let mut guard = self.lock_file();
        let file = guard
            .as_mut()
            .ok_or_else(|| GraphError::StorageError("WAL file handle unavailable".into()))?;

        let offset = file.seek(SeekFrom::End(0))?;
        file.write_all(WAL_MAGIC)?;
        file.write_all(&(payload.len() as u32).to_le_bytes())?;
        file.write_all(&crc.to_le_bytes())?;
        file.write_all(&payload)?;
        drop(guard);
        self.frames_written.fetch_add(1, Ordering::Relaxed);
        Ok(offset)
    }

    /// 依偏移随机回读并校验单条 WAL 记录
    pub fn read_frame_at(&self, offset: u64) -> Result<Option<WalRecord>, GraphError> {
        if self.is_memory {
            let payload = self.lock_mem().get(&offset).cloned();
            return match payload {
                Some(bytes) => Ok(bincode::deserialize::<WalRecord>(&bytes).ok()),
                None => Ok(None),
            };
        }

        let mut guard = self.lock_file();
        let file = guard
            .as_mut()
            .ok_or_else(|| GraphError::StorageError("WAL file handle unavailable".into()))?;

        file.seek(SeekFrom::Start(offset))?;

        let mut header = [0u8; WAL_FRAME_HEADER_SIZE as usize];
        if file.read_exact(&mut header).is_err() {
            return Ok(None);
        }
        if &header[0..4] != WAL_MAGIC {
            return Ok(None);
        }
        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let expected_crc = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);

        let mut payload = vec![0u8; len];
        if file.read_exact(&mut payload).is_err() {
            return Ok(None);
        }
        if payload_crc(&payload) != expected_crc {
            return Ok(None);
        }

        match bincode::deserialize::<WalRecord>(&payload) {
            Ok(rec) => Ok(Some(rec)),
            Err(_) => Ok(None),
        }
    }

    /// 回读指定 `PageWrite` 帧中的 4KB 物理页镜像
    pub fn read_page_image(&self, offset: u64) -> Result<Option<[u8; PAGE_SIZE]>, GraphError> {
        match self.read_frame_at(offset)? {
            Some(WalRecord::PageWrite { data, .. }) if data.len() == PAGE_SIZE => {
                let mut buf = [0u8; PAGE_SIZE];
                buf.copy_from_slice(&data);
                Ok(Some(buf))
            }
            _ => Ok(None),
        }
    }

    /// 顺序扫描全部有效帧（遇到损坏/截断半帧立即安全停止）
    pub fn read_all_frames(&self) -> Result<Vec<WalRecord>, GraphError> {
        if self.is_memory {
            let mut offsets: Vec<u64> = self.lock_mem().keys().copied().collect();
            offsets.sort_unstable();
            let mut records = Vec::with_capacity(offsets.len());
            for off in offsets {
                if let Some(rec) = self.read_frame_at(off)? {
                    records.push(rec);
                }
            }
            return Ok(records);
        }

        let mut guard = self.lock_file();
        let file = guard
            .as_mut()
            .ok_or_else(|| GraphError::StorageError("WAL file handle unavailable".into()))?;

        file.seek(SeekFrom::Start(0))?;
        let mut reader = io::BufReader::new(&mut *file);

        let mut records = Vec::new();
        let mut header = [0u8; WAL_FRAME_HEADER_SIZE as usize];

        loop {
            match reader.read_exact(&mut header) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(_) => break,
            }

            if &header[0..4] != WAL_MAGIC {
                break;
            }

            let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
            let expected_crc = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);

            let mut payload = vec![0u8; len];
            if reader.read_exact(&mut payload).is_err() {
                break;
            }
            if payload_crc(&payload) != expected_crc {
                break;
            }

            match bincode::deserialize::<WalRecord>(&payload) {
                Ok(rec) => records.push(rec),
                Err(_) => break,
            }
        }

        Ok(records)
    }

    /// 物理落盘（fsync）
    pub fn sync(&self) -> Result<(), GraphError> {
        if self.is_memory {
            return Ok(());
        }
        let mut guard = self.lock_file();
        if let Some(file) = guard.as_mut() {
            file.flush()?;
            file.sync_data()?;
        }
        drop(guard);
        self.fsync_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 截断清空 WAL
    pub fn truncate(&self) -> Result<(), GraphError> {
        if self.is_memory {
            self.lock_mem().clear();
            self.mem_next.store(0, Ordering::SeqCst);
            return Ok(());
        }
        let mut guard = self.lock_file();
        if let Some(file) = guard.as_mut() {
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            file.flush()?;
        }
        Ok(())
    }

    /// 当前 WAL 物理体积
    pub fn len(&self) -> u64 {
        if self.is_memory {
            return self.lock_mem().values().map(|v| v.len() as u64).sum();
        }
        let guard = self.lock_file();
        match guard.as_ref() {
            Some(file) => file.metadata().map(|m| m.len()).unwrap_or(0),
            None => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 页级持久化存储引擎：负责单主数据文件及 WAL 预写日志
pub struct StorageEngine {
    db_path: PathBuf,
    wal: Arc<WalWriter>,
    is_memory: bool,
}

impl StorageEngine {
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn wal_path(&self) -> &Path {
        self.wal.path()
    }

    pub fn is_memory(&self) -> bool {
        self.is_memory
    }

    pub fn wal_writer(&self) -> &Arc<WalWriter> {
        &self.wal
    }

    /// 打开存储引擎：如果存在 WAL 则回放已提交的页到主数据文件
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        let path_ref = path.as_ref();
        let is_memory = path_ref.to_str() == Some(":memory:") || path_ref.as_os_str().is_empty();

        if is_memory {
            return Ok(Self {
                db_path: PathBuf::from(":memory:"),
                wal: WalWriter::open(":memory:.wal", true)?,
                is_memory: true,
            });
        }

        let db_path = path_ref.to_path_buf();
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut wal_path_str = db_path.as_os_str().to_os_string();
        wal_path_str.push(".wal");
        let wal_path = PathBuf::from(wal_path_str);

        // 确保主数据文件存在
        if !db_path.exists() {
            let _ = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&db_path)?;
        }

        let wal = WalWriter::open(&wal_path, false)?;

        // 回放 WAL 中已提交的页（崩溃自愈）
        apply_committed_pages(&wal, &db_path)?;

        Ok(Self {
            db_path,
            wal,
            is_memory: false,
        })
    }

    /// 写入单个 WAL 记录（附带 CRC32 校验）
    pub fn append_record(&self, record: &WalRecord) -> Result<(), GraphError> {
        self.wal.append(record)?;
        Ok(())
    }

    /// 强制执行 WAL 物理落盘 (fsync)
    pub fn sync(&self) -> Result<(), GraphError> {
        self.wal.sync()
    }

    /// 将 WAL 中已提交的页物理应用到主数据文件（不完全截断 WAL）
    pub fn apply_committed_to_db(&self) -> Result<usize, GraphError> {
        if self.is_memory {
            return Ok(0);
        }
        apply_committed_pages(&self.wal, &self.db_path)
    }

    /// 执行 Checkpoint：截断清空 WAL 文件并记录 Checkpoint 标记
    pub fn checkpoint(&self) -> Result<(), GraphError> {
        if self.is_memory {
            self.wal.truncate()?;
            return Ok(());
        }
        self.wal.truncate()?;
        self.append_record(&WalRecord::Checkpoint)?;
        self.sync()?;
        // 截断后残留一个 Checkpoint 标记帧亦无副作用，但保持 WAL 干净以对齐 SQLite 语义
        self.wal.truncate()?;
        Ok(())
    }

    /// 当前 WAL 体积
    pub fn wal_size(&self) -> u64 {
        self.wal.len()
    }
}

/// 将 WAL 中所有已提交事务的物理页按序重放到主数据文件。
///
/// 未提交事务（仅有 `PageWrite` 而无 `TxCommit`）的溢出帧会被安全忽略，
/// 这正是 STEAL 策略下「主库零污染」的根本保证。
pub fn apply_committed_pages(wal: &WalWriter, db_path: &Path) -> Result<usize, GraphError> {
    let records = wal.read_all_frames()?;
    if records.is_empty() {
        return Ok(0);
    }

    let mut committed_txs: HashSet<u64> = HashSet::new();
    let mut aborted_txs: HashSet<u64> = HashSet::new();
    for record in &records {
        match record {
            WalRecord::TxCommit { tx_id } => {
                committed_txs.insert(*tx_id);
            }
            WalRecord::TxRollback { tx_id } => {
                aborted_txs.insert(*tx_id);
            }
            _ => {}
        }
    }

    if committed_txs.is_empty() {
        return Ok(0);
    }

    let mut db_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(db_path)?;

    let mut applied = 0usize;
    for record in records {
        if let WalRecord::PageWrite {
            tx_id,
            page_id,
            data,
            ..
        } = record
        {
            if committed_txs.contains(&tx_id)
                && !aborted_txs.contains(&tx_id)
                && data.len() == PAGE_SIZE
            {
                let offset = (page_id as u64) * (PAGE_SIZE as u64);
                db_file.seek(SeekFrom::Start(offset))?;
                db_file.write_all(&data)?;
                applied += 1;
            }
        }
    }

    db_file.flush()?;
    db_file.sync_all()?;

    Ok(applied)
}
