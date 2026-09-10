use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(u64),

    #[error("Edge not found: {0}")]
    EdgeNotFound(u64),

    #[error("Invalid weight: {0}, weight must be non-negative")]
    InvalidWeight(f64),

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("Storage I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("WAL corrupted: {0}")]
    WalCorrupted(String),

    #[error("General database error: {0}")]
    General(String),
}

/// 属性图动态值类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// 内存属性图拓扑模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: HashMap<u64, Node>,
    pub edges: HashMap<u64, Edge>,
    pub next_node_id: u64,
    pub next_edge_id: u64,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            next_node_id: 1,
            next_edge_id: 1,
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn add_node(&mut self, labels: HashSet<String>, properties: HashMap<String, Value>) -> u64 {
        let id = self.next_node_id;
        self.next_node_id += 1;
        let node = Node::new(id, labels, properties);
        self.nodes.insert(id, node);
        id
    }

    pub fn insert_node_with_id(&mut self, node: Node) {
        if node.id >= self.next_node_id {
            self.next_node_id = node.id + 1;
        }
        self.nodes.insert(node.id, node);
    }

    pub fn get_node(&self, id: u64) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn get_node_mut(&mut self, id: u64) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    pub fn remove_node(&mut self, id: u64) -> Result<Node, GraphError> {
        let node = self.nodes.remove(&id).ok_or(GraphError::NodeNotFound(id))?;

        // 级联删除相关的出边和入边
        let mut edges_to_remove = Vec::with_capacity(node.outgoing.len() + node.incoming.len());
        edges_to_remove.extend_from_slice(&node.outgoing);
        edges_to_remove.extend_from_slice(&node.incoming);
        edges_to_remove.sort_unstable();
        edges_to_remove.dedup();

        for edge_id in edges_to_remove {
            if let Some(edge) = self.edges.remove(&edge_id) {
                // 如果是出边，从对端(dst)的 incoming 中移除该 edge_id
                if edge.dst_id != id {
                    if let Some(dst_node) = self.nodes.get_mut(&edge.dst_id) {
                        dst_node.incoming.retain(|&e_id| e_id != edge_id);
                    }
                }
                // 如果是入边，从对端(src)的 outgoing 中移除该 edge_id
                if edge.src_id != id {
                    if let Some(src_node) = self.nodes.get_mut(&edge.src_id) {
                        src_node.outgoing.retain(|&e_id| e_id != edge_id);
                    }
                }
            }
        }

        Ok(node)
    }

    pub fn add_edge(
        &mut self,
        src_id: u64,
        dst_id: u64,
        edge_type: String,
        properties: HashMap<String, Value>,
        weight: f64,
    ) -> Result<u64, GraphError> {
        if weight < 0.0 || weight.is_nan() {
            return Err(GraphError::InvalidWeight(weight));
        }
        if !self.nodes.contains_key(&src_id) {
            return Err(GraphError::NodeNotFound(src_id));
        }
        if !self.nodes.contains_key(&dst_id) {
            return Err(GraphError::NodeNotFound(dst_id));
        }

        let edge_id = self.next_edge_id;
        self.next_edge_id += 1;

        let edge = Edge::new(edge_id, src_id, dst_id, edge_type, properties, weight);
        self.edges.insert(edge_id, edge);

        // 免索引邻接更新
        if let Some(src_node) = self.nodes.get_mut(&src_id) {
            src_node.outgoing.push(edge_id);
        }
        if let Some(dst_node) = self.nodes.get_mut(&dst_id) {
            dst_node.incoming.push(edge_id);
        }

        Ok(edge_id)
    }

    pub fn insert_edge_with_id(&mut self, edge: Edge) -> Result<(), GraphError> {
        if edge.weight < 0.0 || edge.weight.is_nan() {
            return Err(GraphError::InvalidWeight(edge.weight));
        }
        if !self.nodes.contains_key(&edge.src_id) {
            return Err(GraphError::NodeNotFound(edge.src_id));
        }
        if !self.nodes.contains_key(&edge.dst_id) {
            return Err(GraphError::NodeNotFound(edge.dst_id));
        }

        if edge.id >= self.next_edge_id {
            self.next_edge_id = edge.id + 1;
        }

        let edge_id = edge.id;
        let src_id = edge.src_id;
        let dst_id = edge.dst_id;

        self.edges.insert(edge_id, edge);

        if let Some(src_node) = self.nodes.get_mut(&src_id) {
            if !src_node.outgoing.contains(&edge_id) {
                src_node.outgoing.push(edge_id);
            }
        }
        if let Some(dst_node) = self.nodes.get_mut(&dst_id) {
            if !dst_node.incoming.contains(&edge_id) {
                dst_node.incoming.push(edge_id);
            }
        }

        Ok(())
    }

    pub fn get_edge(&self, id: u64) -> Option<&Edge> {
        self.edges.get(&id)
    }

    pub fn get_edge_mut(&mut self, id: u64) -> Option<&mut Edge> {
        self.edges.get_mut(&id)
    }

    pub fn remove_edge(&mut self, id: u64) -> Result<Edge, GraphError> {
        let edge = self.edges.remove(&id).ok_or(GraphError::EdgeNotFound(id))?;

        if let Some(src_node) = self.nodes.get_mut(&edge.src_id) {
            src_node.outgoing.retain(|&e_id| e_id != id);
        }
        if let Some(dst_node) = self.nodes.get_mut(&edge.dst_id) {
            dst_node.incoming.retain(|&e_id| e_id != id);
        }

        Ok(edge)
    }

    pub fn update_node_property(
        &mut self,
        id: u64,
        key: String,
        value: Value,
    ) -> Result<(), GraphError> {
        let node = self
            .nodes
            .get_mut(&id)
            .ok_or(GraphError::NodeNotFound(id))?;
        node.properties.insert(key, value);
        Ok(())
    }

    pub fn update_edge_property(
        &mut self,
        id: u64,
        key: String,
        value: Value,
    ) -> Result<(), GraphError> {
        let edge = self
            .edges
            .get_mut(&id)
            .ok_or(GraphError::EdgeNotFound(id))?;
        edge.properties.insert(key, value);
        Ok(())
    }

    pub fn outgoing_edges(&self, node_id: u64) -> Result<Vec<&Edge>, GraphError> {
        let node = self
            .nodes
            .get(&node_id)
            .ok_or(GraphError::NodeNotFound(node_id))?;
        let edges = node
            .outgoing
            .iter()
            .filter_map(|&e_id| self.edges.get(&e_id))
            .collect();
        Ok(edges)
    }

    pub fn incoming_edges(&self, node_id: u64) -> Result<Vec<&Edge>, GraphError> {
        let node = self
            .nodes
            .get(&node_id)
            .ok_or(GraphError::NodeNotFound(node_id))?;
        let edges = node
            .incoming
            .iter()
            .filter_map(|&e_id| self.edges.get(&e_id))
            .collect();
        Ok(edges)
    }

    pub fn neighbors(&self, node_id: u64, direction: Direction) -> Result<Vec<u64>, GraphError> {
        let node = self
            .nodes
            .get(&node_id)
            .ok_or(GraphError::NodeNotFound(node_id))?;
        let mut neighbors = Vec::new();

        match direction {
            Direction::Outgoing => {
                for &e_id in &node.outgoing {
                    if let Some(edge) = self.edges.get(&e_id) {
                        neighbors.push(edge.dst_id);
                    }
                }
            }
            Direction::Incoming => {
                for &e_id in &node.incoming {
                    if let Some(edge) = self.edges.get(&e_id) {
                        neighbors.push(edge.src_id);
                    }
                }
            }
            Direction::Both => {
                for &e_id in &node.outgoing {
                    if let Some(edge) = self.edges.get(&e_id) {
                        neighbors.push(edge.dst_id);
                    }
                }
                for &e_id in &node.incoming {
                    if let Some(edge) = self.edges.get(&e_id) {
                        neighbors.push(edge.src_id);
                    }
                }
            }
        }

        Ok(neighbors)
    }
}
