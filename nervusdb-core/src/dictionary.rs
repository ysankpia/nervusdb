//! Simple in-memory dictionary that maps strings to compact numeric identifiers.

use indexmap::IndexSet;

use crate::error::{Error, Result};

/// Identifier assigned to unique strings within the dictionary.
pub type StringId = u64;

#[derive(Default, Debug)]
pub struct Dictionary {
    inner: IndexSet<String>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_vec(values: Vec<String>) -> Self {
        let mut inner = IndexSet::with_capacity(values.len());
        for value in values {
            inner.insert(value);
        }
        Self { inner }
    }

    /// Returns the identifier for `value`, inserting it if missing.
    pub fn get_or_insert<S: AsRef<str>>(&mut self, value: S) -> StringId {
        let (index, _) = self.inner.insert_full(value.as_ref().to_owned());
        index as StringId
    }

    /// Returns the identifier for `value`, or `None` if it is unknown.
    pub fn lookup_id<S: AsRef<str>>(&self, value: S) -> Option<StringId> {
        self.inner
            .get_index_of(value.as_ref())
            .map(|i| i as StringId)
    }

    /// Returns the string associated with `id`.
    pub fn lookup_value(&self, id: StringId) -> Result<&str> {
        self.inner
            .get_index(id as usize)
            .map(|s| s.as_str())
            .ok_or_else(|| Error::UnknownString(format!("id {id}")))
    }

    /// Current number of unique entries in the dictionary.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_lookup() {
        let mut dict = Dictionary::new();
        let alice = dict.get_or_insert("alice");
        assert_eq!(alice, 0);
        assert_eq!(dict.lookup_id("alice"), Some(alice));
        assert_eq!(dict.lookup_value(alice).unwrap(), "alice");

        let duplicate = dict.get_or_insert("alice");
        assert_eq!(duplicate, alice);
        assert_eq!(dict.len(), 1);
    }

    #[test]
    fn unknown_value() {
        let dict = Dictionary::new();
        let err = dict.lookup_value(42).unwrap_err();
        assert_eq!(format!("{err}"), "unknown string: id 42");
    }
}
