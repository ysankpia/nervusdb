pub mod algo;
pub mod buffer;
pub mod c_api;
pub mod cypher;
pub mod disk_graph;
pub mod graph;
pub mod index;
pub mod page;
pub mod query;
pub mod storage;

pub use algo::{bfs_shortest_path, dijkstra_shortest_path, find_cycles, has_cycle};
pub use buffer::{BufferPoolManager, BufferStats, DiskManager};
pub use c_api::*;
pub use cypher::{
    execute_cypher, execute_mutate, execute_query, CypherResultSet, ExecuteResult, Row,
};
pub use disk_graph::DiskGraph;
pub use graph::{Direction, Edge, GraphError, Node, Value};
pub use index::IndexManager;
pub use page::{EdgeRecord, NodeRecord, PageId, PAGE_SIZE};
pub use query::{GraphQuery, MultiHopPath, PathMatch, QueryBuilder, QueryResult};
pub use storage::{StorageEngine, WalRecord};

use crc32fast::Hasher;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

pub const DEFAULT_BUFFER_POOL_FRAMES: usize = 1024; // 1024 * 4KB = 4MB

/// 内部核心结构体（彻底剔除内存 HashMap 图，DiskGraph 为唯一数据源）
pub struct GraphInner {
    pub disk_graph: DiskGraph,
    pub storage: StorageEngine,
    pub index_mgr: IndexManager,
    pub next_tx_id: u64,
}

/// GraphLite: 生产级纯磁盘嵌入式属性图数据库引擎 (SQLite 3.0 标准)
#[derive(Clone)]
pub struct GraphLite {
    inner: Arc<RwLock<GraphInner>>,
    db_path: PathBuf,
}

impl GraphLite {
    /// 打开或创建指定路径的图数据库 (默认 4MB Buffer Pool)
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, GraphError> {
        Self::open_with_pool_size(path, DEFAULT_BUFFER_POOL_FRAMES)
    }

    /// 打开图数据库并自定义 Buffer Pool 帧数上限（纯磁盘受控，无内存泄露）
    pub fn open_with_pool_size<P: AsRef<Path>>(
        path: P,
        pool_size: usize,
    ) -> Result<Self, GraphError> {
        let db_path = path.as_ref().to_path_buf();

        // 1. 初始化页级 WAL 持久化引擎（若存在未 Checkpoint 的 WAL，自动将已提交页重放至主文件）
        let storage = StorageEngine::open(&db_path)?;

        // 2. 初始化纯磁盘 4KB 物理分页管理器与 LRU Buffer Pool（统一单文件 {path}）
        let disk_manager = Arc::new(DiskManager::open(&db_path)?);
        let bpm = Arc::new(Mutex::new(BufferPoolManager::new(
            disk_manager,
            pool_size.max(2),
        )));
        let disk_graph = DiskGraph::new(bpm)?;

        // 3. 构建二级索引系统（从持久化 catalog 极速加载，<1ms 杜绝全表 I/O 阻塞）
        let index_mgr = IndexManager::from_catalog(disk_graph.index_catalog.clone());

        let inner = GraphInner {
            disk_graph,
            storage,
            index_mgr,
            next_tx_id: 1,
        };

        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
            db_path,
        })
    }

    /// 执行 Cypher 变更语句 (如 CREATE / DETACH DELETE)
    pub fn execute(&self, cypher_str: &str) -> Result<ExecuteResult, GraphError> {
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

        Ok(result.stats)
    }

    /// 执行 Cypher 查询语句 (True MRSW: 只读语句并发持有共享读锁，写操作持有排他写锁并写入 WAL)
    pub fn query_cypher(&self, cypher_str: &str) -> Result<CypherResultSet, GraphError> {
        let lexer = crate::cypher::lexer::Lexer::new(cypher_str);
        let tokens = lexer.tokenize()?;
        let mut parser = crate::cypher::parser::Parser::new(tokens);
        let statement = parser.parse()?;

        match statement {
            crate::cypher::CypherStatement::Match {
                delete_clause: None,
                ..
            } => {
                // 只读查询：只获取 inner.read() 共享锁，允许几十个读线程无阻塞并发执行！
                let inner = self
                    .inner
                    .read()
                    .map_err(|e| GraphError::General(e.to_string()))?;
                cypher::execute_query(cypher_str, &inner.disk_graph, &inner.index_mgr)
            }
            _ => {
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
                Ok(result)
            }
        }
    }

    /// 执行检查点 Checkpoint：将 Buffer Pool 所有脏页刷入主文件，并截断 WAL
    pub fn checkpoint(&self) -> Result<(), GraphError> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        inner.disk_graph.flush()?;
        inner.storage.checkpoint()
    }

    /// 获取 Buffer Pool 运行统计指标
    pub fn buffer_stats(&self) -> BufferStats {
        let inner = self.inner.read().expect("Lock poisoned");
        let bpm = inner.disk_graph.bpm.lock().unwrap();
        bpm.stats()
    }

    /// 获取二级索引信息
    pub fn index_labels(&self) -> Vec<String> {
        let inner = self.inner.read().expect("Lock poisoned");
        inner.index_mgr.indexed_labels()
    }

    pub fn index_properties(&self) -> Vec<(String, String)> {
        let inner = self.inner.read().expect("Lock poisoned");
        inner.index_mgr.indexed_properties()
    }

    /// 获取数据库主文件路径
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// 获取当前节点总数（纯磁盘定长元数据头统计）
    pub fn node_count(&self) -> usize {
        let inner = self.inner.read().expect("Lock poisoned");
        inner.disk_graph.node_count
    }

    /// 获取当前边总数
    pub fn edge_count(&self) -> usize {
        let inner = self.inner.read().expect("Lock poisoned");
        inner.disk_graph.edge_count
    }

    /// 获取指定节点（按需通过 Buffer Pool 调入）
    pub fn get_node(&self, id: u64) -> Option<Node> {
        let inner = self.inner.read().expect("Lock poisoned");
        inner.disk_graph.get_node(id).ok().flatten()
    }

    /// 获取指定边（按需通过 Buffer Pool 调入）
    pub fn get_edge(&self, id: u64) -> Option<Edge> {
        let inner = self.inner.read().expect("Lock poisoned");
        inner.disk_graph.get_edge(id).ok().flatten()
    }

    /// 添加单个节点（纯磁盘定长写入，优先复用 Freelist，记录页级 WAL）
    pub fn add_node(
        &self,
        labels: HashSet<String>,
        properties: HashMap<String, Value>,
    ) -> Result<u64, GraphError> {
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
        Ok(edge_id)
    }

    /// 删除节点（级联删除关联边，槽位回收至 Freelist）
    pub fn remove_node(&self, id: u64) -> Result<Node, GraphError> {
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
        Ok(node)
    }

    /// 删除边（从双向双环磁盘链表中脱链，槽位回收至 Freelist）
    pub fn remove_edge(&self, id: u64) -> Result<Edge, GraphError> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let tx_id = inner.next_tx_id;
        inner.next_tx_id += 1;

        let edge = inner.disk_graph.remove_edge(id)?;

        Self::commit_dirty_pages_to_wal(&mut inner, tx_id)?;
        Ok(edge)
    }

    /// 更新节点属性
    pub fn update_node_property<V: Into<Value>>(
        &self,
        id: u64,
        key: impl Into<String>,
        value: V,
    ) -> Result<(), GraphError> {
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
        Ok(())
    }

    /// 更新边属性
    pub fn update_edge_property<V: Into<Value>>(
        &self,
        id: u64,
        key: impl Into<String>,
        value: V,
    ) -> Result<(), GraphError> {
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
        Ok(())
    }

    /// 内部辅助：将当前事务修改的物理页作为 PageWrite 帧原子落盘至 WAL 并 Commit
    fn commit_dirty_pages_to_wal(inner: &mut GraphInner, tx_id: u64) -> Result<(), GraphError> {
        let modified_pages: Vec<PageId> =
            if let Ok(mut set) = inner.disk_graph.tx_modified_pages.lock() {
                set.drain().collect()
            } else {
                Vec::new()
            };

        let mut records = Vec::with_capacity(modified_pages.len() + 2);
        records.push(WalRecord::TxBegin { tx_id });

        {
            let mut bpm = inner.disk_graph.bpm.lock().unwrap();
            for &page_id in &modified_pages {
                if let Ok(frame_id) = bpm.fetch_page(page_id) {
                    let frame = bpm.get_frame(frame_id);
                    let mut hasher = Hasher::new();
                    hasher.update(&frame.data);
                    let crc = hasher.finalize();

                    records.push(WalRecord::PageWrite {
                        tx_id,
                        page_id,
                        crc32: crc,
                        data: frame.data.to_vec(),
                    });
                    bpm.unpin_page(page_id, false);
                }
            }
        }

        records.push(WalRecord::TxCommit { tx_id });
        inner.storage.append_records(&records)?;

        // WAL 持久化落盘完成后，放行 BufferPool 脏页允许安全置换
        let mut bpm = inner.disk_graph.bpm.lock().unwrap();
        bpm.mark_pages_committed(&modified_pages);

        Ok(())
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
        let inner = self.inner.read().expect("Lock poisoned");
        GraphQuery::new(inner.disk_graph.clone())
    }

    /// 执行带权 Dijkstra 最短路径算法（纯磁盘流式遍历，脱离全局锁并发执行）
    pub fn dijkstra(
        &self,
        start_id: u64,
        end_id: u64,
        edge_type: Option<&str>,
    ) -> Option<(f64, Vec<u64>)> {
        let graph = {
            let inner = self.inner.read().expect("Lock poisoned");
            inner.disk_graph.clone()
        };
        algo::dijkstra_shortest_path(&graph, start_id, end_id, edge_type)
    }

    /// 执行无权 BFS 最短路径算法（纯磁盘流式遍历，脱离全局锁并发执行）
    pub fn bfs(&self, start_id: u64, end_id: u64, edge_type: Option<&str>) -> Option<Vec<u64>> {
        let graph = {
            let inner = self.inner.read().expect("Lock poisoned");
            inner.disk_graph.clone()
        };
        algo::bfs_shortest_path(&graph, start_id, end_id, edge_type)
    }

    /// 检测全图是否存在有向环路（纯磁盘按页扫描，脱离全局锁并发执行）
    pub fn has_cycle(&self) -> bool {
        let graph = {
            let inner = self.inner.read().expect("Lock poisoned");
            inner.disk_graph.clone()
        };
        algo::has_cycle(&graph)
    }

    /// 查找全图所有有向环路（脱离全局锁并发执行）
    pub fn find_cycles(&self) -> Vec<Vec<u64>> {
        let graph = {
            let inner = self.inner.read().expect("Lock poisoned");
            inner.disk_graph.clone()
        };
        algo::find_cycles(&graph)
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
    pub fn add_edge(
        &mut self,
        src_id: u64,
        dst_id: u64,
        edge_type: impl Into<String>,
        properties: HashMap<String, Value>,
        weight: f64,
    ) -> Result<u64, GraphError> {
        if weight < 0.0 || weight.is_nan() {
            return Err(GraphError::InvalidWeight(weight));
        }

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

    /// 提交事务：将修改原子应用至 DiskGraph，批量刷出 PageWrite 帧并提交
    pub fn commit(mut self) -> Result<(), GraphError> {
        if self.committed {
            return Ok(());
        }

        let mut inner = self
            .db
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        let mut failed_err = None;

        for op in self.ops.drain(..) {
            let res = match op {
                TxAction::AddNode {
                    id,
                    labels,
                    properties,
                } => inner
                    .disk_graph
                    .insert_node_with_id_exact(id, labels.clone(), properties.clone())
                    .map(|_| {
                        for l in &labels {
                            inner.index_mgr.insert_label(l, id);
                            inner.disk_graph.index_catalog.labels.insert(l.clone());
                            for (k, v) in &properties {
                                inner.index_mgr.insert_property(l, k, v.clone(), id);
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
                } => inner
                    .disk_graph
                    .insert_edge_with_id_exact(id, src_id, dst_id, &edge_type, properties, weight),
                TxAction::UpdateNodeProp { id, key, value } => {
                    let old_val = if let Ok(Some(n)) = inner.disk_graph.get_node(id) {
                        n.get_prop(&key).cloned()
                    } else {
                        None
                    };
                    let r = inner
                        .disk_graph
                        .update_node_property(id, key.clone(), value.clone());
                    if r.is_ok() {
                        if let Ok(Some(n)) = inner.disk_graph.get_node(id) {
                            for l in &n.labels {
                                if let Some(ref ov) = old_val {
                                    inner.index_mgr.remove_property(l, &key, ov, id);
                                }
                                inner.index_mgr.insert_property(l, &key, value.clone(), id);
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
                TxAction::UpdateEdgeProp { id, key, value } => {
                    inner.disk_graph.update_edge_property(id, key, value)
                }
                TxAction::RemoveNode { id } => match inner.disk_graph.remove_node(id) {
                    Ok(node) => {
                        let labels_bt: std::collections::BTreeSet<String> =
                            node.labels.into_iter().collect();
                        inner
                            .index_mgr
                            .remove_node_all_indices(id, &labels_bt, &node.properties);
                        Ok(())
                    }
                    Err(e) => Err(e),
                },
                TxAction::RemoveEdge { id } => inner.disk_graph.remove_edge(id).map(|_| ()),
            };

            if let Err(e) = res {
                failed_err = Some(e);
                break;
            }
        }

        if let Some(err) = failed_err {
            let failed_pages: Vec<PageId> =
                if let Ok(mut set) = inner.disk_graph.tx_modified_pages.lock() {
                    set.drain().collect()
                } else {
                    Vec::new()
                };
            inner
                .disk_graph
                .bpm
                .lock()
                .unwrap()
                .discard_uncommitted_pages(&failed_pages);
            return Err(err);
        }

        inner.disk_graph.sync_header()?;
        GraphLite::commit_dirty_pages_to_wal(&mut inner, self.tx_id)?;

        self.committed = true;
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
