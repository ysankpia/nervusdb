use crate::graph::{GraphError, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

type PropIndexMap = HashMap<(String, String), BTreeMap<Value, BTreeSet<u64>>>;

/// 索引完整性状态标记（防止未构建的半成品索引冒充权威结果截断历史数据）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStatus {
    Registered, // 已注册元数据，但冷重启后尚未全量加载
    Complete,   // 已全量构建，权威可信
}

/// 二级索引持久化目录元数据（对齐 SQLite 模式表架构）
///
/// 磁盘布局见 `FORMAT.md`：
///
/// ```text
/// labels_count:u32      labels_count × (len:u32 + UTF-8)
/// props_count:u32       props_count  × ( label_len:u32 + label,
///                                        key_len:u32   + key   )
/// edge_types_count:u32  edge_types_count × (len:u32 + UTF-8)
/// ```
#[derive(Debug, Clone, Default)]
pub struct IndexCatalog {
    pub labels: BTreeSet<String>,
    pub properties: BTreeSet<(String, String)>,
    /// 图模式中的关系类型清单（供 `.schema` 与管理工具展示）
    pub edge_types: BTreeSet<String>,
}

impl IndexCatalog {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = crate::codec::Writer::with_capacity(
            self.labels.len() * 16 + self.properties.len() * 32 + self.edge_types.len() * 16 + 12,
        );
        w.u32(self.labels.len() as u32);
        for l in &self.labels {
            w.string(l);
        }
        w.u32(self.properties.len() as u32);
        for (l, k) in &self.properties {
            w.string(l);
            w.string(k);
        }
        w.u32(self.edge_types.len() as u32);
        for t in &self.edge_types {
            w.string(t);
        }
        w.finish()
    }

    pub fn decode(buf: &[u8]) -> Result<IndexCatalog, GraphError> {
        let mut r = crate::codec::Reader::new(buf);

        // 每个集合成员至少 4 字节（长度前缀），据此拒绝荒谬的计数，
        // 避免用损坏的计数预分配巨量内存。
        let n = r.u32()? as usize;
        if n.saturating_mul(4) > r.remaining() {
            return Err(GraphError::SerializationError(format!(
                "index catalog claims {} label(s) but only {} byte(s) remain",
                n,
                r.remaining()
            )));
        }
        let mut labels = BTreeSet::new();
        for _ in 0..n {
            labels.insert(r.string()?);
        }

        let n = r.u32()? as usize;
        if n.saturating_mul(8) > r.remaining() {
            return Err(GraphError::SerializationError(format!(
                "index catalog claims {} propert(y|ies) but only {} byte(s) remain",
                n,
                r.remaining()
            )));
        }
        let mut properties = BTreeSet::new();
        for _ in 0..n {
            let l = r.string()?;
            let k = r.string()?;
            properties.insert((l, k));
        }

        let n = r.u32()? as usize;
        if n.saturating_mul(4) > r.remaining() {
            return Err(GraphError::SerializationError(format!(
                "index catalog claims {} edge type(s) but only {} byte(s) remain",
                n,
                r.remaining()
            )));
        }
        let mut edge_types = BTreeSet::new();
        for _ in 0..n {
            edge_types.insert(r.string()?);
        }

        if !r.is_exhausted() {
            return Err(GraphError::SerializationError(format!(
                "index catalog has {} trailing byte(s)",
                r.remaining()
            )));
        }

        Ok(IndexCatalog {
            labels,
            properties,
            edge_types,
        })
    }
}

/// 二级索引管理器：支持 Label 索引与 (Label, PropKey) 属性索引加速
#[derive(Debug, Clone, Default)]
pub struct IndexManager {
    catalog: IndexCatalog,
    label_status: HashMap<String, IndexStatus>,

    /// 标签索引：Label -> 包含该标签的 NodeId 倒排有序集合
    label_index: HashMap<String, BTreeSet<u64>>,

    /// 属性索引：`(Label, PropKey) -> BTreeMap<Value, BTreeSet<NodeId>>`
    /// 支持精确等值查找与有序范围检索
    prop_index: PropIndexMap,
}

impl IndexManager {
    pub fn new() -> Self {
        Self {
            catalog: IndexCatalog::default(),
            label_status: HashMap::new(),
            label_index: HashMap::new(),
            prop_index: HashMap::new(),
        }
    }

    pub fn from_catalog(catalog: IndexCatalog) -> Self {
        let mut label_status = HashMap::new();
        for l in &catalog.labels {
            label_status.insert(l.clone(), IndexStatus::Registered);
        }
        Self {
            catalog,
            label_status,
            label_index: HashMap::new(),
            prop_index: HashMap::new(),
        }
    }

    /// 检查指定标签的索引是否权威完整
    pub fn is_label_complete(&self, label: &str) -> bool {
        self.label_status.get(label) == Some(&IndexStatus::Complete)
    }

    /// 按需全量构建并完成指定标签及其属性的索引
    pub fn ensure_label_index(&mut self, graph: &crate::disk_graph::DiskGraph, label: &str) {
        if self.is_label_complete(label) {
            return;
        }
        // 重建前清除该标签的陈旧属性索引项，防止旧值索引残留造成幻读
        let stale_keys: Vec<(String, String)> = self
            .prop_index
            .keys()
            .filter(|(l, _)| l == label)
            .cloned()
            .collect();
        for key in stale_keys {
            self.prop_index.remove(&key);
        }
        if let Some(set) = self.label_index.get_mut(label) {
            set.clear();
        }

        if let Ok(node_ids) = graph.all_node_ids() {
            let mut set = BTreeSet::new();
            for nid in node_ids {
                if let Ok(Some(node)) = graph.get_node(nid) {
                    if node.has_label(label) {
                        set.insert(nid);
                        for (k, v) in &node.properties {
                            let idx_k = (label.to_string(), k.clone());
                            self.prop_index
                                .entry(idx_k)
                                .or_default()
                                .entry(v.clone())
                                .or_default()
                                .insert(nid);
                        }
                    }
                }
            }
            self.label_index.insert(label.to_string(), set);
            self.label_status
                .insert(label.to_string(), IndexStatus::Complete);
        }
    }

    /// 索引整体失效（事务失败/回滚后调用）：降级为 Registered，后续按需从 DiskGraph 重建。
    /// 这保证了失败事务绝不会留下任何可被查询观察到的索引残留。
    pub fn invalidate_all(&mut self) {
        self.label_index.clear();
        self.prop_index.clear();
        for status in self.label_status.values_mut() {
            *status = IndexStatus::Registered;
        }
    }

    /// 为指定节点注册标签
    pub fn insert_label(&mut self, label: &str, node_id: u64) {
        if label.is_empty() {
            return;
        }
        self.catalog.labels.insert(label.to_string());
        self.label_status
            .entry(label.to_string())
            .or_insert(IndexStatus::Complete);
        self.label_index
            .entry(label.to_string())
            .or_default()
            .insert(node_id);
    }

    /// 为指定节点注销标签
    pub fn remove_label(&mut self, label: &str, node_id: u64) {
        if let Some(set) = self.label_index.get_mut(label) {
            set.remove(&node_id);
        }
    }

    /// 标签索引点查：O(1) 获取具有特定 Label 的所有 NodeId
    pub fn find_by_label(&self, label: &str) -> Option<&BTreeSet<u64>> {
        self.label_index.get(label)
    }

    /// 为指定节点注册属性索引
    pub fn insert_property(&mut self, label: &str, key: &str, value: Value, node_id: u64) {
        if label.is_empty() || key.is_empty() {
            return;
        }
        self.catalog.labels.insert(label.to_string());
        self.catalog
            .properties
            .insert((label.to_string(), key.to_string()));
        let index_key = (label.to_string(), key.to_string());
        self.prop_index
            .entry(index_key)
            .or_default()
            .entry(value)
            .or_default()
            .insert(node_id);
    }

    /// 注销属性索引
    pub fn remove_property(&mut self, label: &str, key: &str, value: &Value, node_id: u64) {
        let index_key = (label.to_string(), key.to_string());
        if let Some(map) = self.prop_index.get_mut(&index_key) {
            if let Some(set) = map.get_mut(value) {
                set.remove(&node_id);
                if set.is_empty() {
                    map.remove(value);
                }
            }
        }
    }

    /// 属性索引精确点查：O(log K) 获取匹配 (Label, Key == Value) 的 NodeId 集合
    pub fn find_by_property_exact(
        &self,
        label: &str,
        key: &str,
        value: &Value,
    ) -> Option<&BTreeSet<u64>> {
        let index_key = (label.to_string(), key.to_string());
        self.prop_index
            .get(&index_key)
            .and_then(|map| map.get(value))
    }

    /// 属性索引范围检索：例如 key > val
    pub fn find_by_property_greater_than(
        &self,
        label: &str,
        key: &str,
        min_val: &Value,
        inclusive: bool,
    ) -> BTreeSet<u64> {
        let index_key = (label.to_string(), key.to_string());
        let mut result = BTreeSet::new();

        if let Some(map) = self.prop_index.get(&index_key) {
            for (val, set) in map.range(min_val..) {
                if !inclusive && val == min_val {
                    continue;
                }
                result.extend(set.iter().copied());
            }
        }
        result
    }

    /// 节点完全移除时清理所有索引条目
    pub fn remove_node_all_indices(
        &mut self,
        node_id: u64,
        labels: &BTreeSet<String>,
        props: &HashMap<String, Value>,
    ) {
        for label in labels {
            self.remove_label(label, node_id);
            for (k, v) in props {
                self.remove_property(label, k, v, node_id);
            }
        }
    }

    /// 获取所有已建立索引的标签列表（持久化 catalog 与内存合集）
    pub fn indexed_labels(&self) -> Vec<String> {
        let mut set = self.catalog.labels.clone();
        set.extend(self.label_index.keys().cloned());
        set.into_iter().collect()
    }

    /// 获取所有属性索引键对
    pub fn indexed_properties(&self) -> Vec<(String, String)> {
        let mut set = self.catalog.properties.clone();
        set.extend(self.prop_index.keys().cloned());
        set.into_iter().collect()
    }

    /// 获取图模式中的关系类型清单
    pub fn edge_types(&self) -> Vec<String> {
        self.catalog.edge_types.iter().cloned().collect()
    }

    pub fn label_index_count(&self) -> usize {
        self.label_index.len()
    }

    pub fn property_index_count(&self) -> usize {
        self.prop_index.len()
    }
}
