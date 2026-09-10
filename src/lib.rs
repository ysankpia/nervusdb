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
pub use page::{EdgeRecord, NodeRecord, PageId, PAGE_SIZE};
pub use query::{GraphQuery, MultiHopPath, PathMatch, QueryBuilder, QueryResult};
pub use storage::{StorageEngine, WalRecord};

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
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
const FRAMES_PER_MB: usize = 256;

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

    /// 以 MB 为单位打开图数据库（frames = mb × 256，下限 2 帧）。
    ///
    /// 便捷构造器：`open_with_pool_mb(path, 1)` 等价于 256 帧（1MB），
    /// `open_with_pool_mb(path, 16)` 等价于 4096 帧（16MB）。
    pub fn open_with_pool_mb<P: AsRef<Path>>(path: P, mb: usize) -> Result<Self, GraphError> {
        Self::open_with_pool_size(path, (mb * FRAMES_PER_MB).max(2))
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
        let mut bpm = BufferPoolManager::new(disk_manager, pool_size.max(2));
        // 挂载页级 WAL：缓冲池据此支持 STEAL 溢出与未提交页安全置换
        bpm.attach_wal(Arc::clone(storage.wal_writer()));
        let bpm = Arc::new(Mutex::new(bpm));
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
        Ok(result)
    }

    /// 执行检查点 Checkpoint：将 WAL 中所有已提交物理页落回主文件，刷出常驻脏页并截断 WAL。
    ///
    /// 未提交事务的溢出帧不会被重放，因此检查点绝不会把任何未提交数据写入主库。
    pub fn checkpoint(&self) -> Result<(), GraphError> {
        let inner = self
            .inner
            .write()
            .map_err(|e| GraphError::General(e.to_string()))?;

        // 1. 把 WAL 中已提交的页按序重放到主数据文件
        inner.storage.apply_committed_to_db()?;

        // 2. 刷出常驻缓冲池的已提交脏页
        inner.disk_graph.flush()?;

        // 3. 主文件已成为全部页的权威副本，WAL 页位置索引失效并截断 WAL
        {
            let mut bpm = inner.disk_graph.bpm.lock().unwrap();
            bpm.clear_wal_page_index();
        }
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

    /// 内部辅助：把当前事务修改的物理页原子提交到页级 WAL。
    ///
    /// 只有常驻缓冲池的脏页需要追加 redo 帧；早先因缓冲池耗尽而被 STEAL 溢出到 WAL 的
    /// 非常驻页，其最新镜像已在 WAL 中。整个流程以 O(1) 内存完成，绝不随事务规模线性膨胀。
    fn commit_dirty_pages_to_wal(inner: &mut GraphInner, tx_id: u64) -> Result<(), GraphError> {
        let modified_pages = inner.disk_graph.drain_modified_pages();
        let mut bpm = inner.disk_graph.bpm.lock().unwrap();
        bpm.begin_tx(tx_id);
        bpm.commit_tx(tx_id, &modified_pages)
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
            let inner = self.inner.read().expect("Lock poisoned");
            inner.disk_graph.clone()
        };
        algo::pagerank(&graph, damping_factor, max_iterations, tolerance)
    }

    /// 弱连通分量分析（并查集划分社群 / 孤岛检测），按分量规模降序返回
    pub fn weakly_connected_components(&self) -> Vec<Vec<u64>> {
        let graph = {
            let inner = self.inner.read().expect("Lock poisoned");
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
            let inner = self.inner.read().expect("Lock poisoned");
            inner.disk_graph.clone()
        };
        algo::k_hop_subgraph(&graph, start_id, k, direction, edge_type)
    }

    /// 获取图模式中的全部节点标签
    pub fn labels(&self) -> Vec<String> {
        let inner = self.inner.read().expect("Lock poisoned");
        inner.index_mgr.indexed_labels()
    }

    /// 获取图模式中的全部关系类型（以 DiskGraph 的持久化目录为权威来源）
    pub fn edge_types(&self) -> Vec<String> {
        let inner = self.inner.read().expect("Lock poisoned");
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
            let mut bpm = inner.disk_graph.bpm.lock().unwrap();
            bpm.begin_tx(self.tx_id);
        }

        let mut failed_err = None;
        let ops: Vec<TxAction> = self.ops.drain(..).collect();

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
                    let batch: Vec<crate::disk_graph::EdgeInsert> = ops[idx..end]
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
                let mut bpm = inner.disk_graph.bpm.lock().unwrap();
                let _ = bpm.rollback_uncommitted_pages(&failed_pages);
            }
            inner.disk_graph.restore_meta(&snapshot);
            inner.index_mgr.invalidate_all();
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
