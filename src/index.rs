use crate::graph::{GraphError, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

type PropIndexMap = HashMap<(String, String), BTreeMap<Value, BTreeSet<u64>>>;

/// 索引完整性状态标记（防止未构建的半成品索引冒充权威结果截断历史数据）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStatus {
    Registered, // 已注册元数据，但冷重启后尚未全量加载
    Complete,   // 已全量构建，权威可信
}

/// 唯一约束：`(label, prop)` 组合的取值在全部该标签节点中必须唯一。
///
/// ## 为什么需要它
///
/// 这是**数据不脏**的最后一道防线。没有约束时，一个有 bug 的写入方（或一个
/// 重试逻辑出错的 Agent）可以给同一个实体建两个节点，而查询只会返回其中一半
/// 数据——错误被延迟到很久以后才被发现。
///
/// ## 与索引的关系
///
/// 约束检查复用既有的 `(label, prop)` 属性索引，不额外维护数据结构：
/// 索引本身就把「值 → 节点集合」建好了，违例检测就是看目标值对应的集合是否
/// 已包含别的节点。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UniqueConstraint {
    pub label: String,
    pub prop: String,
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
/// unique_count:u32      unique_count × ( label_len:u32 + label,
///                                        prop_len:u32  + prop  )
/// ```
#[derive(Debug, Clone, Default)]
pub struct IndexCatalog {
    pub labels: BTreeSet<String>,
    pub properties: BTreeSet<(String, String)>,
    /// 图模式中的关系类型清单（供 `.schema` 与管理工具展示）
    pub edge_types: BTreeSet<String>,
    /// 唯一约束清单
    pub unique_constraints: BTreeSet<UniqueConstraint>,
}

impl IndexCatalog {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = crate::codec::Writer::with_capacity(
            self.labels.len() * 16
                + self.properties.len() * 32
                + self.edge_types.len() * 16
                + self.unique_constraints.len() * 32
                + 16,
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
        w.u32(self.unique_constraints.len() as u32);
        for c in &self.unique_constraints {
            w.string(&c.label);
            w.string(&c.prop);
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

        let n = r.u32()? as usize;
        if n.saturating_mul(8) > r.remaining() {
            return Err(GraphError::SerializationError(format!(
                "index catalog claims {} unique constraint(s) but only {} byte(s) remain",
                n,
                r.remaining()
            )));
        }
        let mut unique_constraints = BTreeSet::new();
        for _ in 0..n {
            let label = r.string()?;
            let prop = r.string()?;
            unique_constraints.insert(UniqueConstraint { label, prop });
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
            unique_constraints,
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

    /// 声明一个唯一约束。
    ///
    /// **不做既有数据校验**：调用方应先确认现有数据不违反约束（`check_unique_violation`），
    /// 否则约束会在下一次写入时才炸出来，而那时用户已经不知道是历史数据的问题。
    /// 库为空或调用方已检查时可直接声明。
    pub fn declare_unique(&mut self, label: &str, prop: &str) {
        self.catalog.unique_constraints.insert(UniqueConstraint {
            label: label.to_string(),
            prop: prop.to_string(),
        });
    }

    /// 当前全部唯一约束
    pub fn unique_constraints(&self) -> &BTreeSet<UniqueConstraint> {
        &self.catalog.unique_constraints
    }

    /// 在写入前检查唯一约束：返回违反约束的 `(label, prop, value)` 描述。
    ///
    /// `exclude` 是要排除的节点 ID（更新已有节点的属性时传自身，避免自己和自己冲突）。
    ///
    /// ## 判定依据
    ///
    /// 复用 `(label, prop)` 属性索引：它已经把「值 → 节点集合」建好，因此违例
    /// 就是「该值对应的集合里存在别的节点」。索引不完整时**拒绝写入**而不是
    /// 放行——放行会让约束形同虚设，而「拒绝」至少是可见且可恢复的。
    pub fn check_unique_violation(
        &self,
        label: &str,
        prop: &str,
        value: &Value,
        exclude: Option<u64>,
    ) -> Option<(String, String, String)> {
        let constrained = self
            .catalog
            .unique_constraints
            .iter()
            .any(|c| c.label == label && c.prop == prop);
        if !constrained {
            return None;
        }

        // 索引不可用时不能保证唯一性：返回违例让写入失败，
        // 而不是乐观放行（那会静默破坏约束）
        if !self.is_label_complete(label) {
            return Some((
                label.to_string(),
                prop.to_string(),
                format!(
                    "{:?} (label index for :{} is not built; cannot verify uniqueness)",
                    value, label
                ),
            ));
        }

        let existing = self
            .prop_index
            .get(&(label.to_string(), prop.to_string()))
            .and_then(|m| m.get(value));

        if let Some(nodes) = existing {
            let conflict = nodes.iter().find(|nid| Some(**nid) != exclude).copied();
            if let Some(nid) = conflict {
                return Some((
                    label.to_string(),
                    prop.to_string(),
                    format!("{:?} (already held by node {})", value, nid),
                ));
            }
        }
        None
    }

    /// 写入前的唯一约束闸门：任何写入路径都必须先过这里。
    ///
    /// ## 为什么放在 `IndexManager` 上
    ///
    /// 约束状态（`catalog.unique_constraints`）与判定所需的索引都在这里，因此这是
    /// 唯一一个「三条写入路径都够得着」的位置。
    ///
    /// 此前这个检查只存在于 `NervusDb::add_node` / `update_node_property` 两个
    /// Rust API 入口上，而 `Cypher CREATE`（`execute_create`、`apply_create_clause`）
    /// 与 `Transaction::commit` 都直接调用 `DiskGraph::add_node`，于是**绕过了约束**：
    /// 实测声明 `(:C {name})` 唯一后，`CREATE (x:C {name:'林渊'})` 会静默插入第二个
    /// 同名节点，事务路径同样。
    ///
    /// ## 索引未建立时按需重建，而不是拒绝写入
    ///
    /// 初版在索引不可用时直接返回违例，理由是「无法验证唯一性就不该放行」。那个
    /// 判断的**方向**是对的，**手段**是错的：`invalidate_all()`（事务失败时调用）
    /// 会把所有标签降级为 `Registered`，于是失败一次之后，该标签上的**任何**写入
    /// 都会被永久拒绝——包括完全合法的取值。实测表现为「写完一个重名被拒之后，
    /// 连不重名的也写不进去了」。
    ///
    /// 现在改为按需重建：需要索引就先建，建完再判定。代价是一次全标签扫描，但只在
    /// 索引失效后的首次约束检查上发生，且换来的是「约束既不放过重复，也不误杀合法
    /// 写入」——这正是约束应有的语义。
    pub fn guard_unique_constraints(
        &mut self,
        graph: &crate::disk_graph::DiskGraph,
        labels: &std::collections::HashSet<String>,
        properties: &std::collections::HashMap<String, Value>,
        exclude: Option<u64>,
    ) -> Result<(), GraphError> {
        if self.catalog.unique_constraints.is_empty() {
            return Ok(());
        }

        // 只有「本次写入涉及的标签」才需要索引，避免为无关标签付扫描代价
        for label in labels {
            let constrained = self
                .catalog
                .unique_constraints
                .iter()
                .any(|c| &c.label == label);
            if constrained && !self.is_label_complete(label) {
                self.ensure_label_index(graph, label);
            }
        }

        for label in labels {
            for (prop, value) in properties {
                if let Some((l, p, detail)) =
                    self.check_unique_violation(label, prop, value, exclude)
                {
                    return Err(GraphError::UniqueConstraintViolation {
                        label: l,
                        prop: p,
                        detail,
                    });
                }
            }
        }
        Ok(())
    }

    /// 检查**指定的** `(label, prop)` 在现有数据上是否已有重复值。
    ///
    /// 返回每个重复取值及其节点集合；空表示可以安全声明约束。
    ///
    /// ## 为什么必须接受「候选约束」而不是只查已声明的
    ///
    /// 声明约束时它尚未进入 `catalog.unique_constraints`。若只遍历已声明的约束，
    /// 这个函数永远返回「无重复」——既有脏数据会被静默接受，直到下一次写入才以
    /// 一条指向错误位置的错误炸出来。这正是它初版的行为，实测被抓出来。
    pub fn find_duplicates_for(&self, label: &str, prop: &str) -> Vec<(Value, Vec<u64>)> {
        let mut found = Vec::new();
        if let Some(map) = self.prop_index.get(&(label.to_string(), prop.to_string())) {
            for (value, nodes) in map {
                if nodes.len() > 1 {
                    found.push((value.clone(), nodes.iter().copied().collect()));
                }
            }
        }
        found
    }

    /// 检查某标签的**全部已声明**唯一约束在现有数据上是否满足。
    ///
    /// 与 [`Self::find_duplicates_for`] 的分工：这个用于**巡检**已生效的约束，
    /// 那个用于**声明前**验证候选约束。
    pub fn find_duplicates_for_label(
        &self,
        label: &str,
    ) -> Vec<(UniqueConstraint, Value, Vec<u64>)> {
        let mut found = Vec::new();
        for c in self
            .catalog
            .unique_constraints
            .iter()
            .filter(|c| c.label == label)
        {
            for (value, nodes) in self.find_duplicates_for(&c.label, &c.prop) {
                found.push((c.clone(), value, nodes));
            }
        }
        found
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
}
