//! Basic triple/fact representation helpers.

use crate::StringId;
use serde::{Deserialize, Serialize};

/// Fully encoded triple referencing dictionary identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Triple {
    pub subject_id: StringId,
    pub predicate_id: StringId,
    pub object_id: StringId,
}

impl Triple {
    pub fn new(subject_id: StringId, predicate_id: StringId, object_id: StringId) -> Self {
        Self {
            subject_id,
            predicate_id,
            object_id,
        }
    }

    /// Serialises the triple into a fixed-width 24 byte representation.
    pub fn to_bytes(self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        buf[0..8].copy_from_slice(&self.subject_id.to_le_bytes());
        buf[8..16].copy_from_slice(&self.predicate_id.to_le_bytes());
        buf[16..24].copy_from_slice(&self.object_id.to_le_bytes());
        buf
    }

    /// Reconstructs a triple from the 24 byte representation.
    pub fn from_bytes(buf: [u8; 24]) -> Self {
        let subject_id = StringId::from_le_bytes(buf[0..8].try_into().unwrap());
        let predicate_id = StringId::from_le_bytes(buf[8..16].try_into().unwrap());
        let object_id = StringId::from_le_bytes(buf[16..24].try_into().unwrap());
        Self::new(subject_id, predicate_id, object_id)
    }
}

/// User-facing fact used when inserting new data.
#[derive(Debug, Clone, Copy)]
pub struct Fact<'a> {
    pub subject: &'a str,
    pub predicate: &'a str,
    pub object: &'a str,
}

impl<'a> Fact<'a> {
    pub fn new(subject: &'a str, predicate: &'a str, object: &'a str) -> Self {
        Self {
            subject,
            predicate,
            object,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bytes() {
        let triple = Triple::new(1, 2, 3);
        let buf = triple.to_bytes();
        let restored = Triple::from_bytes(buf);
        assert_eq!(triple, restored);
    }
}
