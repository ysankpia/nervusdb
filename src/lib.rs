pub mod algo;
pub mod buffer;
pub mod c_api;
pub mod codec;
pub mod crc;
pub mod crc32;
pub mod cypher;
pub mod disk_graph;
pub mod graph;
pub mod index;
pub mod integrity;
pub mod json;
pub mod lock;
pub mod page;
pub mod query;
pub mod storage;
pub mod sync_ext;

pub use algo::{
    bfs_shortest_path, dijkstra_shortest_path, find_cycles, has_cycle, k_hop_subgraph, pagerank,
    weakly_connected_components, KHopSubgraph, PageRankScore,
};
pub use buffer::{BufferPoolManager, BufferStats, DiskManager};
pub use c_api::*;
pub use cypher::{
    execute_cypher, execute_mutate, execute_query, CypherResultSet, ExecuteResult, Row,
};
pub use disk_graph::{DiskGraph, GraphMetaSnapshot};
pub use graph::{Direction, Edge, GraphError, Node, Value};
pub use index::IndexManager;
pub use integrity::{check_integrity, IntegrityIssue, IntegrityIssueKind, IntegrityReport};
pub use lock::DbLock;
pub use page::{EdgeRecord, NodeRecord, PageId, PAGE_SIZE};
pub use query::{GraphQuery, MultiHopPath, PathMatch, QueryBuilder, QueryResult};
pub use storage::{StorageEngine, WalRecord};
pub use sync_ext::{MutexRecoverExt, RwLockRecoverExt};

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// 微型缓冲池：1MB（256 帧），用于极低内存环境与受限内存回归测试
pub const SMALL_POOL_FRAMES: usize = 256;
/// 标准默认缓冲池：4MB（1024 帧）
pub const DEFAULT_BUFFER_POOL_FRAMES: usize = 1024;
/// 常规生产缓冲池：16MB（4096 帧）
pub const MEDIUM_POOL_FRAMES: usize = 4096;
/// 大规模离线导入缓冲池：64MB（16384 帧）
pub const LARGE_POOL_FRAMES: usize = 16384;

/// 每 MB 对应的 4KB 页帧数
pub const FRAMES_PER_MB: usize = 256;

/// 默认的 WAL 自动 Checkpoint 阈值：64MB。
///
/// 越过该值后在**写锁之外**触发一次 Checkpoint，避免长时间批量导入把 WAL
/// 撑爆磁盘。设为 `0` 可关闭（见 [`GraphLiteOptions`]）。
pub const DEFAULT_WAL_AUTO_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;

/// 打开数据库时的可调参数。
///
/// 既有构造器（`open` / `open_with_pool_size` / `open_with_pool_mb`）都使用
/// [`GraphLiteOptions::default`]，因此默认启用 64MB 自动 Checkpoint，对调用方透明。
#[derive(Debug, Clone)]
pub struct GraphLiteOptions {
    /// 缓冲池帧数上限（每帧 4KB）
    pub buffer_pool_frames: usize,
    /// WAL 体积达到该阈值后自动 Checkpoint；`0` 表示关闭
    pub wal_auto_checkpoint_bytes: u64,
    /// 以只读模式打开：取共享锁，可与其它读者共存，但与任何写者互斥。
    ///
    /// 只读句柄**绝不写主数据文件**，因此不触发 WAL 回放。若 WAL 中确有已提交
    /// 但未回放的页，打开会失败并提示先用读写句柄打开一次——静默忽略那些页会
    /// 让读者看到过期数据。
    pub read_only: bool,
}

impl Default for GraphLiteOptions {
    fn default() -> Self {
        Self {
            buffer_pool_frames: DEFAULT_BUFFER_POOL_FRAMES,
            wal_auto_checkpoint_bytes: DEFAULT_WAL_AUTO_CHECKPOINT_BYTES,
            read_only: false,
        }
    }
}

/// 内部核心结构体（彻底剔除内存 HashMap 图，DiskGraph 为唯一数据源）
pub struct GraphInner {
    pub disk_graph: DiskGraph,
    pub storage: StorageEngine,
    pub index_mgr: IndexManager,
    pub next_tx_id: u64,
    /// WAL 已达自动 Checkpoint 阈值（写锁内置位，写锁外消费）
    pub wal_checkpoint_pending: Arc<AtomicBool>,
    /// 自动 Checkpoint 阈值快照
    pub wal_auto_checkpoint_bytes: u64,
}

/// GraphLite: 生产级纯磁盘嵌入式属性图数据库引擎 (SQLite 3.0 标准)
#[derive(Clone)]
pub struct GraphLite {
    inner: Arc<RwLock<GraphInner>>,
    db_path: PathBuf,
    /// 进程级锁守卫：写句柄取排他锁，只读句柄取共享锁。
    /// 锁随句柄 Drop 自动释放；`:memory:` 模式为 `None`。
    lock: Arc<Option<DbLock>>,
    /// 本句柄是否以只读方式打开（决定写入口是否拒绝）
    read_only: bool,
}

impl GraphLite {
    /// 打开或创建指定路径的图数据库 (默认 4MB Buffer Pool)
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        Self::open_with_pool_size(path, DEFAULT_BUFFER_POOL_FRAMES)
    }

    /// 以 MB 为单位打开图数据库（frames = mb × 256，下限 2 帧）。
    ///
    /// 便捷构造器：`open_with_pool_mb(path, 1)` 等价于 256 帧（1MB），
    /// `open_with_pool_mb(path, 16)` 等价于 4096 帧（16MB）。
    pub fn open_with_pool_mb<P: AsRef<Path>>(path: P, mb: usize) -> Result<Self, GraphError> {
        Self::open_with_pool_size(path, (mb * FRAMES_PER_MB).max(2))
    }

    /// 以**只读**模式打开图数据库：取共享锁，可与其它读者共存。
    ///
    /// 适合「一个进程写入、多个进程观察」的场景（例如后台 Agent 写、前台界面读）。
    /// 写入口在只读句柄上会返回明确错误，而不是静默尝试。
    ///
    /// 若 WAL 中还有未回放的已提交页，打开会失败并提示先用读写句柄打开一次——
    /// 读者不能回放（那会写主数据文件），静默跳过会让它看到过期数据。
    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        Self::open_with_options(
            path,
            GraphLiteOptions {
                read_only: true,
                ..GraphLiteOptions::default()
            },
        )
    }

    /// 打开图数据库并自定义 Buffer Pool 帧数上限（纯磁盘受控，无内存泄露）
    pub fn open_with_pool_size<P: AsRef<Path>>(
        path: P,
        pool_size: usize,
    ) -> Result<Self, GraphError> {
        Self::open_with_options(
            path,
            GraphLiteOptions {
                buffer_pool_frames: pool_size,
                ..GraphLiteOptions::default()
            },
        )
    }

    /// 打开图数据库并完全自定义参数（缓冲池帧数与 WAL 自动 Checkpoint 阈值）。
    pub fn open_with_options<P: AsRef<Path>>(
        path: P,
        options: GraphLiteOptions,
    ) -> Result<Self, GraphError> {
        let pool_size = options.buffer_pool_frames;
        let db_path = path.as_ref().to_path_buf();
        let is_memory = db_path.to_str() == Some(":memory:") || db_path.as_os_str().is_empty();

        // 0. 先获取进程级锁，再执行任何读写。
        //    顺序至关重要：`StorageEngine::open` 会回放 WAL 并**写主数据文件**，
        //    若在其之后才加锁，并发回放本身就已经破坏了数据。
        //
        //    只读模式取**共享**锁：多个读者可以共存，但与写者互斥。
        let lock = if is_memory {
            None
        } else if options.read_only {
            Some(DbLock::acquire_shared(&db_path)?)
        } else {
            Some(DbLock::acquire(&db_path)?)
        };

        // 0a. 只读模式不得回放 WAL：那会写主数据文件。
        //     因此必须先确认没有待回放内容，否则读者会看到过期数据。
        //     这一步必须在 `StorageEngine::open` 之前——顺序反了就已经写了。
        if options.read_only && !is_memory {
            let engine = StorageEngine::open_readonly(&db_path)?;
            let pending = engine.pending_replay_pages()?;
            if pending > 0 {
                return Err(GraphError::StorageError(format!(
                    "Cannot open '{}' read-only: its WAL holds {} committed page(s) \
                     not yet replayed into the data file.\n\
                     A read-only handle never writes, so it cannot apply them and would \
                     return stale data.\n\
                     Open the database once with a read-write handle to replay the WAL, \
                     then open it read-only.",
                    db_path.display(),
                    pending
                )));
            }
        }

        // 0b. 格式与尺寸闸门：**必须在任何写入之前**。
        //
        //     顺序是硬约束，原因是 WAL 回放会写主数据文件。若等到回放之后才检查
        //     格式版本，就已经用**当前版本的语义**解释并写回了一个旧格式的库——
        //     那时文件已经被污染，再报错也晚了。
        //
        //     这里同时挡住两件事：
        //     - 版本不符（v1/v2 旧库）：明确指引用户用匹配的旧版导出再导入
        //     - 文件超过 64 GiB：属性指针只有 24 位页号，越界会静默指向错误页
        if !is_memory {
            Self::check_format_version(&db_path)?;
            Self::check_file_size_limit(&db_path)?;
        }

        // 1. 初始化页级 WAL 持久化引擎（若存在未 Checkpoint 的 WAL，自动将已提交页重放至主文件）
        let storage = StorageEngine::open(&db_path)?;

        // 2. 初始化纯磁盘 4KB 物理分页管理器与 LRU Buffer Pool（统一单文件 {path}）
        let disk_manager = Arc::new(DiskManager::open(&db_path)?);
        let mut bpm = BufferPoolManager::new(Arc::clone(&disk_manager), pool_size.max(2));
        // 挂载页级 WAL：缓冲池据此支持 STEAL 溢出与未提交页安全置换
        bpm.attach_wal(Arc::clone(storage.wal_writer()));
        let bpm = Arc::new(Mutex::new(bpm));
        let disk_graph = DiskGraph::new(bpm)?;

        // 2b. 挂载页校验和存储：惰性计算（只在页落盘/读入时），不污染写入热路径。
        //     根页号来自 Header，因此冷启动后目录链可继续使用。
        {
            let crc_root = disk_graph.crc_dir_root();
            let mut pool = disk_graph.bpm.lock_recover();
            pool.attach_crc(crate::crc::CrcStore::new(
                Arc::clone(&disk_manager),
                crc_root,
            ));

            // 2c. 重新登记刚才回放写回的那些页的校验和。
            //
            //     顺序是硬约束：回放必须早于 `DiskManager::open`（回放会把主文件
            //     补长，而页号高水位由文件长度推导），但回放本身不带 CrcStore。
            //     若不在这里补登记，磁盘上会留着**上一次**写的旧校验和，而页内容
            //     已经是回放后的新内容 —— 之后每次读取该页都会报校验和不匹配。
            //     由于 `get_node` 是 lossy 的（折叠为 `None`），这种现象看起来
            //     就像「已提交的数据在崩溃后丢了」。
            //
            //     只走内存（`record` 不落盘），随后由 `flush()` 统一写入，避免
            //     在打开路径上额外做一次 fsync。
            let wal = Arc::clone(storage.wal_writer());
            crate::storage::for_each_committed_page(&wal, |page_id, data| {
                pool.record_checksum(page_id, data)
            })?;
        }

        // 3. 构建二级索引系统（从持久化 catalog 极速加载，<1ms 杜绝全表 I/O 阻塞）
        let index_mgr = IndexManager::from_catalog(disk_graph.index_catalog.clone());

        let inner = GraphInner {
            disk_graph,
            storage,
            index_mgr,
            next_tx_id: 1,
            wal_checkpoint_pending: Arc::new(AtomicBool::new(false)),
            wal_auto_checkpoint_bytes: options.wal_auto_checkpoint_bytes,
        };

        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
            db_path,
            lock: Arc::new(lock),
            read_only: options.read_only,
        })
    }

    /// 本句柄是否为只读（共享锁）。
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// 只读句柄上的写操作统一入口：给出可操作的错误，而不是让写入静默失败。
    fn reject_write(&self, what: &str) -> Result<(), GraphError> {
        if self.read_only {
            return Err(GraphError::General(format!(
                "Cannot {}: this handle was opened read-only (`GraphLite::open_read_only`).\n\
                 Read-only handles hold a shared lock and never write the data file.\n\
                 Open the database with `GraphLite::open` to modify it.",
                what
            )));
        }
        Ok(())
    }

    /// 打开前校验磁盘格式版本，**早于任何写入**（含 WAL 回放）。
    ///
    /// 只读主文件的前 4096 字节——足够读到 magic 与版本字段，且不需要
    /// `DiskManager`（它在回放之后才能创建）。
    ///
    /// 空文件与全新文件直接放行：它们还没有格式，即将由本版本创建。
    fn check_format_version(db_path: &Path) -> Result<(), GraphError> {
        let mut file = match std::fs::File::open(db_path) {
            Ok(f) => f,
            // 文件不存在是正常的首次创建路径
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(GraphError::IoError(e)),
        };

        let mut header = [0u8; crate::page::PAGE_SIZE];
        // 短读说明文件比一页还小：尚无格式，放行
        if std::io::Read::read(&mut file, &mut header).unwrap_or(0) < crate::page::PAGE_SIZE {
            return Ok(());
        }

        let magic = &header[0..4];
        if magic != crate::page::DB_PAGE_MAGIC && magic != crate::page::DB_PAGE_MAGIC_LEGACY {
            // 不是本项目的文件。交由后续路径处理（会按「新库」初始化），
            // 这里不越权判定。
            return Ok(());
        }

        let file_version = u32::from_le_bytes(
            header[crate::page::HeaderPage::VERSION_OFFSET
                ..crate::page::HeaderPage::VERSION_OFFSET + 4]
                .try_into()
                .unwrap_or([0; 4]),
        );

        if file_version < crate::page::DB_PAGE_VERSION {
            return Err(GraphError::StorageError(format!(
                "Database file format version {} is not readable by this build \
                 (current format version {}).\n\
                 GraphLite 1.0 froze the on-disk format and does not silently \
                 reinterpret older files.\n\
                 To migrate: export the graph with the matching older GraphLite \
                 build via `.dump`, then re-import that script into a fresh database.",
                file_version,
                crate::page::DB_PAGE_VERSION
            )));
        }
        if file_version > crate::page::DB_PAGE_VERSION {
            return Err(GraphError::StorageError(format!(
                "Database file format version {} is newer than this build supports \
                 (current format version {}).\n\
                 Upgrade GraphLite to open this file; do not open it with an older \
                 version, as that risks writing an incompatible format.",
                file_version,
                crate::page::DB_PAGE_VERSION
            )));
        }
        Ok(())
    }

    /// 打开前拒绝超过格式上限的主文件，**早于任何写入**（含 WAL 回放）。
    ///
    /// 属性指针以 24 位存页号，因此只有 `2^24 × 4 KiB = 64 GiB` 的地址空间。
    /// 越界写入会被 `pack_prop_ptr` 拒绝，但**读取**一条已越界的旧数据无法
    /// 自我修复，且越界页号在 24 位空间里会与合法页号混淆。因此在打开时就拒绝，
    /// 而不是等到某次写入才失败——那时库可能已经处于半损坏状态。
    fn check_file_size_limit(db_path: &Path) -> Result<(), GraphError> {
        let size = match std::fs::metadata(db_path) {
            Ok(m) => m.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(GraphError::IoError(e)),
        };
        let limit_bytes =
            (crate::page::MAX_PROP_PAGE_ID as u64 + 1) * crate::page::PAGE_SIZE as u64;
        if size > limit_bytes {
            return Err(GraphError::StorageError(format!(
                "Database file is {} bytes, which exceeds the {} byte ({} GiB) format limit.\n\
                 Property pointers encode a page number in 24 bits, so the largest \
                 addressable file is 2^24 pages x 4 KiB = 64 GiB.\n\
                 This limit is part of the frozen 1.0 format and will not be raised \
                 without a format version bump.",
                size,
                limit_bytes,
                limit_bytes / 1024 / 1024 / 1024
            )));
        }
        Ok(())
    }

    /// 执行 Cypher 变更语句 (如 CREATE / DETACH DELETE)
    pub fn execute(&self, cypher_str: &str) -> Result<ExecuteResult, GraphError> {
        // 变更语句在只读句柄上明确拒绝（读查询仍走 `run_cypher`）
        if self.read_only
            && crate::cypher::parser::Parser::new(
                crate::cypher::lexer::Lexer::new(cypher_str).tokenize()?,
            )
            .parse()
            .map(|st| st.is_mutating())
            .unwrap_or(false)
        {
            self.reject_write("execute a mutating Cypher statement")?;
        }
        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        let result = {
            let GraphInner {
                disk_graph,
                index_mgr,
                ..
            } = &mut *inner;
            cypher::execute_mutate(cypher_str, disk_graph, index_mgr)?
        };

        // 若产生了物理修改，生成页级 WAL 记录并持久化
        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;

        let stats = result.stats;
        drop(inner);
        // 写锁已释放，此时才能安全做 Checkpoint（它需要重新获取写锁）
        self.maybe_auto_checkpoint()?;
        Ok(stats)
    }

    /// 执行 Cypher 查询语句 (True MRSW: 只读语句并发持有共享读锁，写操作持有排他写锁并写入 WAL)
    pub fn query_cypher(&self, cypher_str: &str) -> Result<CypherResultSet, GraphError> {
        self.run_cypher(cypher_str)
    }

    /// 执行任意 Cypher 语句并按 AST 类型自动路由（只读走共享锁并发路径，写操作走单事务 WAL 路径），
    /// 统一返回结果集与执行摘要。供 CLI 等需要「一条语句一个结果」的调用方使用。
    pub fn run_cypher(&self, cypher_str: &str) -> Result<CypherResultSet, GraphError> {
        let lexer = crate::cypher::lexer::Lexer::new(cypher_str);
        let tokens = lexer.tokenize()?;
        let mut parser = crate::cypher::parser::Parser::new(tokens);
        let statement = parser.parse()?;
        let mutating = statement.is_mutating();

        if !mutating {
            // 只读查询：只获取 inner.read() 共享锁，允许几十个读线程无阻塞并发执行！
            let inner = self
                .inner
                .read()
                .map_err(|e| GraphError::General(e.to_string()))?;
            return cypher::execute_query(cypher_str, &inner.disk_graph, &inner.index_mgr);
        }

        // 写操作：获取 inner.write() 排他锁，记录 WAL 并持久化
        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;
        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        let GraphInner {
            disk_graph,
            index_mgr,
            ..
        } = &mut *inner;
        let result = cypher::execute_mutate(cypher_str, disk_graph, index_mgr)?;
        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;
        drop(inner);
        // 写锁已释放，此时才能安全做 Checkpoint
        self.maybe_auto_checkpoint()?;
        Ok(result)
    }

    /// 执行检查点 Checkpoint：将 WAL 中所有已提交物理页落回主文件，刷出常驻脏页并截断 WAL。
    ///
    /// 未提交事务的溢出帧不会被重放，因此检查点绝不会把任何未提交数据写入主库。
    pub fn checkpoint(&self) -> Result<(), GraphError> {
        self.reject_write("run a checkpoint")?;
        let mut inner = self.inner.write_recover();

        // 1. 把 WAL 中已提交的页按序重放到主数据文件，同时为每页记录校验和。
        //    此步可能首次建立 CRC 目录，因此紧接着把根页号写回 Header 元数据。
        {
            let GraphInner {
                disk_graph,
                storage,
                ..
            } = &mut *inner;
            let wal = Arc::clone(storage.wal_writer());
            let db_path = storage.db_path().to_path_buf();
            let root = {
                let mut bpm = disk_graph.bpm.lock_recover();
                bpm.replay_wal_with_crc(&wal, &db_path)?;
                bpm.crc_dir_root()
            };
            if let Some(root) = root {
                disk_graph.set_crc_dir_root(root);
            }
        }

        // 2. Header 帧定稿（含 crc_dir_root）：只标记脏页，尚未落盘
        inner.disk_graph.sync_header()?;

        // 3. 刷出全部已提交脏页。**Page 0 至此才真正带着 root 落到磁盘**。
        //    顺序关键：CrcStore::flush 会读-改-写 Page 0（叠加内联 CRC 区），
        //    若此时 root 还只在缓冲池帧里，它读到的就是旧值并把它写回去，
        //    root 会被静默抹掉。
        inner.disk_graph.flush()?;

        // 4. 校验和目录 + Page 0 内联区落盘并 fsync —— 必须在截断 WAL **之前**完成，
        //    否则崩溃时会留下「数据已写、校验和未写」的中间态。
        {
            let mut bpm = inner.disk_graph.bpm.lock_recover();
            bpm.flush_crc()?;
        }

        // 5. 主文件已成为全部页的权威副本，WAL 页位置索引失效并截断 WAL
        {
            let mut bpm = inner.disk_graph.bpm.lock_recover();
            bpm.clear_wal_page_index();
        }
        inner.storage.checkpoint()
    }

    /// WAL 提交之后调用：越过阈值则**只置位**，不做任何 I/O。
    ///
    /// 置位与执行分离是刻意的：`checkpoint` 需要写锁，而调用点位于写锁之内，
    /// 直接调用会在 `RwLock` 上自锁死。真正的刷盘由
    /// [`GraphLite::maybe_auto_checkpoint`] 在写锁释放后完成。
    fn note_wal_size(inner: &GraphInner) {
        let limit = inner.wal_auto_checkpoint_bytes;
        if limit > 0 && inner.storage.wal_size() >= limit {
            inner.wal_checkpoint_pending.store(true, Ordering::Release);
        }
    }

    /// 在写锁**之外**消费自动 Checkpoint 请求。
    ///
    /// `swap(false)` 保证并发写入时只有抢到标志的那个线程真正刷盘，其余线程直接
    /// 返回：既不会重复刷，也不会互相等待。
    pub(crate) fn maybe_auto_checkpoint(&self) -> Result<(), GraphError> {
        let pending = {
            let inner = self.inner.read_recover();
            inner.wal_checkpoint_pending.swap(false, Ordering::AcqRel)
        };
        if pending {
            self.checkpoint()?;
        }
        Ok(())
    }

    /// 获取 Buffer Pool 运行统计指标
    pub fn buffer_stats(&self) -> BufferStats {
        let inner = self.inner.read_recover();
        let bpm = inner.disk_graph.bpm.lock_recover();
        bpm.stats()
    }

    /// 获取二级索引信息
    pub fn index_labels(&self) -> Vec<String> {
        let inner = self.inner.read_recover();
        inner.index_mgr.indexed_labels()
    }

    pub fn index_properties(&self) -> Vec<(String, String)> {
        let inner = self.inner.read_recover();
        inner.index_mgr.indexed_properties()
    }

    /// 获取数据库主文件路径
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// 扫描数据库的结构完整性（只读，不修改任何页）。
    ///
    /// 依据守恒律而非抽样：同一量用「沿链指针走出」与「遍历边记录」两条独立口径
    /// 计算并比对，因此无需第二份实现即可发现不一致。
    ///
    /// 只报告问题，不自动修复——修复策略需单独设计并经显式授权。
    pub fn integrity_check(&self) -> Result<IntegrityReport, GraphError> {
        let inner = self
            .inner
            .read()
            .map_err(|e| GraphError::General(e.to_string()))?;
        check_integrity(&inner.disk_graph)
    }

    /// 结构校验失败即返回 `Err`；成功返回报告。
    ///
    /// 适合作为运维探针：`db.verify()?` 通过即认为库结构自洽。
    pub fn verify(&self) -> Result<IntegrityReport, GraphError> {
        let report = self.integrity_check()?;
        if report.is_ok() {
            Ok(report)
        } else {
            Err(report.into_error())
        }
    }

    /// 直接从主文件读取并校验指定页的 CRC32（运维探针）。
    ///
    /// 绕过缓冲池，因此校验的是磁盘上的实际内容，而不是可能已被修好的内存副本。
    /// 未记录 CRC 的页（新分配、或从未落盘）会直接通过。
    pub fn verify_page_on_disk(&self, page_id: u32) -> Result<(), GraphError> {
        let inner = self.inner.read_recover();
        inner.disk_graph.verify_page_on_disk(page_id)
    }

    /// 该句柄是否持有了进程级排他锁（`:memory:` 模式下为 `false`）。
    ///
    /// 锁由 `GraphLite` 持有并在其 Drop 时释放；此访问器同时让编译器确认
    /// 锁字段被真实读取，而非仅在构造时赋值。
    pub fn is_locked(&self) -> bool {
        self.lock.is_some()
    }

    /// 获取当前节点总数（纯磁盘定长元数据头统计）
    pub fn node_count(&self) -> usize {
        let inner = self.inner.read_recover();
        inner.disk_graph.node_count
    }

    /// 获取当前边总数
    pub fn edge_count(&self) -> usize {
        let inner = self.inner.read_recover();
        inner.disk_graph.edge_count
    }

    /// 获取指定节点（按需通过 Buffer Pool 调入）。
    ///
    /// **有损 API**：存储层的 I/O 或损坏错误会被折叠为 `None`，因此无法区分
    /// 「节点不存在」与「页读不出来」。生产代码应改用 [`GraphLite::try_get_node`]。
    pub fn get_node(&self, id: u64) -> Option<Node> {
        let inner = self.inner.read_recover();
        inner.disk_graph.get_node(id).ok().flatten()
    }

    /// 获取指定边（按需通过 Buffer Pool 调入）。
    ///
    /// **有损 API**：同 [`GraphLite::get_node`]，错误被折叠为 `None`。
    /// 生产代码应改用 [`GraphLite::try_get_edge`]。
    pub fn get_edge(&self, id: u64) -> Option<Edge> {
        let inner = self.inner.read_recover();
        inner.disk_graph.get_edge(id).ok().flatten()
    }

    /// 获取指定节点，**完整保留**存储错误：`Ok(None)` 仅表示节点不存在。
    pub fn try_get_node(&self, id: u64) -> Result<Option<Node>, GraphError> {
        let inner = self
            .inner
            .read()
            .map_err(|e| GraphError::General(e.to_string()))?;
        inner.disk_graph.get_node(id)
    }

    /// 获取指定边，**完整保留**存储错误：`Ok(None)` 仅表示边不存在。
    pub fn try_get_edge(&self, id: u64) -> Result<Option<Edge>, GraphError> {
        let inner = self
            .inner
            .read()
            .map_err(|e| GraphError::General(e.to_string()))?;
        inner.disk_graph.get_edge(id)
    }

    /// 添加单个节点（纯磁盘定长写入，优先复用 Freelist，记录页级 WAL）
    pub fn add_node(
        &self,
        labels: HashSet<String>,
        properties: HashMap<String, Value>,
    ) -> Result<u64, GraphError> {
        self.reject_write("add a node")?;
        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        let node_id = inner
            .disk_graph
            .add_node(labels.clone(), properties.clone())?;

        // 维护二级索引与目录
        for l in &labels {
            inner.index_mgr.insert_label(l, node_id);
            inner.disk_graph.index_catalog.labels.insert(l.clone());
            for (k, v) in &properties {
                inner.index_mgr.insert_property(l, k, v.clone(), node_id);
                inner
                    .disk_graph
                    .index_catalog
                    .properties
                    .insert((l.clone(), k.clone()));
            }
        }

        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;
        drop(inner);
        self.maybe_auto_checkpoint()?;
        Ok(node_id)
    }

    /// 添加单条有向属性边（维护磁盘双向双环免索引邻接链表）
    pub fn add_edge(
        &self,
        src_id: u64,
        dst_id: u64,
        edge_type: impl Into<String>,
        properties: HashMap<String, Value>,
        weight: f64,
    ) -> Result<u64, GraphError> {
        self.reject_write("add an edge")?;
        let edge_type_str = edge_type.into();
        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        let edge_id =
            inner
                .disk_graph
                .add_edge(src_id, dst_id, &edge_type_str, properties, weight)?;

        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;
        drop(inner);
        self.maybe_auto_checkpoint()?;
        Ok(edge_id)
    }

    /// 删除节点（级联删除关联边，槽位回收至 Freelist）
    pub fn remove_node(&self, id: u64) -> Result<Node, GraphError> {
        self.reject_write("remove a node")?;
        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        let node = inner.disk_graph.remove_node(id)?;

        // 清理二级索引
        let labels_bt: std::collections::BTreeSet<String> = node.labels.iter().cloned().collect();
        inner
            .index_mgr
            .remove_node_all_indices(id, &labels_bt, &node.properties);

        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;
        drop(inner);
        self.maybe_auto_checkpoint()?;
        Ok(node)
    }

    /// 删除边（从双向双环磁盘链表中脱链，槽位回收至 Freelist）
    pub fn remove_edge(&self, id: u64) -> Result<Edge, GraphError> {
        self.reject_write("remove an edge")?;
        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        let edge = inner.disk_graph.remove_edge(id)?;

        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;
        drop(inner);
        self.maybe_auto_checkpoint()?;
        Ok(edge)
    }

    /// 更新节点属性
    pub fn update_node_property<V: Into<Value>>(
        &self,
        id: u64,
        key: impl Into<String>,
        value: V,
    ) -> Result<(), GraphError> {
        self.reject_write("update a node property")?;
        let key_str = key.into();
        let val = value.into();

        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        let old_val = if let Ok(Some(node)) = inner.disk_graph.get_node(id) {
            node.get_prop(&key_str).cloned()
        } else {
            None
        };

        inner
            .disk_graph
            .update_node_property(id, key_str.clone(), val.clone())?;

        // 更新索引：先移除旧值索引，再插入新值索引
        if let Ok(Some(node)) = inner.disk_graph.get_node(id) {
            for l in &node.labels {
                if let Some(ref ov) = old_val {
                    inner.index_mgr.remove_property(l, &key_str, ov, id);
                }
                inner
                    .index_mgr
                    .insert_property(l, &key_str, val.clone(), id);
                inner.disk_graph.index_catalog.labels.insert(l.clone());
                inner
                    .disk_graph
                    .index_catalog
                    .properties
                    .insert((l.clone(), key_str.clone()));
            }
        }

        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;
        drop(inner);
        self.maybe_auto_checkpoint()?;
        Ok(())
    }

    /// 更新边属性
    pub fn update_edge_property<V: Into<Value>>(
        &self,
        id: u64,
        key: impl Into<String>,
        value: V,
    ) -> Result<(), GraphError> {
        self.reject_write("update an edge property")?;
        let key_str = key.into();
        let val = value.into();

        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        inner.disk_graph.update_edge_property(id, key_str, val)?;

        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;
        drop(inner);
        self.maybe_auto_checkpoint()?;
        Ok(())
    }

    /// 内部辅助：把当前事务修改的物理页原子提交到页级 WAL。
    ///
    /// 只有常驻缓冲池的脏页需要追加 redo 帧；早先因缓冲池耗尽而被 STEAL 溢出到 WAL 的
    /// 非常驻页，其最新镜像已在 WAL 中。整个流程以 O(1) 内存完成，绝不随事务规模线性膨胀。
    fn commit_dirty_pages_to_wal(inner: &mut GraphInner, tx_id: u64) -> Result<(), GraphError> {
        let modified_pages = inner.disk_graph.drain_modified_pages();
        let result = {
            let mut bpm = inner.disk_graph.bpm.lock_recover();
            bpm.begin_tx(tx_id);
            bpm.commit_tx(tx_id, &modified_pages)
        };
        Self::note_wal_size(inner);
        result
    }

    /// 开启显式事务
    pub fn begin_transaction(&self) -> Result<Transaction, GraphError> {
        let tx_id = {
            let mut inner = self
                .inner
                .write()
                .map_err(|e| GraphError::General(e.to_string()))?;
            let tx_id = inner.next_tx_id;
            inner.next_tx_id += 1;
            tx_id
        };

        Ok(Transaction {
            db: self.clone(),
            tx_id,
            ops: Vec::new(),
            committed: false,
        })
    }

    /// 构造链式查询执行器（基于纯磁盘游标执行，无锁并发只读遍历）
    pub fn query(&self) -> GraphQuery {
        let inner = self.inner.read_recover();
        GraphQuery::new(inner.disk_graph.clone())
    }

    /// 在单个显式事务内执行一段批量写入，成功自动 `commit`，闭包返回 `Err` 时自动 `rollback`。
    ///
    /// 这是批量写入的首选入口：整个闭包内的所有变更共享一次 WAL 追加与**一次 fsync**，
    /// 因此吞吐量相较逐条自动提交可提升两到三个数量级。
    pub fn with_transaction<F, R>(&self, f: F) -> Result<R, GraphError>
    where
        F: FnOnce(&mut Transaction) -> Result<R, GraphError>,
    {
        let mut tx = self.begin_transaction()?;
        match f(&mut tx) {
            Ok(value) => {
                tx.commit()?;
                Ok(value)
            }
            Err(err) => {
                // 闭包失败：丢弃事务动作，未提交数据绝不落库
                tx.rollback()?;
                Err(err)
            }
        }
    }

    /// 执行带权 Dijkstra 最短路径算法（纯磁盘流式遍历，脱离全局锁并发执行）
    pub fn dijkstra(
        &self,
        start_id: u64,
        end_id: u64,
        edge_type: Option<&str>,
    ) -> Option<(f64, Vec<u64>)> {
        let graph = {
            let inner = self.inner.read_recover();
            inner.disk_graph.clone()
        };
        algo::dijkstra_shortest_path(&graph, start_id, end_id, edge_type)
    }

    /// 执行无权 BFS 最短路径算法（纯磁盘流式遍历，脱离全局锁并发执行）
    pub fn bfs(&self, start_id: u64, end_id: u64, edge_type: Option<&str>) -> Option<Vec<u64>> {
        let graph = {
            let inner = self.inner.read_recover();
            inner.disk_graph.clone()
        };
        algo::bfs_shortest_path(&graph, start_id, end_id, edge_type)
    }

    /// 检测全图是否存在有向环路（纯磁盘按页扫描，脱离全局锁并发执行）
    pub fn has_cycle(&self) -> bool {
        let graph = {
            let inner = self.inner.read_recover();
            inner.disk_graph.clone()
        };
        algo::has_cycle(&graph)
    }

    /// 查找全图所有有向环路（脱离全局锁并发执行）
    pub fn find_cycles(&self) -> Vec<Vec<u64>> {
        let graph = {
            let inner = self.inner.read_recover();
            inner.disk_graph.clone()
        };
        algo::find_cycles(&graph)
    }

    /// PageRank 阻尼迭代：评估全图节点影响力（默认阻尼 0.85、最长 100 轮、容差 1e-6）
    pub fn pagerank(&self) -> Vec<algo::PageRankScore> {
        self.pagerank_with(0.85, 100, 1e-6)
    }

    /// PageRank 阻尼迭代（自定义阻尼因子、最大迭代轮数与收敛容差），按分数降序返回
    pub fn pagerank_with(
        &self,
        damping_factor: f64,
        max_iterations: usize,
        tolerance: f64,
    ) -> Vec<algo::PageRankScore> {
        let graph = {
            let inner = self.inner.read_recover();
            inner.disk_graph.clone()
        };
        algo::pagerank(&graph, damping_factor, max_iterations, tolerance)
    }

    /// 弱连通分量分析（并查集划分社群 / 孤岛检测），按分量规模降序返回
    pub fn weakly_connected_components(&self) -> Vec<Vec<u64>> {
        let graph = {
            let inner = self.inner.read_recover();
            inner.disk_graph.clone()
        };
        algo::weakly_connected_components(&graph)
    }

    /// K-Hop 局部子图提取（默认沿无向边扩展）
    pub fn k_hop_subgraph(
        &self,
        start_id: u64,
        k: usize,
    ) -> Result<algo::KHopSubgraph, GraphError> {
        self.k_hop_subgraph_with(start_id, k, Direction::Both, None)
    }

    /// K-Hop 局部子图提取（自定义扩展方向与关系类型过滤）
    pub fn k_hop_subgraph_with(
        &self,
        start_id: u64,
        k: usize,
        direction: Direction,
        edge_type: Option<&str>,
    ) -> Result<algo::KHopSubgraph, GraphError> {
        let graph = {
            let inner = self.inner.read_recover();
            inner.disk_graph.clone()
        };
        algo::k_hop_subgraph(&graph, start_id, k, direction, edge_type)
    }

    /// 获取图模式中的全部节点标签
    pub fn labels(&self) -> Vec<String> {
        let inner = self.inner.read_recover();
        inner.index_mgr.indexed_labels()
    }

    /// 获取图模式中的全部关系类型（以 DiskGraph 的持久化目录为权威来源）
    pub fn edge_types(&self) -> Vec<String> {
        let inner = self.inner.read_recover();
        inner
            .disk_graph
            .index_catalog
            .edge_types
            .iter()
            .cloned()
            .collect()
    }

    /// 导出当前图数据为可回灌的 Cypher 脚本（流式写入，无中间大字符串）
    pub fn dump_cypher<W: Write>(&self, mut out: W) -> Result<(), GraphError> {
        let inner = self
            .inner
            .read()
            .map_err(|e| GraphError::General(e.to_string()))?;
        let graph = &inner.disk_graph;

        writeln!(out, "-- GraphLite-RS logical dump")?;
        writeln!(
            out,
            "-- nodes: {}, edges: {}",
            graph.node_count, graph.edge_count
        )?;

        for node_id in graph.all_node_ids()? {
            let node = match graph.get_node(node_id)? {
                Some(n) => n,
                None => continue,
            };
            let mut labels: Vec<String> = node.labels.iter().cloned().collect();
            labels.sort();

            let mut props: Vec<(String, Value)> = node
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            props.sort_by(|a, b| a.0.cmp(&b.0));

            // 节点身份用 `__glid` 承载，标签按需附加
            let mut create_parts: Vec<String> = Vec::new();
            if !labels.is_empty() {
                create_parts.push(format!(":{}", labels.join(":")));
            }
            create_parts.push(format!("{{__glid: {}}}", node_id));
            writeln!(out, "CREATE ({});", create_parts.join(" "))?;

            // 属性走 SET：回灌到既有节点时为「替换」语义，天然幂等
            if !props.is_empty() {
                let assignments: Vec<String> = props
                    .iter()
                    .map(|(k, v)| format!("n.{} = {}", k, format_literal(v)))
                    .collect();
                writeln!(
                    out,
                    "MATCH (n {{__glid: {}}}) SET {};",
                    node_id,
                    assignments.join(", ")
                )?;
            }
        }

        for node_id in graph.all_node_ids()? {
            for edge in graph.outgoing_edges(node_id)? {
                let mut props: Vec<(String, Value)> = edge
                    .properties
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                props.sort_by(|a, b| a.0.cmp(&b.0));

                let rel = if props.is_empty() {
                    format!("[:{}]", edge.edge_type)
                } else {
                    format!("[:{} {{{}}}]", edge.edge_type, format_props(&props))
                };

                writeln!(
                    out,
                    "MATCH (a {{__glid: {}}}), (b {{__glid: {}}}) CREATE (a)-{}->(b);",
                    edge.src_id, edge.dst_id, rel
                )?;
            }
        }

        Ok(())
    }
}

/// 将属性键值对渲染为 Cypher 映射字面量片段
fn format_props(props: &[(String, Value)]) -> String {
    props
        .iter()
        .map(|(k, v)| format!("{}: {}", k, format_literal(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 将属性值渲染为 Cypher 字面量
fn format_literal(value: &Value) -> String {
    match value {
        Value::Int(v) => v.to_string(),
        Value::Float(v) => v.to_string(),
        Value::Bool(v) => v.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "\\'")),
    }
}

/// 事务操作原子动作记录（用于显式事务在 commit 前的缓存）
#[derive(Debug, Clone)]
pub enum TxAction {
    AddNode {
        id: u64,
        labels: HashSet<String>,
        properties: HashMap<String, Value>,
    },
    AddEdge {
        id: u64,
        src_id: u64,
        dst_id: u64,
        edge_type: String,
        properties: HashMap<String, Value>,
        weight: f64,
    },
    UpdateNodeProp {
        id: u64,
        key: String,
        value: Value,
    },
    UpdateEdgeProp {
        id: u64,
        key: String,
        value: Value,
    },
    RemoveNode {
        id: u64,
    },
    RemoveEdge {
        id: u64,
    },
}

/// 显式事务上下文结构体（严格遵循 ACID，回滚彻底复原）
pub struct Transaction {
    db: GraphLite,
    tx_id: u64,
    ops: Vec<TxAction>,
    committed: bool,
}

impl Transaction {
    pub fn tx_id(&self) -> u64 {
        self.tx_id
    }

    /// 事务内添加节点
    pub fn add_node(
        &mut self,
        labels: HashSet<String>,
        properties: HashMap<String, Value>,
    ) -> Result<u64, GraphError> {
        let id = {
            let mut inner = self
                .db
                .inner
                .write()
                .map_err(|e| GraphError::General(e.to_string()))?;
            inner.disk_graph.allocate_next_node_id()?
        };

        self.ops.push(TxAction::AddNode {
            id,
            labels,
            properties,
        });

        Ok(id)
    }

    /// 事务内添加边
    ///
    /// 此处仅分配槽位并缓存动作；权重合法性（如非负约束）在 `commit` 的 apply 阶段
    /// 统一由 `DiskGraph` 校验，从而保证一条非法边会令整个事务原子失败并干净回滚。
    pub fn add_edge(
        &mut self,
        src_id: u64,
        dst_id: u64,
        edge_type: impl Into<String>,
        properties: HashMap<String, Value>,
        weight: f64,
    ) -> Result<u64, GraphError> {
        let id = {
            let mut inner = self
                .db
                .inner
                .write()
                .map_err(|e| GraphError::General(e.to_string()))?;
            inner.disk_graph.allocate_next_edge_id()?
        };

        self.ops.push(TxAction::AddEdge {
            id,
            src_id,
            dst_id,
            edge_type: edge_type.into(),
            properties,
            weight,
        });

        Ok(id)
    }

    /// 事务内更新节点属性
    pub fn update_node_property<V: Into<Value>>(
        &mut self,
        id: u64,
        key: impl Into<String>,
        value: V,
    ) {
        self.ops.push(TxAction::UpdateNodeProp {
            id,
            key: key.into(),
            value: value.into(),
        });
    }

    /// 事务内更新边属性
    pub fn update_edge_property<V: Into<Value>>(
        &mut self,
        id: u64,
        key: impl Into<String>,
        value: V,
    ) {
        self.ops.push(TxAction::UpdateEdgeProp {
            id,
            key: key.into(),
            value: value.into(),
        });
    }

    /// 事务内删除节点
    pub fn remove_node(&mut self, id: u64) {
        self.ops.push(TxAction::RemoveNode { id });
    }

    /// 事务内删除边
    pub fn remove_edge(&mut self, id: u64) {
        self.ops.push(TxAction::RemoveEdge { id });
    }

    /// 提交事务：将修改原子应用至 DiskGraph，批量刷出 PageWrite 帧并提交。
    ///
    /// 连续 `AddEdge` 段达到 `EDGE_BATCH_WEAVE_MIN` 时自动走两阶段批量织网，
    /// 消除受限内存下的批内缓存抖动与假溢出。
    pub fn commit(self) -> Result<(), GraphError> {
        self.commit_internal(true)
    }

    /// 提交事务，但**禁用**批量织网，强制按客户端原序逐条插入每一条边。
    ///
    /// 仅供回归与等价性对照使用（验证批量织网与逐条路径产出完全相同的图结构）。
    /// 生产路径请使用 [`Transaction::commit`]。
    #[doc(hidden)]
    pub fn commit_unclustered(self) -> Result<(), GraphError> {
        self.commit_internal(false)
    }

    /// 事务提交内部实现：`enable_weave` 控制是否启用两阶段批量织网。
    ///
    /// 两条路径共用同一失败回滚与 WAL 提交尾部，保证回滚语义完全一致。
    fn commit_internal(mut self, enable_weave: bool) -> Result<(), GraphError> {
        if self.committed {
            return Ok(());
        }

        let mut inner = self
            .db
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        // 事务生效前采集轻量元数据快照（O(1) 规模，不含图拓扑），用于失败时精确回拨
        let snapshot = inner.disk_graph.snapshot_meta();
        {
            let mut bpm = inner.disk_graph.bpm.lock_recover();
            bpm.begin_tx(self.tx_id);
        }

        let mut failed_err = None;
        // Take ownership of the action list instead of draining it into a fresh
        // allocation; the planner needs owned actions and `take` avoids the copy.
        let ops: Vec<TxAction> = std::mem::take(&mut self.ops);

        let mut idx = 0usize;
        while idx < ops.len() {
            // 连续 AddEdge 段达到阈值时走两阶段批量织网，消除批内缓存抖动。
            // 只合并**连续**段，绝不跨非边操作重排，保证 AddNode 先于 AddEdge 的依赖不变。
            // 混合事务因段内夹杂非边操作而天然不触发批量路径，无需额外安全性启发式。
            let is_edge = enable_weave && matches!(ops[idx], TxAction::AddEdge { .. });
            if is_edge {
                let mut end = idx;
                while end < ops.len() && matches!(ops[end], TxAction::AddEdge { .. }) {
                    end += 1;
                }
                if end - idx >= crate::disk_graph::EDGE_BATCH_WEAVE_MIN {
                    // 长段切成有界子段：织网在内存中构建 O(段长) 的哈希表与
                    // 记录向量，一次提交上千万条边会突破「内存由缓冲池决定」的
                    // 架构承诺（AGENTS.md §1）。子段上限使峰值内存有界。
                    //
                    // 按序逐段施加即可保持等价：每一子段的「批次前旧链头」就是
                    // 上一子段写下的链头，拼接后与一次织完整段结果相同。
                    let mut sub_start = idx;
                    while sub_start < end {
                        let sub_end =
                            (sub_start + crate::disk_graph::MAX_BATCH_EDGES_IN_MEMORY).min(end);

                        let batch: Vec<crate::disk_graph::EdgeInsert> = ops[sub_start..sub_end]
                            .iter()
                            .filter_map(|op| match op {
                                TxAction::AddEdge {
                                    id,
                                    src_id,
                                    dst_id,
                                    edge_type,
                                    properties,
                                    weight,
                                } => Some(crate::disk_graph::EdgeInsert {
                                    edge_id: *id,
                                    src_id: *src_id,
                                    dst_id: *dst_id,
                                    edge_type: edge_type.clone(),
                                    properties: properties.clone(),
                                    weight: *weight,
                                }),
                                _ => None,
                            })
                            .collect();

                        if let Err(e) = inner.disk_graph.insert_edges_batch(&batch) {
                            failed_err = Some(e);
                            break;
                        }
                        sub_start = sub_end;
                    }
                    if failed_err.is_some() {
                        break;
                    }
                    idx = end;
                    continue;
                }
            }

            let res = match &ops[idx] {
                TxAction::AddNode {
                    id,
                    labels,
                    properties,
                } => inner
                    .disk_graph
                    .insert_node_with_id_exact(*id, labels.clone(), properties.clone())
                    .map(|_| {
                        for l in labels {
                            inner.index_mgr.insert_label(l, *id);
                            inner.disk_graph.index_catalog.labels.insert(l.clone());
                            for (k, v) in properties {
                                inner.index_mgr.insert_property(l, k, v.clone(), *id);
                                inner
                                    .disk_graph
                                    .index_catalog
                                    .properties
                                    .insert((l.clone(), k.clone()));
                            }
                        }
                    }),
                TxAction::AddEdge {
                    id,
                    src_id,
                    dst_id,
                    edge_type,
                    properties,
                    weight,
                } => inner.disk_graph.insert_edge_with_id_exact(
                    *id,
                    *src_id,
                    *dst_id,
                    edge_type,
                    properties.clone(),
                    *weight,
                ),
                TxAction::UpdateNodeProp { id, key, value } => {
                    let old_val = if let Ok(Some(n)) = inner.disk_graph.get_node(*id) {
                        n.get_prop(key).cloned()
                    } else {
                        None
                    };
                    let r = inner
                        .disk_graph
                        .update_node_property(*id, key.clone(), value.clone());
                    if r.is_ok() {
                        if let Ok(Some(n)) = inner.disk_graph.get_node(*id) {
                            for l in &n.labels {
                                if let Some(ref ov) = old_val {
                                    inner.index_mgr.remove_property(l, key, ov, *id);
                                }
                                inner.index_mgr.insert_property(l, key, value.clone(), *id);
                                inner.disk_graph.index_catalog.labels.insert(l.clone());
                                inner
                                    .disk_graph
                                    .index_catalog
                                    .properties
                                    .insert((l.clone(), key.clone()));
                            }
                        }
                    }
                    r
                }
                TxAction::UpdateEdgeProp { id, key, value } => inner
                    .disk_graph
                    .update_edge_property(*id, key.clone(), value.clone()),
                TxAction::RemoveNode { id } => match inner.disk_graph.remove_node(*id) {
                    Ok(node) => {
                        let labels_bt: std::collections::BTreeSet<String> =
                            node.labels.into_iter().collect();
                        inner
                            .index_mgr
                            .remove_node_all_indices(*id, &labels_bt, &node.properties);
                        Ok(())
                    }
                    Err(e) => Err(e),
                },
                TxAction::RemoveEdge { id } => inner.disk_graph.remove_edge(*id).map(|_| ()),
            };

            if let Err(e) = res {
                failed_err = Some(e);
                break;
            }
            idx += 1;
        }

        if let Some(err) = failed_err {
            // 失败事务清理：丢弃未提交页（按基线还原内容与 WAL 位置索引）、
            // 回拨内存元数据、并使二级索引整体失效，确保零残留、主库零污染。
            let failed_pages = inner.disk_graph.drain_modified_pages();
            {
                let mut bpm = inner.disk_graph.bpm.lock_recover();
                let _ = bpm.rollback_uncommitted_pages(&failed_pages);
            }
            inner.disk_graph.restore_meta(&snapshot);
            inner.index_mgr.invalidate_all();
            return Err(err);
        }

        inner.disk_graph.sync_header()?;
        GraphLite::commit_dirty_pages_to_wal(&mut inner, self.tx_id)?;
        drop(inner);

        self.committed = true;
        // 写锁已释放，此时才能安全做 Checkpoint（大事务尤其需要这条路径）
        self.db.maybe_auto_checkpoint()?;
        Ok(())
    }

    /// 回滚事务：彻底清空未提交动作（全局自增序列保持单调递增，不回拨）
    pub fn rollback(mut self) -> Result<(), GraphError> {
        if self.committed {
            return Ok(());
        }

        self.revert_internal();
        self.committed = true;
        Ok(())
    }

    fn revert_internal(&mut self) {
        self.ops.clear();
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.committed {
            self.revert_internal();
        }
    }
}
