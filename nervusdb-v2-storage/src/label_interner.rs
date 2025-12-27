//! Label Interner - maps label names (String) to LabelId (u32).
//!
//! This module provides automatic label management for the graph.
//! Users can use string labels in Cypher (`MATCH (n:User)`) without
//! manually managing numeric IDs.
//!
//! # Architecture
//!
//! - `s2i`: HashMap for O(1) string → ID lookup
//! - `i2s`: Vec for O(1) ID → string lookup
//! - Thread-safe snapshots via `Arc<HashMap>`

use std::collections::HashMap;
use std::sync::Arc;

pub use crate::idmap::LabelId;

/// A thread-safe snapshot of the label interner state.
///
/// Used for read transactions to provide consistent label lookups.
#[derive(Debug, Clone)]
pub struct LabelSnapshot {
    s2i: Arc<HashMap<String, LabelId>>,
    i2s: Arc<Vec<String>>,
}

impl LabelSnapshot {
    /// Create a new snapshot from the given maps.
    pub fn new(s2i: HashMap<String, LabelId>, i2s: Vec<String>) -> Self {
        Self {
            s2i: Arc::new(s2i),
            i2s: Arc::new(i2s),
        }
    }

    /// Get the label ID for a name, returning None if not found.
    #[inline]
    pub fn get_id(&self, name: &str) -> Option<LabelId> {
        self.s2i.get(name).copied()
    }

    /// Get the label name for an ID, returning None if not found.
    #[inline]
    pub fn get_name(&self, id: LabelId) -> Option<&str> {
        self.i2s.get(id as usize).map(|s| s.as_str())
    }

    /// Returns true if the label exists.
    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.s2i.contains_key(name)
    }

    /// Returns the number of registered labels.
    #[inline]
    pub fn len(&self) -> usize {
        self.s2i.len()
    }

    /// Returns true if there are no labels.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.s2i.is_empty()
    }

    /// Iterate over all label IDs.
    #[inline]
    pub fn iter_ids(&self) -> impl Iterator<Item = LabelId> + '_ {
        0..self.i2s.len() as LabelId
    }
}

/// A mutable label interner for write operations.
///
/// This is used during write transactions to create and manage labels.
/// On commit, a snapshot is created for readers.
#[derive(Debug, Default)]
pub struct LabelInterner {
    s2i: HashMap<String, LabelId>,
    i2s: Vec<String>,
}

impl LabelInterner {
    /// Create a new empty interner.
    pub fn new() -> Self {
        Self {
            s2i: HashMap::new(),
            i2s: Vec::new(),
        }
    }

    /// Create a snapshot of the current state.
    #[inline]
    pub fn snapshot(&self) -> LabelSnapshot {
        LabelSnapshot::new(self.s2i.clone(), self.i2s.clone())
    }

    /// Get the label ID for a name, returning None if not found.
    ///
    /// This is a read-only operation and can be used in any context.
    #[inline]
    pub fn get_id(&self, name: &str) -> Option<LabelId> {
        self.s2i.get(name).copied()
    }

    /// Get the label name for an ID, returning None if not found.
    ///
    /// This is a read-only operation and can be used in any context.
    #[inline]
    pub fn get_name(&self, id: LabelId) -> Option<&str> {
        self.i2s.get(id as usize).map(|s| s.as_str())
    }

    /// Get or create a label, returning the existing or new ID.
    ///
    /// If the label already exists, returns the existing ID.
    /// If the label doesn't exist, creates it and returns the new ID.
    ///
    /// # Panics
    ///
    /// Panics if the label count exceeds `u32::MAX`.
    #[inline]
    pub fn get_or_create(&mut self, name: &str) -> LabelId {
        if let Some(id) = self.s2i.get(name) {
            return *id;
        }

        let id = self.i2s.len() as LabelId;
        self.s2i.insert(name.to_string(), id);
        self.i2s.push(name.to_string());
        id
    }

    /// Get or create a label, returning the existing or new ID.
    ///
    /// This is an optimized version that avoids String cloning when possible.
    #[inline]
    pub fn get_or_create_owned(&mut self, name: String) -> LabelId {
        if let Some(id) = self.s2i.get(&name) {
            return *id;
        }

        let id = self.i2s.len() as LabelId;
        self.s2i.insert(name.clone(), id);
        self.i2s.push(name);
        id
    }

    /// Returns true if the label exists.
    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.s2i.contains_key(name)
    }

    /// Returns the number of registered labels.
    #[inline]
    pub fn len(&self) -> usize {
        self.s2i.len()
    }

    /// Returns true if there are no labels.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.s2i.is_empty()
    }

    /// Returns the next label ID that would be created.
    #[inline]
    pub fn next_id(&self) -> LabelId {
        self.i2s.len() as LabelId
    }

    /// Merge another interner's labels into this one.
    ///
    /// Returns the number of labels that were added.
    pub fn merge(&mut self, other: &LabelInterner) -> usize {
        let mut added = 0;
        for (name, id) in &other.s2i {
            if !self.s2i.contains_key(name) {
                self.s2i.insert(name.clone(), *id);
                if let Some(name_str) = other.i2s.get(*id as usize) {
                    self.i2s.push(name_str.clone());
                }
                added += 1;
            }
        }
        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut interner = LabelInterner::new();

        // Create first label
        let user_id = interner.get_or_create("User");
        assert_eq!(user_id, 0);
        assert_eq!(interner.get_name(user_id), Some("User"));
        assert_eq!(interner.get_id("User"), Some(user_id));

        // Create same label - should return same ID
        let user_id2 = interner.get_or_create("User");
        assert_eq!(user_id, user_id2);

        // Create different label
        let post_id = interner.get_or_create("Post");
        assert_eq!(post_id, 1);
        assert_ne!(user_id, post_id);
    }

    #[test]
    fn test_snapshot() {
        let mut interner = LabelInterner::new();
        let user_id = interner.get_or_create("User");

        // Create snapshot
        let snapshot = interner.snapshot();
        assert_eq!(snapshot.get_id("User"), Some(user_id));
        assert_eq!(snapshot.get_name(user_id), Some("User"));

        // Modify original - snapshot should be unchanged
        let post_id = interner.get_or_create("Post");
        assert_eq!(snapshot.get_id("Post"), None);
        assert_eq!(interner.get_id("Post"), Some(post_id));
    }

    #[test]
    fn test_contains() {
        let mut interner = LabelInterner::new();
        interner.get_or_create("User");

        assert!(interner.contains("User"));
        assert!(!interner.contains("Post"));
    }

    #[test]
    fn test_len() {
        let mut interner = LabelInterner::new();
        assert_eq!(interner.len(), 0);

        interner.get_or_create("User");
        assert_eq!(interner.len(), 1);

        interner.get_or_create("User"); // Same label
        assert_eq!(interner.len(), 1);

        interner.get_or_create("Post");
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn test_get_or_create_owned() {
        let mut interner = LabelInterner::new();

        let user_id = interner.get_or_create_owned("User".to_string());
        assert_eq!(user_id, 0);
        assert_eq!(interner.get_id("User"), Some(user_id));
    }

    #[test]
    fn test_iter_ids() {
        let mut interner = LabelInterner::new();
        interner.get_or_create("User");
        interner.get_or_create("Post");
        interner.get_or_create("Comment");

        let ids: Vec<_> = interner.snapshot().iter_ids().collect();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    #[test]
    fn test_empty_snapshot() {
        let interner = LabelInterner::new();
        let snapshot = interner.snapshot();

        assert!(snapshot.is_empty());
        assert_eq!(snapshot.len(), 0);
        assert_eq!(snapshot.get_id("User"), None);
    }

    #[test]
    fn test_merge() {
        let mut interner1 = LabelInterner::new();
        interner1.get_or_create("User");
        interner1.get_or_create("Post");

        let mut interner2 = LabelInterner::new();
        interner2.get_or_create("User"); // Same as interner1
        interner2.get_or_create("Comment"); // New

        let added = interner1.merge(&interner2);
        assert_eq!(added, 1); // Only Comment was added
        assert_eq!(interner1.len(), 3);
    }
}
