use std::collections::{HashMap, HashSet};
use std::fmt;

/// 统一的错误类型。
///
/// `Display` 与 `std::error::Error` 均手写实现（不再派生 `thiserror`），
/// 使依赖树保持为空。文案属于**面向用户的接口**：一旦发布就应当保持稳定，
/// 因此每条消息都写清楚「哪里错了」和「该怎么办」。
#[derive(Debug)]
pub enum GraphError {
    NodeNotFound(u64),

    EdgeNotFound(u64),

    InvalidWeight(f64),

    TransactionError(String),

    IoError(std::io::Error),

    SerializationError(String),

    StorageError(String),

    WalCorrupted(String),

    DatabaseLocked(String),

    IntegrityError(String),

    PageChecksumMismatch {
        page_id: u64,
        expected: u32,
        actual: u32,
    },

    /// 违反唯一约束：`(:Label {prop})` 的取值已存在于另一个节点。
    ///
    /// 独立变体而非 `General`：调用方需要能程序化区分「数据违反约束」与
    /// 「其它一般性失败」，前者是可预期的业务结果，后者通常意味着 bug。
    UniqueConstraintViolation {
        label: String,
        prop: String,
        detail: String,
    },

    General(String),
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::NodeNotFound(id) => write!(f, "Node not found: {}", id),
            GraphError::EdgeNotFound(id) => write!(f, "Edge not found: {}", id),
            GraphError::InvalidWeight(w) => {
                write!(f, "Invalid weight: {}, weight must be non-negative", w)
            }
            GraphError::TransactionError(msg) => write!(f, "Transaction error: {}", msg),
            GraphError::IoError(e) => write!(f, "Storage I/O error: {}", e),
            GraphError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            GraphError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            GraphError::WalCorrupted(msg) => write!(f, "WAL corrupted: {}", msg),
            GraphError::DatabaseLocked(msg) => write!(f, "Database locked: {}", msg),
            GraphError::IntegrityError(msg) => write!(f, "Integrity check failed: {}", msg),
            GraphError::PageChecksumMismatch {
                page_id,
                expected,
                actual,
            } => write!(
                f,
                "Page checksum mismatch at page {}: expected {:#010x}, actual {:#010x}",
                page_id, expected, actual
            ),
            GraphError::UniqueConstraintViolation {
                label,
                prop,
                detail,
            } => write!(
                f,
                "Unique constraint violated: (:{}.{}) = {}. \
                 Each value must be unique across all :{} nodes.",
                label, prop, detail, label
            ),
            GraphError::General(msg) => write!(f, "General database error: {}", msg),
        }
    }
}

impl std::error::Error for GraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // 只有 IO 错误有底层 cause；其余都是自包含的诊断信息
        match self {
            GraphError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

/// `?` 在 `std::io::Error` 上的自动转换（原 `#[from]` 的等价物）。
///
/// 手写而非派生：转换只此一条，且必须显式，否则 IO 错误会在调用点悄悄
/// 变成 `General`，丢掉 `IoError` 这一可判别类型。
impl From<std::io::Error> for GraphError {
    fn from(e: std::io::Error) -> Self {
        GraphError::IoError(e)
    }
}

/// 属性图动态值类型
///
/// ## `Null` 与 `List` 是**求值期类型**，不落盘
///
/// 这两个变体只存在于查询执行过程中，`PropCodec` 不编码它们：
///
/// - **`Null`**：Cypher 语义中「属性值为 null」等价于「该属性不存在」，因此
///   `SET n.k = null` 会**删除**属性而不是写入一个 null 值。没有东西需要落盘。
/// - **`List`**：`UNWIND [1,2,3] AS x` 这类列表是查询里的中间值。把它存进属性
///   需要一套嵌套类型的编解码，而当前没有任何语法能产生「属性值是列表」——
///   因此暂不支持，遇到时明确报错而不是静默丢弃。
///
/// 这样 `FORMAT.md` 描述的磁盘布局**不需要任何改动**，格式版本仍是 4。
///
/// ## `Null` 为什么必须是真的变体
///
/// 此前 null 用字符串 `"null"` 冒充，于是**真的叫 "null" 的值会被当成空值**：
///
/// ```text
/// CREATE (c:Character {name: 'null'})   -- 用户确实写了这个值
/// MATCH (c) RETURN count(c.name)        -- 返回 0：值被静默忽略
/// ```
///
/// 任何 `count` / `sum` / `avg` 都会漏掉这类数据。用独立变体后二者不再混淆。
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// SQL/Cypher 意义上的 NULL（求值期专用，不落盘）
    Null,
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    /// 同构值的有序集合（求值期专用，不落盘）
    List(Vec<Value>),
}

impl Value {
    /// 是否为 null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// 构造 null
    pub fn null() -> Self {
        Value::Null
    }

    /// 是否为列表
    pub fn is_list(&self) -> bool {
        matches!(self, Value::List(_))
    }

    /// 该值是否可以写入属性存储。
    ///
    /// `Null` 与 `List` 不行：前者按 Cypher 语义表示「删除该属性」，后者需要一套
    /// 嵌套编解码而当前没有语法能产生它。调用方必须在写入前用它把关，**不能
    /// 静默丢弃**——静默丢弃会让 `SET` 看起来成功而数据没变。
    pub fn is_storable(&self) -> bool {
        !matches!(self, Value::Null | Value::List(_))
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Float(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(v) => Some(v.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for Value {}

/// 跨类型排序时的类型次序。
///
/// 显式排序而不是靠 `match` 的臂顺序兜底：后者在新增变体时极易漏改（编译器只要求
/// 穷尽匹配，不会提醒「你忘了给新类型定位置」）。数值类型共用一个 rank，因为
/// `Int` 与 `Float` 之间按数值比较而不是按类型分先后。
fn type_rank(v: &Value) -> u8 {
    match v {
        Value::Int(_) | Value::Float(_) => 0,
        Value::String(_) => 1,
        Value::Bool(_) => 2,
        Value::List(_) => 3,
        // null 排在最后（SQL 的 NULLS LAST 约定）。`ORDER BY` 依赖这一点；
        // 而 `min`/`max` 会在聚合层先滤掉 null，不受此影响。
        Value::Null => 4,
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // 同秩才做值比较；跨秩只看类型次序
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
            (Value::Int(a), Value::Float(b)) => (*a as f64).total_cmp(b),
            (Value::Float(a), Value::Int(b)) => a.total_cmp(&(*b as f64)),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            // 列表按字典序比较：逐元素比，先到尽头的更小
            (Value::List(a), Value::List(b)) => {
                for (x, y) in a.iter().zip(b.iter()) {
                    let ord = x.cmp(y);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
                a.len().cmp(&b.len())
            }
            (Value::Null, Value::Null) => Ordering::Equal,
            _ => type_rank(self).cmp(&type_rank(other)),
        }
    }
}

impl PartialEq<i64> for Value {
    fn eq(&self, other: &i64) -> bool {
        self.as_i64() == Some(*other)
    }
}

impl PartialOrd<i64> for Value {
    fn partial_cmp(&self, other: &i64) -> Option<std::cmp::Ordering> {
        self.as_i64().and_then(|v| v.partial_cmp(other))
    }
}

impl PartialEq<i32> for Value {
    fn eq(&self, other: &i32) -> bool {
        self.as_i64() == Some(*other as i64)
    }
}

impl PartialOrd<i32> for Value {
    fn partial_cmp(&self, other: &i32) -> Option<std::cmp::Ordering> {
        self.as_i64().and_then(|v| v.partial_cmp(&(*other as i64)))
    }
}

impl PartialEq<f64> for Value {
    fn eq(&self, other: &f64) -> bool {
        self.as_f64() == Some(*other)
    }
}

impl PartialOrd<f64> for Value {
    fn partial_cmp(&self, other: &f64) -> Option<std::cmp::Ordering> {
        self.as_f64().and_then(|v| v.partial_cmp(other))
    }
}

impl PartialEq<&str> for Value {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(v as i64)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Float(v as f64)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // 键盘输入里 `null` 就是字面量 `null`（不带引号），而字符串是带引号的
            // `"null"`。二者在显示上也必须能区分，否则用户无法判断属性值到底是
            // 空值还是那个字符串。
            Value::Null => write!(f, "null"),
            Value::Int(v) => write!(f, "{}", v),
            Value::Float(v) => write!(f, "{}", v),
            Value::String(v) => write!(f, "\"{}\"", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
        }
    }
}

/// 节点模型：免索引邻接直接包含 outgoing/incoming EdgeId 列表
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: u64,
    pub labels: HashSet<String>,
    pub properties: HashMap<String, Value>,
    pub outgoing: Vec<u64>,
    pub incoming: Vec<u64>,
}

impl Node {
    pub fn new(id: u64, labels: HashSet<String>, properties: HashMap<String, Value>) -> Self {
        Self {
            id,
            labels,
            properties,
            outgoing: Vec::new(),
            incoming: Vec::new(),
        }
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.labels.contains(label)
    }

    pub fn get_prop(&self, key: &str) -> Option<&Value> {
        self.properties.get(key)
    }

    pub fn set_prop<V: Into<Value>>(&mut self, key: impl Into<String>, value: V) {
        self.properties.insert(key.into(), value.into());
    }

    pub fn add_label(&mut self, label: impl Into<String>) {
        self.labels.insert(label.into());
    }

    pub fn remove_label(&mut self, label: &str) -> bool {
        self.labels.remove(label)
    }
}

/// 边模型：包含源、目的节点、类型、权重和动态属性
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub id: u64,
    pub src_id: u64,
    pub dst_id: u64,
    pub edge_type: String,
    pub properties: HashMap<String, Value>,
    pub weight: f64,
}

impl Edge {
    pub fn new(
        id: u64,
        src_id: u64,
        dst_id: u64,
        edge_type: String,
        properties: HashMap<String, Value>,
        weight: f64,
    ) -> Self {
        Self {
            id,
            src_id,
            dst_id,
            edge_type,
            properties,
            weight,
        }
    }

    pub fn get_prop(&self, key: &str) -> Option<&Value> {
        self.properties.get(key)
    }

    pub fn set_prop<V: Into<Value>>(&mut self, key: impl Into<String>, value: V) {
        self.properties.insert(key.into(), value.into());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}
