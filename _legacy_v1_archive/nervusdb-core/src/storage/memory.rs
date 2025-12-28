use std::collections::BTreeSet;
use std::ops::RangeInclusive;
use std::sync::{Arc, RwLock, RwLockReadGuard};

use ouroboros::self_referencing;

use crate::triple::{Fact, Triple};

#[derive(Debug)]
pub struct MemoryHexastore {
    spo: Arc<RwLock<BTreeSet<(u64, u64, u64)>>>,
    pos: Arc<RwLock<BTreeSet<(u64, u64, u64)>>>,
    osp: Arc<RwLock<BTreeSet<(u64, u64, u64)>>>,
    id_to_str: Arc<RwLock<std::collections::HashMap<u64, String>>>,
    str_to_id: Arc<RwLock<std::collections::HashMap<String, u64>>>,
    next_id: Arc<RwLock<u64>>,
}

impl Default for MemoryHexastore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryHexastore {
    pub fn new() -> Self {
        Self {
            spo: Arc::new(RwLock::new(BTreeSet::new())),
            pos: Arc::new(RwLock::new(BTreeSet::new())),
            osp: Arc::new(RwLock::new(BTreeSet::new())),
            id_to_str: Arc::new(RwLock::new(std::collections::HashMap::new())),
            str_to_id: Arc::new(RwLock::new(std::collections::HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    pub fn remove(&mut self, triple: &Triple) {
        let s = triple.subject_id;
        let p = triple.predicate_id;
        let o = triple.object_id;
        self.spo.write().unwrap().remove(&(s, p, o));
        self.pos.write().unwrap().remove(&(p, o, s));
        self.osp.write().unwrap().remove(&(o, s, p));
    }

    fn plan(&self, subject: Option<u64>, predicate: Option<u64>, object: Option<u64>) -> QuerySpec {
        match (subject, predicate, object) {
            (Some(s), Some(p), Some(o)) => QuerySpec::Exact(Triple::new(s, p, o)),
            (Some(s), Some(p), None) => QuerySpec::range(
                self.index(IndexKind::Spo),
                (s, p, u64::MIN),
                (s, p, u64::MAX),
                IndexKind::Spo.decode_fn(),
            ),
            (Some(s), None, Some(o)) => QuerySpec::range(
                self.index(IndexKind::Osp),
                (o, s, u64::MIN),
                (o, s, u64::MAX),
                IndexKind::Osp.decode_fn(),
            ),
            (None, Some(p), Some(o)) => QuerySpec::range(
                self.index(IndexKind::Pos),
                (p, o, u64::MIN),
                (p, o, u64::MAX),
                IndexKind::Pos.decode_fn(),
            ),
            (Some(s), None, None) => QuerySpec::range(
                self.index(IndexKind::Spo),
                (s, u64::MIN, u64::MIN),
                (s, u64::MAX, u64::MAX),
                IndexKind::Spo.decode_fn(),
            ),
            (None, Some(p), None) => QuerySpec::range(
                self.index(IndexKind::Pos),
                (p, u64::MIN, u64::MIN),
                (p, u64::MAX, u64::MAX),
                IndexKind::Pos.decode_fn(),
            ),
            (None, None, Some(o)) => QuerySpec::range(
                self.index(IndexKind::Osp),
                (o, u64::MIN, u64::MIN),
                (o, u64::MAX, u64::MAX),
                IndexKind::Osp.decode_fn(),
            ),
            (None, None, None) => QuerySpec::range(
                self.index(IndexKind::Spo),
                (u64::MIN, u64::MIN, u64::MIN),
                (u64::MAX, u64::MAX, u64::MAX),
                IndexKind::Spo.decode_fn(),
            ),
        }
    }

    fn index(&self, kind: IndexKind) -> Arc<RwLock<BTreeSet<(u64, u64, u64)>>> {
        match kind {
            IndexKind::Spo => Arc::clone(&self.spo),
            IndexKind::Pos => Arc::clone(&self.pos),
            IndexKind::Osp => Arc::clone(&self.osp),
        }
    }

    fn contains(&self, triple: (u64, u64, u64)) -> bool {
        self.spo.read().unwrap().contains(&triple)
    }
}

impl crate::storage::Hexastore for MemoryHexastore {
    fn insert(&mut self, triple: &Triple) -> crate::Result<bool> {
        let s = triple.subject_id;
        let p = triple.predicate_id;
        let o = triple.object_id;

        {
            let mut spo = self.spo.write().unwrap();
            if spo.contains(&(s, p, o)) {
                return Ok(false);
            }
            spo.insert((s, p, o));
        }
        self.pos.write().unwrap().insert((p, o, s));
        self.osp.write().unwrap().insert((o, s, p));

        Ok(true)
    }

    fn delete(&mut self, triple: &Triple) -> crate::Result<bool> {
        let s = triple.subject_id;
        let p = triple.predicate_id;
        let o = triple.object_id;

        {
            let mut spo = self.spo.write().unwrap();
            if !spo.contains(&(s, p, o)) {
                return Ok(false);
            }
            spo.remove(&(s, p, o));
        }
        self.pos.write().unwrap().remove(&(p, o, s));
        self.osp.write().unwrap().remove(&(o, s, p));

        Ok(true)
    }

    fn insert_fact(&mut self, fact: Fact<'_>) -> crate::Result<Triple> {
        let s = self.intern(fact.subject)?;
        let p = self.intern(fact.predicate)?;
        let o = self.intern(fact.object)?;
        let triple = Triple::new(s, p, o);
        self.insert(&triple)?;
        Ok(triple)
    }

    fn query(
        &self,
        subject: Option<u64>,
        predicate: Option<u64>,
        object: Option<u64>,
    ) -> crate::storage::HexastoreIter {
        match self.plan(subject, predicate, object) {
            QuerySpec::Exact(triple) => {
                if self.contains((triple.subject_id, triple.predicate_id, triple.object_id)) {
                    Box::new(std::iter::once(triple))
                } else {
                    Box::new(std::iter::empty())
                }
            }
            QuerySpec::Range(range) => match MemoryCursor::create(range) {
                Ok(cursor) => Box::new(cursor),
                Err(_) => Box::new(std::iter::empty()),
            },
        }
    }

    fn resolve_str(&self, id: u64) -> crate::Result<Option<String>> {
        Ok(self.id_to_str.read().unwrap().get(&id).cloned())
    }

    fn resolve_id(&self, value: &str) -> crate::Result<Option<u64>> {
        Ok(self.str_to_id.read().unwrap().get(value).cloned())
    }

    fn intern(&mut self, value: &str) -> crate::Result<u64> {
        let mut str_to_id = self.str_to_id.write().unwrap();
        if let Some(&id) = str_to_id.get(value) {
            return Ok(id);
        }
        let mut next_id = self.next_id.write().unwrap();
        let id = *next_id;
        *next_id += 1;
        str_to_id.insert(value.to_string(), id);
        self.id_to_str
            .write()
            .unwrap()
            .insert(id, value.to_string());
        Ok(id)
    }

    fn dictionary_size(&self) -> crate::Result<u64> {
        Ok(self.id_to_str.read().unwrap().len() as u64)
    }

    fn set_node_property(&mut self, _id: u64, _value: &str) -> crate::Result<()> {
        // TODO: Implement in-memory property storage if needed
        Ok(())
    }

    fn get_node_property(&self, _id: u64) -> crate::Result<Option<String>> {
        Ok(None)
    }

    fn set_edge_property(&mut self, _s: u64, _p: u64, _o: u64, _value: &str) -> crate::Result<()> {
        Ok(())
    }

    fn get_edge_property(&self, _s: u64, _p: u64, _o: u64) -> crate::Result<Option<String>> {
        Ok(None)
    }
}

#[derive(Clone, Copy)]
enum IndexKind {
    Spo,
    Pos,
    Osp,
}

impl IndexKind {
    fn decode_fn(self) -> fn((u64, u64, u64)) -> Triple {
        match self {
            IndexKind::Spo => |(s, p, o)| Triple::new(s, p, o),
            IndexKind::Pos => |(p, o, s)| Triple::new(s, p, o),
            IndexKind::Osp => |(o, s, p)| Triple::new(s, p, o),
        }
    }
}

struct QueryRange {
    index: Arc<RwLock<BTreeSet<(u64, u64, u64)>>>,
    bounds: RangeInclusive<(u64, u64, u64)>,
    decode: fn((u64, u64, u64)) -> Triple,
}

enum QuerySpec {
    Exact(Triple),
    Range(QueryRange),
}

impl QuerySpec {
    fn range(
        index: Arc<RwLock<BTreeSet<(u64, u64, u64)>>>,
        start: (u64, u64, u64),
        end: (u64, u64, u64),
        decode: fn((u64, u64, u64)) -> Triple,
    ) -> Self {
        QuerySpec::Range(QueryRange {
            index,
            bounds: start..=end,
            decode,
        })
    }
}

#[self_referencing]
struct MemoryCursor {
    index: Arc<RwLock<BTreeSet<(u64, u64, u64)>>>,
    #[borrows(index)]
    #[covariant]
    guard: RwLockReadGuard<'this, BTreeSet<(u64, u64, u64)>>,
    #[borrows(guard)]
    #[covariant]
    iter: std::collections::btree_set::Range<'this, (u64, u64, u64)>,
    decode: fn((u64, u64, u64)) -> Triple,
}

impl MemoryCursor {
    fn create(range: QueryRange) -> crate::Result<Self> {
        let QueryRange {
            index,
            bounds,
            decode,
        } = range;
        let cursor = MemoryCursorBuilder {
            index,
            guard_builder: |idx| idx.read().unwrap(),
            iter_builder: move |guard| guard.range(bounds.clone()),
            decode,
        }
        .build();
        Ok(cursor)
    }
}

impl Iterator for MemoryCursor {
    type Item = Triple;

    fn next(&mut self) -> Option<Self::Item> {
        let decode = *self.borrow_decode();
        self.with_iter_mut(|iter| iter.next().map(|key| decode(*key)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Hexastore;

    fn triple(s: u64, p: u64, o: u64) -> Triple {
        Triple::new(s, p, o)
    }

    #[test]
    fn inserts_and_exact_match() {
        let mut store = MemoryHexastore::new();
        assert!(store.insert(&triple(1, 2, 3)).unwrap());
        assert!(!store.insert(&triple(1, 2, 3)).unwrap());

        let mut iter = store.query(Some(1), Some(2), Some(3));
        assert_eq!(iter.next(), Some(triple(1, 2, 3)));
        assert!(iter.next().is_none());
    }

    #[test]
    fn subject_and_object_scans_cover_respective_indices() {
        let mut store = MemoryHexastore::new();
        store.insert(&triple(1, 2, 3)).unwrap();
        store.insert(&triple(1, 4, 5)).unwrap();
        store.insert(&triple(7, 2, 3)).unwrap();

        // Subject-only scan should return both triples for subject 1.
        let results: Vec<_> = store.query(Some(1), None, None).collect();
        assert_eq!(results.len(), 2);

        // Object-only scan should return both triples with object 3.
        let results: Vec<_> = store.query(None, None, Some(3)).collect();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn predicate_only_scan_uses_pos_index() {
        let mut store = MemoryHexastore::new();
        store.insert(&triple(1, 11, 3)).unwrap();
        store.insert(&triple(2, 11, 4)).unwrap();
        store.insert(&triple(3, 22, 5)).unwrap();

        let results: Vec<_> = store.query(None, Some(11), None).collect();
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .any(|t| t.subject_id == 1 && t.object_id == 3)
        );
        assert!(
            results
                .iter()
                .any(|t| t.subject_id == 2 && t.object_id == 4)
        );
    }

    #[test]
    fn full_iteration_yields_all_triples() {
        let mut store = MemoryHexastore::new();
        for i in 0..5 {
            store.insert(&triple(i, i + 1, i + 2)).unwrap();
        }
        let collected: Vec<_> = store.iter().collect();
        assert_eq!(collected.len(), 5);
    }
}
