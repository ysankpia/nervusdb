use crate::property::PropertyValue;

/// Encode an index key component so that lexicographic byte order matches value order.
///
/// This is intentionally *not* `PropertyValue::encode()` because that format is for
/// WAL storage, not ordered comparisons (it uses little-endian numbers).
///
/// Total ordering across types:
/// `Null < Bool < Int < Float < String`.
///
/// Strings are encoded with byte-stuffing so embedded `\0` does not break ordering:
/// - `0x00` byte becomes `0x00 0xFF`
/// - terminator is `0x00 0x00`
pub fn encode_ordered_value(v: &PropertyValue) -> Vec<u8> {
    match v {
        PropertyValue::Null => vec![0x00],
        PropertyValue::Bool(b) => vec![0x01, u8::from(*b)],
        PropertyValue::Int(i) => {
            let mut out = Vec::with_capacity(1 + 8);
            out.push(0x02);
            let u = (*i as u64) ^ 0x8000_0000_0000_0000;
            out.extend_from_slice(&u.to_be_bytes());
            out
        }
        PropertyValue::Float(f) => {
            let mut out = Vec::with_capacity(1 + 8);
            out.push(0x03);
            let bits = f.to_bits();
            let sortable = if (bits & (1 << 63)) != 0 {
                // Negative numbers sort before positives: invert all bits.
                !bits
            } else {
                // Positive numbers: flip sign bit.
                bits ^ (1 << 63)
            };
            out.extend_from_slice(&sortable.to_be_bytes());
            out
        }
        PropertyValue::String(s) => {
            let mut out = Vec::with_capacity(1 + s.len() + 2);
            out.push(0x04);
            for &b in s.as_bytes() {
                if b == 0x00 {
                    out.push(0x00);
                    out.push(0xFF);
                } else {
                    out.push(b);
                }
            }
            out.push(0x00);
            out.push(0x00);
            out
        }
        PropertyValue::DateTime(i) => {
            let mut out = Vec::with_capacity(1 + 8);
            out.push(0x05);
            let u = (*i as u64) ^ 0x8000_0000_0000_0000;
            out.extend_from_slice(&u.to_be_bytes());
            out
        }
        PropertyValue::Blob(b) => {
            let mut out = Vec::with_capacity(1 + b.len() + 2);
            out.push(0x06);
            for &byte in b {
                if byte == 0x00 {
                    out.push(0x00);
                    out.push(0xFF);
                } else {
                    out.push(byte);
                }
            }
            out.push(0x00);
            out.push(0x00);
            out
        }
        PropertyValue::List(_) => {
            // For MVP: Lists only sort by tag. Full sorting is complex.
            vec![0x07]
        }
        PropertyValue::Map(_) => {
            // For MVP: Maps only sort by tag.
            vec![0x08]
        }
    }
}

/// Composite key used by the B+Tree:
/// `[index_id: u32 BE][ordered_value][internal_node_id: u64 BE]`.
pub fn encode_index_key(index_id: u32, v: &PropertyValue, internal_node_id: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 1 + 8 + 8);
    out.extend_from_slice(&index_id.to_be_bytes());
    out.extend_from_slice(&encode_ordered_value(v));
    out.extend_from_slice(&internal_node_id.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_ordered(values: Vec<PropertyValue>) {
        let encoded: Vec<Vec<u8>> = values.iter().map(encode_ordered_value).collect();
        let mut sorted = encoded.clone();
        sorted.sort();
        assert_eq!(encoded, sorted, "ordered encoding does not preserve order");
    }

    #[test]
    fn ordered_null_bool_int_float_string() {
        assert_ordered(vec![
            PropertyValue::Null,
            PropertyValue::Bool(false),
            PropertyValue::Bool(true),
            PropertyValue::Int(i64::MIN),
            PropertyValue::Int(-1),
            PropertyValue::Int(0),
            PropertyValue::Int(1),
            PropertyValue::Int(i64::MAX),
            PropertyValue::Float(f64::NEG_INFINITY),
            PropertyValue::Float(-1.0),
            PropertyValue::Float(-0.0),
            PropertyValue::Float(0.0),
            PropertyValue::Float(1.0),
            PropertyValue::Float(f64::INFINITY),
            PropertyValue::String("".into()),
            PropertyValue::String("A".into()),
            PropertyValue::String("B".into()),
            PropertyValue::String("a".into()),
            PropertyValue::String("aa".into()),
        ]);
    }

    #[test]
    fn ordered_string_with_nul_byte() {
        let a = PropertyValue::String("a".into());
        let a_nul = PropertyValue::String("a\0".into());
        let a_nul_x = PropertyValue::String("a\0x".into());
        let b = PropertyValue::String("b".into());

        let encoded = vec![
            encode_ordered_value(&a),
            encode_ordered_value(&a_nul),
            encode_ordered_value(&a_nul_x),
            encode_ordered_value(&b),
        ];
        let mut sorted = encoded.clone();
        sorted.sort();
        assert_eq!(encoded, sorted);
    }

    #[test]
    fn composite_key_orders_by_index_then_value_then_id() {
        let k1 = encode_index_key(1, &PropertyValue::Int(7), 10);
        let k2 = encode_index_key(1, &PropertyValue::Int(7), 11);
        let k3 = encode_index_key(1, &PropertyValue::Int(8), 1);
        let k4 = encode_index_key(2, &PropertyValue::Int(0), 0);

        let keys = vec![k1.clone(), k2.clone(), k3.clone(), k4.clone()];
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);

        assert!(k1 < k2);
        assert!(k2 < k3);
        assert!(k3 < k4);
    }
}
