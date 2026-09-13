use crate::crc32::Hasher;
use crate::graph::GraphError;
use crate::page::{PageId, PAGE_SIZE};
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
#[derive(Debug, Clone, PartialEq)]
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
    /// 显式事务的**动作溢出帧**：动作队列超出上限时，把动作写到 WAL 并只保留
    /// 位置索引，使单个事务可以大于内存上限而不放弃回滚。
    ///
    /// 与 `PageWrite` 同性质：它属于某个 `tx_id`，**只有该事务提交后**才被视为
    /// 权威数据。回放（第 2 遍）只处理 `PageWrite`，因此本记录天然不会被写进
    /// 主数据文件——它描述的是「尚未施加的动作」，不是「页的新内容」。
    ActionWrite {
        tx_id: u64,
        /// 该动作在所属事务内的序号（从 0 起）。**必须单调递增且连续**：
        /// 提交时按序号重排，以保证「先 AddNode 后 AddEdge」的顺序不被打破。
        seq: u64,
        /// `TxAction::encode()` 的字节
        action: Vec<u8>,
    },
    Checkpoint,
}

/// `WalRecord` 的变体标签。**这些数值是磁盘格式的一部分，永不重用。**
///
/// 与 `bincode` 的变体序号不同，这里是显式且文档化的：新增变体只能追加新标签，
/// 已废弃的标签必须保留占位，否则旧 WAL 文件会被解码成错误的记录类型。
pub mod wal_tag {
    pub const TX_BEGIN: u8 = 1;
    pub const PAGE_WRITE: u8 = 2;
    pub const TX_COMMIT: u8 = 3;
    pub const TX_ROLLBACK: u8 = 4;
    pub const CHECKPOINT: u8 = 5;
    /// 动作溢出帧（事务动作队列溢出到 WAL）。追加式新增，不改动已有标签含义。
    pub const ACTION_WRITE: u8 = 6;
}

impl WalRecord {
    /// 编码为 WAL 帧载荷。
    ///
    /// 布局（小端，见 `FORMAT.md`）：
    ///
    /// ```text
    /// tag:u8
    /// TxBegin     -> tx_id:u64
    /// TxCommit    -> tx_id:u64
    /// TxRollback  -> tx_id:u64
    /// Checkpoint  -> (无载荷)
    /// PageWrite   -> tx_id:u64, page_id:u32, crc32:u32, data:len:u32 + bytes
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut w = crate::codec::Writer::with_capacity(PAGE_SIZE + 16);
        match self {
            WalRecord::TxBegin { tx_id } => {
                w.u8(wal_tag::TX_BEGIN);
                w.u64(*tx_id);
            }
            WalRecord::TxCommit { tx_id } => {
                w.u8(wal_tag::TX_COMMIT);
                w.u64(*tx_id);
            }
            WalRecord::TxRollback { tx_id } => {
                w.u8(wal_tag::TX_ROLLBACK);
                w.u64(*tx_id);
            }
            WalRecord::Checkpoint => {
                w.u8(wal_tag::CHECKPOINT);
            }
            WalRecord::ActionWrite { tx_id, seq, action } => {
                w.u8(wal_tag::ACTION_WRITE);
                w.u64(*tx_id);
                w.u64(*seq);
                w.bytes(action);
            }
            WalRecord::PageWrite {
                tx_id,
                page_id,
                crc32,
                data,
            } => {
                w.u8(wal_tag::PAGE_WRITE);
                w.u64(*tx_id);
                w.u32(*page_id);
                w.u32(*crc32);
                w.bytes(data);
            }
        }
        w.finish()
    }

    /// 从 WAL 帧载荷解码。任何畸形输入都返回 `Err`，绝不 panic。
    ///
    /// 调用方（`read_frame_at` 等）把 `Err` 视为「这一帧无效」并停止回放，
    /// 与尾部撕裂帧的处理一致。
    pub fn decode(buf: &[u8]) -> Result<WalRecord, GraphError> {
        let mut r = crate::codec::Reader::new(buf);
        let tag = r.u8()?;
        let rec = match tag {
            wal_tag::TX_BEGIN => WalRecord::TxBegin { tx_id: r.u64()? },
            wal_tag::TX_COMMIT => WalRecord::TxCommit { tx_id: r.u64()? },
            wal_tag::TX_ROLLBACK => WalRecord::TxRollback { tx_id: r.u64()? },
            wal_tag::CHECKPOINT => WalRecord::Checkpoint,
            wal_tag::ACTION_WRITE => {
                let tx_id = r.u64()?;
                let seq = r.u64()?;
                let action = r.bytes()?;
                WalRecord::ActionWrite { tx_id, seq, action }
            }
            wal_tag::PAGE_WRITE => {
                let tx_id = r.u64()?;
                let page_id = r.u32()?;
                let crc32 = r.u32()?;
                let data = r.bytes()?;
                WalRecord::PageWrite {
                    tx_id,
                    page_id,
                    crc32,
                    data,
                }
            }
            other => {
                return Err(GraphError::SerializationError(format!(
                    "unknown WAL record tag {}",
                    other
                )))
            }
        };
        // 拒绝尾部垃圾：编码器从不留多余字节，有残余说明数据被篡改或损坏。
        if !r.is_exhausted() {
            return Err(GraphError::SerializationError(format!(
                "WAL record tag {} has {} trailing byte(s)",
                tag,
                r.remaining()
            )));
        }
        Ok(rec)
    }
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
        let payload = record.encode();
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
                Some(bytes) => Ok(WalRecord::decode(&bytes).ok()),
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

        match WalRecord::decode(&payload) {
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

    /// 打开一个顺序扫描 WAL 的流式游标。
    ///
    /// 与 [`WalWriter::read_all_frames`] 的关键区别：游标一次只持有一帧的载荷，
    /// 因此峰值内存是 O(1)（外加一个复用缓冲）而不是 O(WAL 体积)。回放 1.5GB 的
    /// WAL 不会再把 1.5GB 拉进内存。
    ///
    /// 文件模式下游标持有 `try_clone` 得到的**独立 fd**，因此流式扫描期间不会
    /// 阻塞 `append`（后者每次写入前都会 seek 到文件末尾）。`:memory:` 模式只保留
    /// 排序后的帧偏移（8 字节/帧），载荷按需查表，同样避免整表拷贝。
    pub fn cursor(&self) -> Result<WalCursor<'_>, GraphError> {
        if self.is_memory {
            let mut offsets: Vec<u64> = self.lock_mem().keys().copied().collect();
            offsets.sort_unstable();
            return Ok(WalCursor::Memory {
                wal: self,
                offsets,
                idx: 0,
                payload: Vec::new(),
            });
        }

        let guard = self.lock_file();
        let file = guard
            .as_ref()
            .ok_or_else(|| GraphError::StorageError("WAL file handle unavailable".into()))?
            .try_clone()?;
        drop(guard);

        let mut reader = io::BufReader::new(file);
        reader.seek(SeekFrom::Start(0))?;

        Ok(WalCursor::File {
            reader,
            offset: 0,
            payload: Vec::new(),
        })
    }

    /// 顺序扫描全部有效帧（遇到损坏/截断半帧立即安全停止）
    ///
    /// 注意：该函数会把整个 WAL 载入内存，只适合小日志或诊断用途。
    /// 回放路径请使用 [`WalWriter::cursor`]，它是流式的。
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

            match WalRecord::decode(&payload) {
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

/// WAL 的顺序流式游标。
///
/// 一次只持有一帧载荷，因此峰值内存与 WAL 体积无关。遇到截断、CRC 失败或
/// 魔数不匹配时返回 `Ok(None)` 安全停止——尾部残帧是正常现象（崩溃时留下），
/// 不是错误。
pub enum WalCursor<'a> {
    /// 文件模式：持独立 fd，顺序读取
    File {
        reader: io::BufReader<File>,
        offset: u64,
        /// 复用缓冲，避免每帧一次分配
        payload: Vec<u8>,
    },
    /// `:memory:` 模式：只保留排序后的偏移表（8 字节/帧），载荷按需取
    Memory {
        wal: &'a WalWriter,
        offsets: Vec<u64>,
        idx: usize,
        payload: Vec<u8>,
    },
}

impl WalCursor<'_> {
    /// 读取下一帧，返回 `(帧起始偏移, 记录)`；遍历结束或遇到残帧时返回 `Ok(None)`。
    pub fn next_frame(&mut self) -> Result<Option<(u64, WalRecord)>, GraphError> {
        match self {
            WalCursor::File {
                reader,
                offset,
                payload,
            } => {
                let mut header = [0u8; WAL_FRAME_HEADER_SIZE as usize];
                match reader.read_exact(&mut header) {
                    Ok(()) => {}
                    Err(_) => return Ok(None), // 正常结束或尾部残帧
                }
                if &header[0..4] != WAL_MAGIC {
                    return Ok(None);
                }
                let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
                let expected_crc =
                    u32::from_le_bytes([header[8], header[9], header[10], header[11]]);

                // 复用缓冲：仅在需要更大空间时扩容，之后不再分配
                if payload.len() < len {
                    payload.resize(len, 0);
                }
                if reader.read_exact(&mut payload[..len]).is_err() {
                    return Ok(None);
                }
                if payload_crc(&payload[..len]) != expected_crc {
                    return Ok(None);
                }

                let frame_offset = *offset;
                *offset += WAL_FRAME_HEADER_SIZE + len as u64;

                match WalRecord::decode(&payload[..len]) {
                    Ok(rec) => Ok(Some((frame_offset, rec))),
                    Err(_) => Ok(None),
                }
            }
            WalCursor::Memory {
                wal,
                offsets,
                idx,
                payload,
            } => {
                let frame_offset = match offsets.get(*idx).copied() {
                    Some(o) => o,
                    None => return Ok(None),
                };
                *idx += 1;

                let bytes = match wal.lock_mem().get(&frame_offset).cloned() {
                    Some(b) => b,
                    None => return Ok(None),
                };
                if payload.len() < bytes.len() {
                    payload.resize(bytes.len(), 0);
                }
                payload[..bytes.len()].copy_from_slice(&bytes);

                match WalRecord::decode(&payload[..bytes.len()]) {
                    Ok(rec) => Ok(Some((frame_offset, rec))),
                    Err(_) => Ok(None),
                }
            }
        }
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

    /// 回放到主数据文件的**待办量**：WAL 中已提交但尚未写回主文件的页数。
    ///
    /// ## 为什么需要它
    ///
    /// 只读打开不能碰主数据文件，但 [`StorageEngine::open`] 的回放**会写**。
    /// 因此只读者必须先确认「没有待回放内容」，否则它无法保证自己看到的
    /// 是与主文件一致的状态。
    ///
    /// 判定与回放本身共用同一套事务状态推导（`collect_committed_txs`），
    /// 避免出现「这里说没有、回放时却有」的不一致。
    ///
    /// 返回 `Ok(0)` 表示可以安全地只读打开。
    pub fn pending_replay_pages(&self) -> Result<usize, GraphError> {
        if self.is_memory {
            return Ok(0);
        }
        let (committed, aborted) = collect_committed_txs(&self.wal)?;
        if committed.is_empty() {
            return Ok(0);
        }
        let mut pending = 0usize;
        let mut cursor = self.wal.cursor()?;
        while let Some((_, record)) = cursor.next_frame()? {
            if let WalRecord::PageWrite { tx_id, .. } = record {
                if committed.contains(&tx_id) && !aborted.contains(&tx_id) {
                    pending += 1;
                }
            }
        }
        Ok(pending)
    }

    /// 打开存储引擎但**不回放** WAL。
    ///
    /// 供只读打开前的预检使用：它只读 WAL 与主文件，不写任何字节。
    /// 调用方随后用 [`StorageEngine::pending_replay_pages`] 判断能否安全地
    /// 以只读方式打开。
    pub fn open_readonly<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
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
        let mut wal_path_str = db_path.as_os_str().to_os_string();
        wal_path_str.push(".wal");
        let wal_path = PathBuf::from(wal_path_str);

        // 主文件不存在时**不创建**：只读打开不应产生副作用
        let wal = WalWriter::open(&wal_path, false)?;

        Ok(Self {
            db_path,
            wal,
            is_memory: false,
        })
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

/// 将 WAL 中所有已提交事务的物理页重放到主数据文件（**流式，两遍扫描**）。
///
/// 未提交事务（只有 `PageWrite` 而没有 `TxCommit`）的帧会被忽略，这是 STEAL
/// 策略下「主库零污染」的根本保证。
///
/// ## 为什么是两遍
///
/// 第一遍必须先知道「哪些事务算提交」，第二遍才能决定写哪些页。旧实现用
/// `read_all_frames()` 一次拿到全部帧，在 1.5GB 的 WAL 上会把整个日志拉进内存，
/// 违反「常驻内存受缓冲池硬约束」这一不变量。改成流式后：
///
/// - 峰值内存 = 单帧载荷(4KB) + 事务状态集，即 `O(已提交事务数)`，与 WAL 体积无关
/// - 代价是多读一遍 WAL，但两遍都是顺序读且走页缓存，实测远低于原实现的内存代价
///
/// ## 崩溃一致性
///
/// 数据页与 CRC 目录**都 fsync 之后**才返回；调用方随后才允许截断 WAL。
/// 若在返回前崩溃，WAL 仍在，重启会重放并重算校验和，因此不会产生假阳性。
pub fn apply_committed_pages(wal: &WalWriter, db_path: &Path) -> Result<usize, GraphError> {
    apply_committed_pages_with(wal, db_path, &mut |_, _| Ok(()))
}

/// 第一遍：流式收集事务状态（提交集 / 回滚集），4KB 页镜像在此遍立即丢弃。
///
/// 单独抽出来是因为「回放写页」与「补算校验和」是两件必须分先后的事
/// （见 [`for_each_committed_page`]），但两者都需要同一份事务状态。
fn collect_committed_txs(wal: &WalWriter) -> Result<(HashSet<u64>, HashSet<u64>), GraphError> {
    let mut committed_txs: HashSet<u64> = HashSet::new();
    let mut aborted_txs: HashSet<u64> = HashSet::new();
    let mut cursor = wal.cursor()?;
    while let Some((_, record)) = cursor.next_frame()? {
        match record {
            WalRecord::TxCommit { tx_id } => {
                committed_txs.insert(tx_id);
            }
            WalRecord::TxRollback { tx_id } => {
                aborted_txs.insert(tx_id);
            }
            _ => {}
        }
    }
    Ok((committed_txs, aborted_txs))
}

/// 遍历 WAL 中所有**已提交**页并回调，但**不写主数据文件**。
///
/// ## 为什么与回放分开
///
/// 校验和必须与页内容成对更新，但回放（由 [`StorageEngine::open`] 完成）发生在
/// `CrcStore` 之前，且**必须先于** `DiskManager::open`：回放会把主文件补长，
/// 而 `DiskManager::allocate_page` 的页号高水位是从文件长度推导的，
/// 若在回放前创建，后续分配就可能与刚回放的页号相撞。
///
/// 因此顺序固定为：回放 → 创建 `DiskManager` → 创建 `CrcStore` → 本函数补算校验和。
/// 分两次遍历 WAL 的代价是顺序读一遍日志，换掉的是「回放后校验和与实际内容不符」
/// ——那会让恢复出来的页在读取时报校验和错误，而 `get_node` 是 lossy 的，
/// 表现为**看起来像丢数据**。
pub fn for_each_committed_page<F>(wal: &WalWriter, mut f: F) -> Result<usize, GraphError>
where
    F: FnMut(PageId, &[u8; PAGE_SIZE]) -> Result<(), GraphError>,
{
    let (committed_txs, aborted_txs) = collect_committed_txs(wal)?;
    if committed_txs.is_empty() {
        return Ok(0);
    }

    let mut seen = 0usize;
    let mut cursor = wal.cursor()?;
    while let Some((_, record)) = cursor.next_frame()? {
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
                let mut page = [0u8; PAGE_SIZE];
                page.copy_from_slice(&data);
                f(page_id, &page)?;
                seen += 1;
            }
        }
    }
    Ok(seen)
}

/// 与 [`apply_committed_pages`] 相同，但每写回一页就回调一次，
/// 供上层同步页校验和（CRC 目录）。
pub fn apply_committed_pages_with<F>(
    wal: &WalWriter,
    db_path: &Path,
    on_page_written: &mut F,
) -> Result<usize, GraphError>
where
    F: FnMut(PageId, &[u8; PAGE_SIZE]) -> Result<(), GraphError>,
{
    let (committed_txs, aborted_txs) = collect_committed_txs(wal)?;

    if committed_txs.is_empty() {
        return Ok(0);
    }

    // ── Pass 2：再次流式遍历，只写已提交页 ──
    let mut db_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(db_path)?;

    let mut applied = 0usize;
    let mut cursor = wal.cursor()?;
    while let Some((_, record)) = cursor.next_frame()? {
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
                let mut page = [0u8; PAGE_SIZE];
                page.copy_from_slice(&data);
                let offset = (page_id as u64) * (PAGE_SIZE as u64);
                db_file.seek(SeekFrom::Start(offset))?;
                db_file.write_all(&page)?;
                on_page_written(page_id, &page)?;
                applied += 1;
            }
        }
    }

    db_file.flush()?;
    db_file.sync_all()?;

    Ok(applied)
}
