use std::collections::BTreeMap;

/// Property value types for nodes and edges.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    DateTime(i64),
    Blob(Vec<u8>),
    List(Vec<PropertyValue>),
    Map(BTreeMap<String, PropertyValue>),
}

impl PropertyValue {
    /// Encode property value to bytes for WAL storage.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            PropertyValue::Null => vec![0],
            PropertyValue::Bool(b) => {
                let mut out = vec![1];
                out.push(if *b { 1 } else { 0 });
                out
            }
            PropertyValue::Int(i) => {
                let mut out = vec![2];
                out.extend_from_slice(&i.to_le_bytes());
                out
            }
            PropertyValue::Float(f) => {
                let mut out = vec![3];
                out.extend_from_slice(&f.to_le_bytes());
                out
            }
            PropertyValue::String(s) => {
                let mut out = vec![4];
                let bytes = s.as_bytes();
                let len = u32::try_from(bytes.len()).expect("string length should fit in u32");
                out.extend_from_slice(&len.to_le_bytes());
                out.extend_from_slice(bytes);
                out
            }
            PropertyValue::DateTime(i) => {
                let mut out = vec![5];
                out.extend_from_slice(&i.to_le_bytes());
                out
            }
            PropertyValue::Blob(b) => {
                let mut out = vec![6];
                let len = u32::try_from(b.len()).expect("blob length should fit in u32");
                out.extend_from_slice(&len.to_le_bytes());
                out.extend_from_slice(b);
                out
            }
            PropertyValue::List(l) => {
                let mut out = vec![7];
                let len = u32::try_from(l.len()).expect("list length should fit in u32");
                out.extend_from_slice(&len.to_le_bytes());
                for item in l {
                    out.extend_from_slice(&item.encode());
                }
                out
            }
            PropertyValue::Map(m) => {
                let mut out = vec![8];
                let len = u32::try_from(m.len()).expect("map length should fit in u32");
                out.extend_from_slice(&len.to_le_bytes());
                for (k, v) in m {
                    let k_bytes = k.as_bytes();
                    let k_len = u32::try_from(k_bytes.len()).expect("key length should fit in u32");
                    out.extend_from_slice(&k_len.to_le_bytes());
                    out.extend_from_slice(k_bytes);
                    out.extend_from_slice(&v.encode());
                }
                out
            }
        }
    }

    /// Decode property value from bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let (val, _) = Self::decode_recursive(bytes)?;
        Ok(val)
    }

    fn decode_recursive(bytes: &[u8]) -> Result<(Self, usize), DecodeError> {
        if bytes.is_empty() {
            return Err(DecodeError::Empty);
        }
        let ty = bytes[0];
        let mut pos = 1;
        match ty {
            0 => Ok((PropertyValue::Null, 1)),
            1 => {
                if bytes.len() < 2 {
                    return Err(DecodeError::InvalidLength);
                }
                Ok((PropertyValue::Bool(bytes[1] != 0), 2))
            }
            2 => {
                if bytes.len() < 9 {
                    return Err(DecodeError::InvalidLength);
                }
                let i = i64::from_le_bytes(bytes[1..9].try_into().unwrap());
                Ok((PropertyValue::Int(i), 9))
            }
            3 => {
                if bytes.len() < 9 {
                    return Err(DecodeError::InvalidLength);
                }
                let f = f64::from_le_bytes(bytes[1..9].try_into().unwrap());
                Ok((PropertyValue::Float(f), 9))
            }
            4 => {
                if bytes.len() < 5 {
                    return Err(DecodeError::InvalidLength);
                }
                let len = u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
                if bytes.len() < 5 + len {
                    return Err(DecodeError::InvalidLength);
                }
                let s = String::from_utf8(bytes[5..5 + len].to_vec())
                    .map_err(|_| DecodeError::InvalidUtf8)?;
                Ok((PropertyValue::String(s), 5 + len))
            }
            5 => {
                if bytes.len() < 9 {
                    return Err(DecodeError::InvalidLength);
                }
                let i = i64::from_le_bytes(bytes[1..9].try_into().unwrap());
                Ok((PropertyValue::DateTime(i), 9))
            }
            6 => {
                if bytes.len() < 5 {
                    return Err(DecodeError::InvalidLength);
                }
                let len = u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
                if bytes.len() < 5 + len {
                    return Err(DecodeError::InvalidLength);
                }
                Ok((PropertyValue::Blob(bytes[5..5 + len].to_vec()), 5 + len))
            }
            7 => {
                if bytes.len() < 5 {
                    return Err(DecodeError::InvalidLength);
                }
                let count = u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
                pos = 5;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    let (item, consumed) = Self::decode_recursive(&bytes[pos..])?;
                    items.push(item);
                    pos += consumed;
                }
                Ok((PropertyValue::List(items), pos))
            }
            8 => {
                if bytes.len() < 5 {
                    return Err(DecodeError::InvalidLength);
                }
                let count = u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
                pos = 5;
                let mut map = BTreeMap::new();
                for _ in 0..count {
                    if bytes.len() < pos + 4 {
                        return Err(DecodeError::InvalidLength);
                    }
                    let k_len =
                        u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
                    pos += 4;
                    if bytes.len() < pos + k_len {
                        return Err(DecodeError::InvalidLength);
                    }
                    let key = String::from_utf8(bytes[pos..pos + k_len].to_vec())
                        .map_err(|_| DecodeError::InvalidUtf8)?;
                    pos += k_len;
                    let (val, consumed) = Self::decode_recursive(&bytes[pos..])?;
                    map.insert(key, val);
                    pos += consumed;
                }
                Ok((PropertyValue::Map(map), pos))
            }
            _ => Err(DecodeError::UnknownType(ty)),
        }
    }
}

#[derive(Debug)]
pub enum DecodeError {
    Empty,
    InvalidLength,
    InvalidUtf8,
    UnknownType(u8),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Empty => write!(f, "empty property value bytes"),
            DecodeError::InvalidLength => write!(f, "invalid property value length"),
            DecodeError::InvalidUtf8 => write!(f, "invalid UTF-8 in string property"),
            DecodeError::UnknownType(ty) => write!(f, "unknown property value type: {}", ty),
        }
    }
}

impl std::error::Error for DecodeError {}

impl PropertyValue {
    /// Get float value if this is a Float variant.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            PropertyValue::Float(f) => Some(*f),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_null() {
        let v = PropertyValue::Null;
        let encoded = v.encode();
        let decoded = PropertyValue::decode(&encoded).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn encode_decode_bool() {
        for b in [true, false] {
            let v = PropertyValue::Bool(b);
            let encoded = v.encode();
            let decoded = PropertyValue::decode(&encoded).unwrap();
            assert_eq!(v, decoded);
        }
    }

    #[test]
    fn encode_decode_int() {
        for i in [0i64, -1, 1, i64::MIN, i64::MAX] {
            let v = PropertyValue::Int(i);
            let encoded = v.encode();
            let decoded = PropertyValue::decode(&encoded).unwrap();
            assert_eq!(v, decoded);
        }
    }

    #[test]
    fn encode_decode_float() {
        for f in [
            0.0f64,
            -1.0,
            1.0,
            f64::MIN,
            f64::MAX,
            f64::NAN,
            f64::INFINITY,
        ] {
            let v = PropertyValue::Float(f);
            let encoded = v.encode();
            let decoded = PropertyValue::decode(&encoded).unwrap();
            // NaN and Infinity need special handling
            if f.is_nan() {
                assert!(decoded.as_float().unwrap().is_nan());
            } else if f.is_infinite() {
                assert_eq!(f.is_infinite(), decoded.as_float().unwrap().is_infinite());
            } else {
                assert_eq!(v, decoded);
            }
        }
    }

    #[test]
    fn encode_decode_datetime() {
        let v = PropertyValue::DateTime(123456789);
        let encoded = v.encode();
        let decoded = PropertyValue::decode(&encoded).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn encode_decode_blob() {
        let v = PropertyValue::Blob(vec![0x00, 0xFF, 0xDE, 0xAD, 0xBE, 0xEF]);
        let encoded = v.encode();
        let decoded = PropertyValue::decode(&encoded).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn encode_decode_list() {
        let v = PropertyValue::List(vec![
            PropertyValue::Int(1),
            PropertyValue::String("hello".into()),
            PropertyValue::List(vec![PropertyValue::Bool(true)]),
        ]);
        let encoded = v.encode();
        let decoded = PropertyValue::decode(&encoded).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn encode_decode_map() {
        let mut map = BTreeMap::new();
        map.insert("a".into(), PropertyValue::Int(1));
        map.insert("b".into(), PropertyValue::List(vec![PropertyValue::Null]));
        let v = PropertyValue::Map(map);
        let encoded = v.encode();
        let decoded = PropertyValue::decode(&encoded).unwrap();
        assert_eq!(v, decoded);
    }
}
