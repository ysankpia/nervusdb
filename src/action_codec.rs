//! `TxAction` 的字节编解码：供事务动作溢出（spill）到 WAL 使用。
//!
//! ## 为什么单独一个模块
//!
//! 这段字节**会进入磁盘**（WAL 帧载荷），因此和 `page.rs` 的属性编码、`codec.rs`
//! 的字节原语是同一类东西：改动它就是改动磁盘格式。把它放在自己的模块里，是为了
//! 让「动作的磁盘表示」成为一处可审查的完整定义，而不是散在 `lib.rs` 的事务逻辑
//! 中间。
//!
//! ## 为什么是**无损**编码
//!
//! 本项目自家的属性编码（`page::encode_props`）**拒绝** `Null` 与 `List`：它们不可
//! 落盘，所以那里报错是正确的。但动作编解码不能借用那套语义——一个带着 `Null`
//! 属性的动作在**提交**时才会被拒绝，如果溢出时就编码失败，错误就会提前到
//! `add_node` 调用点。
//!
//! 那是一个真实的语义变化：调用方会看到「入队失败」而不是「提交失败」，而事务
//! 的原子性保证是围绕提交失败构建的。因此这里自己编码 `Value` 的全部变体（含
//! `Null` 与 `List`），保证**溢出永远成功**，错误仍在提交时的 apply 阶段报出，
//! 与不溢出时逐位一致。
//!
//! ## 布局
//!
//! 小端。`tag:u8` 后接各变体字段，变长字段用 `codec::Writer` 的 `u32` 长度前缀。
//! `Value` 用 `vtag:u8` 区分。标签数值是格式的一部分，**永不重用**。

use crate::codec::{Reader, Writer};
use crate::graph::{GraphError, Value};
use crate::TxAction;
use std::collections::{HashMap, HashSet};

/// `TxAction` 变体标签。数值是磁盘格式的一部分，新增只能追加。
pub mod action_tag {
    pub const ADD_NODE: u8 = 1;
    pub const ADD_EDGE: u8 = 2;
    pub const UPDATE_NODE_PROP: u8 = 3;
    pub const UPDATE_EDGE_PROP: u8 = 4;
    pub const REMOVE_NODE: u8 = 5;
    pub const REMOVE_EDGE: u8 = 6;
}

/// `Value` 标签。与 `page.rs` 的 `VTAG_*` **不共享数值**：那是属性页的编码，
/// 这是动作载荷的编码，两者独立演进。共用数值会让一处的扩展意外改变另一处的含义。
mod vtag {
    pub const INT: u8 = 1;
    pub const FLOAT: u8 = 2;
    pub const STRING: u8 = 3;
    pub const BOOL: u8 = 4;
    pub const NULL: u8 = 5;
    pub const LIST: u8 = 6;
}

fn write_value(w: &mut Writer, v: &Value) {
    match v {
        Value::Int(i) => {
            w.u8(vtag::INT);
            w.u64(*i as u64);
        }
        Value::Float(f) => {
            w.u8(vtag::FLOAT);
            w.u64(f.to_bits());
        }
        Value::String(s) => {
            w.u8(vtag::STRING);
            w.string(s);
        }
        Value::Bool(b) => {
            w.u8(vtag::BOOL);
            w.u8(u8::from(*b));
        }
        Value::Null => {
            w.u8(vtag::NULL);
        }
        Value::List(items) => {
            w.u8(vtag::LIST);
            w.u32(items.len() as u32);
            for item in items {
                write_value(w, item);
            }
        }
    }
}

fn read_value(r: &mut Reader<'_>) -> Result<Value, GraphError> {
    let tag = r.u8()?;
    let v = match tag {
        vtag::INT => Value::Int(r.u64()? as i64),
        vtag::FLOAT => Value::Float(f64::from_bits(r.u64()?)),
        vtag::STRING => Value::String(r.string()?),
        vtag::BOOL => Value::Bool(r.u8()? != 0),
        vtag::NULL => Value::Null,
        vtag::LIST => {
            let n = r.u32()? as usize;
            // `n` 来自磁盘上的长度前缀，可能损坏。**先确认剩余字节够再分配**：
            // 每个元素至少 1 字节（vtag），因此 `n > remaining` 必然不可能成立。
            // 不设界就 `Vec::with_capacity(n)`，一个 4 字节的坏值就能要求分配数 GB。
            if n > r.remaining() {
                return Err(GraphError::SerializationError(format!(
                    "action payload declares {n} list element(s) but only {} byte(s) remain",
                    r.remaining()
                )));
            }
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push(read_value(r)?);
            }
            Value::List(items)
        }
        other => {
            return Err(GraphError::SerializationError(format!(
                "unknown action value tag {other}"
            )))
        }
    };
    Ok(v)
}

fn write_props(w: &mut Writer, props: &HashMap<String, Value>) {
    // 键排序：同一份数据每次编码得到相同字节，便于比对与调试（与 `encode_props` 同一理由）
    let mut keys: Vec<&String> = props.keys().collect();
    keys.sort_unstable();
    w.u32(keys.len() as u32);
    for k in keys {
        w.string(k);
        write_value(w, &props[k]);
    }
}

fn read_props(r: &mut Reader<'_>) -> Result<HashMap<String, Value>, GraphError> {
    let n = r.u32()? as usize;
    // 同上：每项至少一个长度前缀（≥4 字节）加一个值标签（1 字节）
    if n.saturating_mul(5) > r.remaining() {
        return Err(GraphError::SerializationError(format!(
            "action payload declares {n} propert(y|ies) but only {} byte(s) remain",
            r.remaining()
        )));
    }
    let mut props = HashMap::with_capacity(n);
    for _ in 0..n {
        let k = r.string()?;
        props.insert(k, read_value(r)?);
    }
    Ok(props)
}

impl TxAction {
    /// 编码为自包含的字节串（供 WAL 溢出帧载荷使用）。
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(64);
        match self {
            TxAction::AddNode {
                id,
                labels,
                properties,
            } => {
                w.u8(action_tag::ADD_NODE);
                w.u64(*id);
                w.u32(labels.len() as u32);
                // 标签排序，理由同属性键
                let mut ls: Vec<&String> = labels.iter().collect();
                ls.sort_unstable();
                for l in ls {
                    w.string(l);
                }
                write_props(&mut w, properties);
            }
            TxAction::AddEdge {
                id,
                src_id,
                dst_id,
                edge_type,
                properties,
                weight,
            } => {
                w.u8(action_tag::ADD_EDGE);
                w.u64(*id);
                w.u64(*src_id);
                w.u64(*dst_id);
                w.string(edge_type);
                w.u64(weight.to_bits());
                write_props(&mut w, properties);
            }
            TxAction::UpdateNodeProp { id, key, value } => {
                w.u8(action_tag::UPDATE_NODE_PROP);
                w.u64(*id);
                w.string(key);
                write_value(&mut w, value);
            }
            TxAction::UpdateEdgeProp { id, key, value } => {
                w.u8(action_tag::UPDATE_EDGE_PROP);
                w.u64(*id);
                w.string(key);
                write_value(&mut w, value);
            }
            TxAction::RemoveNode { id } => {
                w.u8(action_tag::REMOVE_NODE);
                w.u64(*id);
            }
            TxAction::RemoveEdge { id } => {
                w.u8(action_tag::REMOVE_EDGE);
                w.u64(*id);
            }
        }
        w.finish()
    }

    /// 从 WAL 溢出帧载荷解码。任何畸形输入都返回 `Err`，绝不 panic。
    pub fn decode(buf: &[u8]) -> Result<TxAction, GraphError> {
        let mut r = Reader::new(buf);
        let tag = r.u8()?;
        let action = match tag {
            action_tag::ADD_NODE => {
                let id = r.u64()?;
                let n = r.u32()? as usize;
                // 每个标签至少 4 字节长度前缀
                if n.saturating_mul(4) > r.remaining() {
                    return Err(GraphError::SerializationError(format!(
                        "action declares {n} label(s) but only {} byte(s) remain",
                        r.remaining()
                    )));
                }
                let mut labels = HashSet::with_capacity(n);
                for _ in 0..n {
                    labels.insert(r.string()?);
                }
                let properties = read_props(&mut r)?;
                TxAction::AddNode {
                    id,
                    labels,
                    properties,
                }
            }
            action_tag::ADD_EDGE => {
                let id = r.u64()?;
                let src_id = r.u64()?;
                let dst_id = r.u64()?;
                let edge_type = r.string()?;
                let weight = f64::from_bits(r.u64()?);
                let properties = read_props(&mut r)?;
                TxAction::AddEdge {
                    id,
                    src_id,
                    dst_id,
                    edge_type,
                    properties,
                    weight,
                }
            }
            action_tag::UPDATE_NODE_PROP => {
                let id = r.u64()?;
                let key = r.string()?;
                let value = read_value(&mut r)?;
                TxAction::UpdateNodeProp { id, key, value }
            }
            action_tag::UPDATE_EDGE_PROP => {
                let id = r.u64()?;
                let key = r.string()?;
                let value = read_value(&mut r)?;
                TxAction::UpdateEdgeProp { id, key, value }
            }
            action_tag::REMOVE_NODE => TxAction::RemoveNode { id: r.u64()? },
            action_tag::REMOVE_EDGE => TxAction::RemoveEdge { id: r.u64()? },
            other => {
                return Err(GraphError::SerializationError(format!(
                    "unknown action tag {other}"
                )))
            }
        };
        // 拒绝尾部垃圾：编码器从不留多余字节，有残余说明数据被篡改或损坏。
        if !r.is_exhausted() {
            return Err(GraphError::SerializationError(format!(
                "action tag {tag} has {} trailing byte(s)",
                r.remaining()
            )));
        }
        Ok(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(a: &TxAction) {
        let bytes = a.encode();
        let back = TxAction::decode(&bytes).expect("decode must succeed");
        assert_eq!(&back, a, "round trip changed the action");
    }

    #[test]
    fn round_trips_every_variant() {
        let mut props = HashMap::new();
        props.insert("name".to_string(), Value::String("青云门".to_string()));
        props.insert("age".to_string(), Value::Int(-7));
        props.insert("score".to_string(), Value::Float(1.5));
        props.insert("ok".to_string(), Value::Bool(true));
        props.insert("gone".to_string(), Value::Null);
        props.insert(
            "tags".to_string(),
            Value::List(vec![Value::Int(1), Value::String("x".to_string())]),
        );

        round_trip(&TxAction::AddNode {
            id: 42,
            labels: HashSet::from(["A".to_string(), "B".to_string()]),
            properties: props.clone(),
        });
        round_trip(&TxAction::AddEdge {
            id: 7,
            src_id: 1,
            dst_id: 2,
            edge_type: "R".to_string(),
            properties: props,
            weight: 2.25,
        });
        round_trip(&TxAction::UpdateNodeProp {
            id: 1,
            key: "k".to_string(),
            value: Value::Null,
        });
        round_trip(&TxAction::UpdateEdgeProp {
            id: 2,
            key: "k".to_string(),
            value: Value::List(vec![Value::Bool(false)]),
        });
        round_trip(&TxAction::RemoveNode { id: 3 });
        round_trip(&TxAction::RemoveEdge { id: 4 });
    }

    /// `Null` 与 `List` 必须能被编码——这正是本模块不借用 `encode_props` 的理由。
    /// 若这里失败，溢出就会把「提交时才报错」提前成「入队时报错」。
    #[test]
    fn encodes_values_that_cannot_be_stored_as_properties() {
        let mut props = HashMap::new();
        props.insert("n".to_string(), Value::Null);
        let bytes = TxAction::AddNode {
            id: 1,
            labels: HashSet::new(),
            properties: props,
        }
        .encode();
        assert!(TxAction::decode(&bytes).is_ok());
    }

    #[test]
    fn truncated_input_errors_instead_of_panicking() {
        let bytes = TxAction::AddNode {
            id: 1,
            labels: HashSet::from(["A".to_string()]),
            properties: HashMap::new(),
        }
        .encode();
        // 逐个前缀都必须报错，绝不 panic
        for cut in 0..bytes.len() {
            let _ = TxAction::decode(&bytes[..cut]);
        }
    }

    #[test]
    fn oversized_list_count_is_rejected() {
        let mut w = Writer::new();
        w.u8(action_tag::ADD_NODE);
        w.u64(1);
        w.u32(0); // 无标签
        w.u32(1); // 一个属性
        w.string("k");
        w.u8(vtag::LIST);
        w.u32(u32::MAX); // 谎报 40 亿个元素
        let bytes = w.finish();
        assert!(
            TxAction::decode(&bytes).is_err(),
            "an absurd list count must be rejected before allocating"
        );
    }

    #[test]
    fn unknown_tag_and_trailing_bytes_are_rejected() {
        assert!(TxAction::decode(&[0xFF]).is_err(), "unknown tag");

        let mut bytes = TxAction::RemoveEdge { id: 1 }.encode();
        bytes.push(0);
        assert!(
            TxAction::decode(&bytes).is_err(),
            "trailing bytes must be rejected"
        );
    }
}
