//! Simple in-memory dictionary that maps strings to compact numeric identifiers.

use std::collections::HashMap;

use crate::error::{Error, Result};

/// Identifier assigned to unique strings within the dictionary.
pub type StringId = u64;

#[derive(Default, Debug)]
pub struct Dictionary {
    str_to_id: HashMap<String, StringId>,
    id_to_str: Vec<String>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_vec(values: Vec<String>) -> Self {
        let mut id_to_str = Vec::with_capacity(values.len());
        let mut str_to_id = HashMap::with_capacity(values.len());

        for (idx, value) in values.into_iter().enumerate() {
            let id = idx as StringId;
            str_to_id.insert(value.clone(), id);
            id_to_str.push(value);
        }

        Self {
            str_to_id,
            id_to_str,
        }
    }

    /// Returns the identifier for `value`, inserting it if missing.
    pub fn get_or_insert<S: AsRef<str>>(&mut self, value: S) -> StringId {
        let key = value.as_ref();
        if let Some(&id) = self.str_to_id.get(key) {
            return id;
        }

        let id = self.id_to_str.len() as StringId;
        let owned = key.to_owned();
        self.id_to_str.push(owned.clone());
        self.str_to_id.insert(owned, id);
        id
    }

    /// Returns the identifier for `value`, or `None` if it is unknown.
    pub fn lookup_id<S: AsRef<str>>(&self, value: S) -> Option<StringId> {
        self.str_to_id.get(value.as_ref()).copied()
    }

    /// Returns the string associated with `id`.
    pub fn lookup_value(&self, id: StringId) -> Result<&str> {
        self.id_to_str
            .get(id as usize)
            .map(|s| s.as_str())
            .ok_or_else(|| Error::UnknownString(format!("id {id}")))
    }

    /// Current number of unique entries in the dictionary.
    pub fn len(&self) -> usize {
        self.id_to_str.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id_to_str.is_empty()
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
