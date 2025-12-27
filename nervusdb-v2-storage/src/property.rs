/// Property value types for nodes and edges.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
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
        }
    }

    /// Decode property value from bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.is_empty() {
            return Err(DecodeError::Empty);
        }
        let ty = bytes[0];
        let payload = &bytes[1..];
        match ty {
            0 => Ok(PropertyValue::Null),
            1 => {
                if payload.len() != 1 {
                    return Err(DecodeError::InvalidLength);
                }
                Ok(PropertyValue::Bool(payload[0] != 0))
            }
            2 => {
                if payload.len() != 8 {
                    return Err(DecodeError::InvalidLength);
                }
                let i = i64::from_le_bytes(payload[0..8].try_into().unwrap());
                Ok(PropertyValue::Int(i))
            }
            3 => {
                if payload.len() != 8 {
                    return Err(DecodeError::InvalidLength);
                }
                let f = f64::from_le_bytes(payload[0..8].try_into().unwrap());
                Ok(PropertyValue::Float(f))
            }
            4 => {
                if payload.len() < 4 {
                    return Err(DecodeError::InvalidLength);
                }
                let len = u32::from_le_bytes(payload[0..4].try_into().unwrap()) as usize;
                if payload.len() < 4 + len {
                    return Err(DecodeError::InvalidLength);
                }
                let s = String::from_utf8(payload[4..4 + len].to_vec())
                    .map_err(|_| DecodeError::InvalidUtf8)?;
                Ok(PropertyValue::String(s))
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
    fn encode_decode_string() {
        for s in ["", "hello", "世界"] {
            let v = PropertyValue::String(s.to_string());
            let encoded = v.encode();
            let decoded = PropertyValue::decode(&encoded).unwrap();
            assert_eq!(v, decoded);
        }
        // Test long string separately
        let long_str = "a".repeat(1000);
        let v = PropertyValue::String(long_str.clone());
        let encoded = v.encode();
        let decoded = PropertyValue::decode(&encoded).unwrap();
        assert_eq!(PropertyValue::String(long_str), decoded);
    }
}
