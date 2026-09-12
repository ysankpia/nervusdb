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
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

impl Value {
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

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
            (Value::Int(a), Value::Float(b)) => (*a as f64).total_cmp(b),
            (Value::Float(a), Value::Int(b)) => a.total_cmp(&(*b as f64)),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Int(_), _) => std::cmp::Ordering::Less,
            (_, Value::Int(_)) => std::cmp::Ordering::Greater,
            (Value::Float(_), _) => std::cmp::Ordering::Less,
            (_, Value::Float(_)) => std::cmp::Ordering::Greater,
            (Value::String(_), _) => std::cmp::Ordering::Less,
            (_, Value::String(_)) => std::cmp::Ordering::Greater,
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
            Value::Int(v) => write!(f, "{}", v),
            Value::Float(v) => write!(f, "{}", v),
            Value::String(v) => write!(f, "\"{}\"", v),
            Value::Bool(v) => write!(f, "{}", v),
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
