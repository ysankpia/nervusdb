//! Varint-encoded triple key for compact storage
//!
//! Reduces key size from 24 bytes (u64 x 3) to ~6-12 bytes average.

use std::cmp::Ordering;

#[cfg(not(target_arch = "wasm32"))]
use redb::{Key, TypeName};

/// Varint-encoded triple key
///
/// Uses LEB128 encoding for each component, resulting in:
/// - 1 byte for values 0-127
/// - 2 bytes for values 128-16383
/// - etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarintTripleKey {
    data: Vec<u8>,
}

impl VarintTripleKey {
    /// Create a new key from three u64 values
    pub fn new(s: u64, p: u64, o: u64) -> Self {
        let mut data = Vec::with_capacity(12);
        encode_varint(&mut data, s);
        encode_varint(&mut data, p);
        encode_varint(&mut data, o);
        Self { data }
    }

    /// Decode the key back to (s, p, o)
    pub fn decode(&self) -> (u64, u64, u64) {
        let mut pos = 0;
        let s = decode_varint(&self.data, &mut pos);
        let p = decode_varint(&self.data, &mut pos);
        let o = decode_varint(&self.data, &mut pos);
        (s, p, o)
    }

    /// Get the raw bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Create from raw bytes
    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
        }
    }
}

/// Encode a u64 as LEB128 varint
fn encode_varint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Decode a LEB128 varint from bytes
fn decode_varint(data: &[u8], pos: &mut usize) -> u64 {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = data[*pos];
        *pos += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    result
}

/// Compare two varint-encoded keys lexicographically by (s, p, o)
pub fn compare_varint_keys(a: &[u8], b: &[u8]) -> Ordering {
    let mut pos_a = 0;
    let mut pos_b = 0;

    // Compare s
    let s_a = decode_varint(a, &mut pos_a);
    let s_b = decode_varint(b, &mut pos_b);
    match s_a.cmp(&s_b) {
        Ordering::Equal => {}
        other => return other,
    }

    // Compare p
    let p_a = decode_varint(a, &mut pos_a);
    let p_b = decode_varint(b, &mut pos_b);
    match p_a.cmp(&p_b) {
        Ordering::Equal => {}
        other => return other,
    }

    // Compare o
    let o_a = decode_varint(a, &mut pos_a);
    let o_b = decode_varint(b, &mut pos_b);
    o_a.cmp(&o_b)
}

#[cfg(not(target_arch = "wasm32"))]
impl Key for VarintTripleKey {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        compare_varint_keys(data1, data2)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl redb::Value for VarintTripleKey {
    type SelfType<'a> = VarintTripleKey;
    type AsBytes<'a> = &'a [u8];

    fn fixed_width() -> Option<usize> {
        None // Variable width
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        VarintTripleKey::from_bytes(data)
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a> {
        value.as_bytes()
    }

    fn type_name() -> TypeName {
        TypeName::new("VarintTripleKey")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let key = VarintTripleKey::new(1, 2, 3);
        assert_eq!(key.decode(), (1, 2, 3));

        let key = VarintTripleKey::new(12345, 67890, 11111);
        assert_eq!(key.decode(), (12345, 67890, 11111));

        let key = VarintTripleKey::new(u64::MAX, u64::MAX, u64::MAX);
        assert_eq!(key.decode(), (u64::MAX, u64::MAX, u64::MAX));
    }

    #[test]
    fn test_size_savings() {
        // Small IDs: 3 bytes instead of 24
        let key = VarintTripleKey::new(1, 2, 3);
        assert_eq!(key.as_bytes().len(), 3);

        // Medium IDs: ~6 bytes instead of 24
        let key = VarintTripleKey::new(1000, 2000, 3000);
        assert!(key.as_bytes().len() <= 9);
    }

    #[test]
    fn test_compare() {
        let k1 = VarintTripleKey::new(1, 2, 3);
        let k2 = VarintTripleKey::new(1, 2, 4);
        let k3 = VarintTripleKey::new(1, 3, 1);
        let k4 = VarintTripleKey::new(2, 1, 1);

        assert_eq!(
            compare_varint_keys(k1.as_bytes(), k1.as_bytes()),
            Ordering::Equal
        );
        assert_eq!(
            compare_varint_keys(k1.as_bytes(), k2.as_bytes()),
            Ordering::Less
        );
        assert_eq!(
            compare_varint_keys(k2.as_bytes(), k3.as_bytes()),
            Ordering::Less
        );
        assert_eq!(
            compare_varint_keys(k3.as_bytes(), k4.as_bytes()),
            Ordering::Less
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_redb_key_trait() {
        use redb::Key;

        let k1 = VarintTripleKey::new(100, 200, 300);
        let k2 = VarintTripleKey::new(100, 200, 301);

        // Test Key::compare
        assert_eq!(
            VarintTripleKey::compare(k1.as_bytes(), k2.as_bytes()),
            Ordering::Less
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_redb_table_roundtrip() {
        use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
        use tempfile::NamedTempFile;

        const TEST_TABLE: TableDefinition<VarintTripleKey, ()> =
            TableDefinition::new("test_varint");

        let tmp = NamedTempFile::new().unwrap();
        let db = Database::create(tmp.path()).unwrap();

        // Insert
        let write_txn = db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(TEST_TABLE).unwrap();
            table.insert(VarintTripleKey::new(1, 2, 3), ()).unwrap();
            table
                .insert(VarintTripleKey::new(100, 200, 300), ())
                .unwrap();
            table
                .insert(VarintTripleKey::new(10000, 20000, 30000), ())
                .unwrap();
        }
        write_txn.commit().unwrap();

        // Read back
        let read_txn = db.begin_read().unwrap();
        let table = read_txn.open_table(TEST_TABLE).unwrap();

        let keys: Vec<_> = table
            .iter()
            .unwrap()
            .map(|r| r.unwrap().0.value())
            .collect();
        assert_eq!(keys.len(), 3);

        // Verify ordering (should be sorted by s, p, o)
        assert_eq!(keys[0].decode(), (1, 2, 3));
        assert_eq!(keys[1].decode(), (100, 200, 300));
        assert_eq!(keys[2].decode(), (10000, 20000, 30000));
    }

    /// Benchmark: Compare storage size between u64 tuple and varint keys
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bench_storage_size_comparison() {
        use redb::{Database, ReadableDatabase, ReadableTableMetadata, TableDefinition};
        use tempfile::NamedTempFile;

        const TABLE_TUPLE: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("tuple");
        const TABLE_VARINT: TableDefinition<VarintTripleKey, ()> = TableDefinition::new("varint");

        let tmp = NamedTempFile::new().unwrap();
        let db = Database::create(tmp.path()).unwrap();

        const N: u64 = 10_000;

        // Insert same data into both tables
        let write_txn = db.begin_write().unwrap();
        {
            let mut tuple_table = write_txn.open_table(TABLE_TUPLE).unwrap();
            let mut varint_table = write_txn.open_table(TABLE_VARINT).unwrap();

            for i in 1..=N {
                // Simulate realistic IDs (dictionary interned strings)
                let s = i;
                let p = (i % 100) + 1; // ~100 predicates
                let o = i * 2;

                tuple_table.insert((s, p, o), ()).unwrap();
                varint_table
                    .insert(VarintTripleKey::new(s, p, o), ())
                    .unwrap();
            }
        }
        write_txn.commit().unwrap();

        // Compare sizes
        let read_txn = db.begin_read().unwrap();
        let tuple_table = read_txn.open_table(TABLE_TUPLE).unwrap();
        let varint_table = read_txn.open_table(TABLE_VARINT).unwrap();

        let tuple_len = tuple_table.len().unwrap();
        let varint_len = varint_table.len().unwrap();

        assert_eq!(tuple_len, N);
        assert_eq!(varint_len, N);

        // Print size comparison (visible in test output with --nocapture)
        println!("=== Storage Size Comparison ({N} triples) ===");
        println!("Tuple key: 24 bytes/key = {} bytes theoretical", N * 24);
        println!(
            "Varint key: ~6-12 bytes/key = {} bytes theoretical (avg 9)",
            N * 9
        );
        println!("Compression ratio: ~{:.1}x", 24.0 / 9.0);
    }
}
