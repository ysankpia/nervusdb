use crate::graph::Value;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

type PropIndexMap = HashMap<(String, String), BTreeMap<Value, BTreeSet<u64>>>;

/// 索引完整性状态标记（防止未构建的半成品索引冒充权威结果截断历史数据）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStatus {
    Registered, // 已注册元数据，但冷重启后尚未全量加载
    Complete,   // 已全量构建，权威可信
}

/// 二级索引持久化目录元数据（对齐 SQLite 模式表架构）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexCatalog {
    pub labels: BTreeSet<String>,
    pub properties: BTreeSet<(String, String)>,
}

/// 二级索引管理器：支持 Label 索引与 (Label, PropKey) 属性索引加速
#[derive(Debug, Clone, Default)]
pub struct IndexManager {
    catalog: IndexCatalog,
    label_status: HashMap<String, IndexStatus>,

    /// 标签索引：Label -> 包含该标签的 NodeId 倒排有序集合
    label_index: HashMap<String, BTreeSet<u64>>,

    /// 属性索引：(Label, PropKey) -> BTreeMap<Value, BTreeSet<NodeId>>
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

    pub fn catalog(&self) -> &IndexCatalog {
        &self.catalog
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

    pub fn label_index_count(&self) -> usize {
        self.label_index.len()
    }

    pub fn property_index_count(&self) -> usize {
        self.prop_index.len()
    }
}
