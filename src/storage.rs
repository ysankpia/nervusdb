use crate::graph::GraphError;
use crate::page::{PageId, PAGE_SIZE};
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const WAL_MAGIC: &[u8; 4] = b"GWAL";

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

/// 页级持久化存储引擎：负责单主数据文件及 WAL 预写日志
pub struct StorageEngine {
    db_path: PathBuf,
    wal_path: PathBuf,
    wal_file: Option<File>,
    is_memory: bool,
}

impl StorageEngine {
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn wal_path(&self) -> &Path {
        &self.wal_path
    }

    pub fn is_memory(&self) -> bool {
        self.is_memory
    }

    /// 打开存储引擎：如果存在 WAL 则回放已提交的页到主数据文件
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        let path_ref = path.as_ref();
        let is_memory = path_ref.to_str() == Some(":memory:") || path_ref.as_os_str().is_empty();

        if is_memory {
            return Ok(Self {
                db_path: PathBuf::from(":memory:"),
                wal_path: PathBuf::from(":memory:.wal"),
                wal_file: None,
                is_memory: true,
            });
        }

        let db_path = path_ref.to_path_buf();
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
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

        // 回放 WAL 中已提交的页
        if wal_path.exists() {
            Self::replay_wal(&wal_path, &db_path)?;
        }

        // 打开 WAL 文件以供后续追加写入
        let wal_file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(&wal_path)?;

        Ok(Self {
            db_path,
            wal_path,
            wal_file: Some(wal_file),
            is_memory: false,
        })
    }

    /// 写入单个 WAL 记录并附带 CRC32 校验
    pub fn append_record(&mut self, record: &WalRecord) -> Result<(), GraphError> {
        if self.is_memory {
            return Ok(());
        }
        let wal_file = match self.wal_file.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };

        let payload = bincode::serialize(record)
            .map_err(|e| GraphError::SerializationError(e.to_string()))?;

        let mut hasher = Hasher::new();
        hasher.update(&payload);
        let crc = hasher.finalize();

        let len = payload.len() as u32;

        wal_file.write_all(WAL_MAGIC)?;
        wal_file.write_all(&len.to_le_bytes())?;
        wal_file.write_all(&crc.to_le_bytes())?;
        wal_file.write_all(&payload)?;
        wal_file.flush()?;

        Ok(())
    }

    /// 批量写入 WAL 记录（用于事务 commit 时的原子批量刷盘）
    pub fn append_records(&mut self, records: &[WalRecord]) -> Result<(), GraphError> {
        if self.is_memory {
            return Ok(());
        }
        let wal_file = match self.wal_file.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };

        for record in records {
            let payload = bincode::serialize(record)
                .map_err(|e| GraphError::SerializationError(e.to_string()))?;

            let mut hasher = Hasher::new();
            hasher.update(&payload);
            let crc = hasher.finalize();

            let len = payload.len() as u32;

            wal_file.write_all(WAL_MAGIC)?;
            wal_file.write_all(&len.to_le_bytes())?;
            wal_file.write_all(&crc.to_le_bytes())?;
            wal_file.write_all(&payload)?;
        }
        wal_file.flush()?;
        wal_file.sync_data()?;

        Ok(())
    }

    /// 强制执行 WAL 物理落盘 (fsync)
    pub fn sync(&mut self) -> Result<(), GraphError> {
        if self.is_memory {
            return Ok(());
        }
        if let Some(ref mut wal_file) = self.wal_file {
            wal_file.flush()?;
            wal_file.sync_data()?;
        }
        Ok(())
    }

    /// 执行 Checkpoint：截断清空 WAL 文件并记录 Checkpoint 标记
    pub fn checkpoint(&mut self) -> Result<(), GraphError> {
        if self.is_memory {
            return Ok(());
        }
        if let Some(ref mut wal_file) = self.wal_file {
            wal_file.set_len(0)?;
            wal_file.seek(SeekFrom::Start(0))?;
        }
        self.append_record(&WalRecord::Checkpoint)?;
        self.sync()?;
        Ok(())
    }

    /// 回放 WAL：校验帧 CRC，将有效已提交事务中的物理页直接重放到主数据文件中
    fn replay_wal(wal_path: &Path, db_path: &Path) -> Result<(), GraphError> {
        let file = match File::open(wal_path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        let file_len = file.metadata()?.len();
        if file_len == 0 {
            return Ok(());
        }

        let mut reader = io::BufReader::new(file);

        let mut raw_records = Vec::new();
        let mut magic_buf = [0u8; 4];
        let mut len_buf = [0u8; 4];
        let mut crc_buf = [0u8; 4];

        loop {
            match reader.read_exact(&mut magic_buf) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            if &magic_buf != WAL_MAGIC {
                break;
            }

            if reader.read_exact(&mut len_buf).is_err() {
                break;
            }
            let len = u32::from_le_bytes(len_buf) as usize;

            if reader.read_exact(&mut crc_buf).is_err() {
                break;
            }
            let expected_crc = u32::from_le_bytes(crc_buf);

            let mut payload = vec![0u8; len];
            if reader.read_exact(&mut payload).is_err() {
                break;
            }

            let mut hasher = Hasher::new();
            hasher.update(&payload);
            if hasher.finalize() != expected_crc {
                // CRC 校验失败，断电半帧截断
                break;
            }

            if let Ok(record) = bincode::deserialize::<WalRecord>(&payload) {
                raw_records.push(record);
            } else {
                break;
            }
        }

        // 收集已提交事务
        let mut committed_txs = HashSet::new();
        let mut aborted_txs = HashSet::new();

        for record in &raw_records {
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

        // 打开主数据文件进行物理页写入重放
        let mut db_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(db_path)?;

        for record in raw_records {
            if let WalRecord::PageWrite {
                tx_id,
                page_id,
                crc32: _,
                data,
            } = record
            {
                if committed_txs.contains(&tx_id) && !aborted_txs.contains(&tx_id) {
                    let offset = (page_id as u64) * (PAGE_SIZE as u64);
                    db_file.seek(SeekFrom::Start(offset))?;
                    db_file.write_all(&data)?;
                }
            }
        }

        db_file.flush()?;
        db_file.sync_all()?;

        Ok(())
    }
}
