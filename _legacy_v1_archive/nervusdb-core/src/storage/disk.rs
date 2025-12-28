use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use lru::LruCache;
use ouroboros::self_referencing;
use redb::{
    Database, Range, ReadTransaction, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    Table, WriteTransaction,
};
use std::num::NonZeroUsize;

use crate::error::{Error, Result};
#[cfg(not(feature = "varint-keys"))]
use crate::storage::schema::{
    TABLE_EDGE_PROPS, TABLE_EDGE_PROPS_BINARY, TABLE_ID_TO_STR, TABLE_META, TABLE_NODE_PROPS,
    TABLE_NODE_PROPS_BINARY, TABLE_OSP, TABLE_POS, TABLE_SPO, TABLE_STR_TO_ID,
};
#[cfg(feature = "varint-keys")]
use crate::storage::schema::{
    TABLE_EDGE_PROPS, TABLE_EDGE_PROPS_BINARY, TABLE_ID_TO_STR, TABLE_META, TABLE_NODE_PROPS,
    TABLE_NODE_PROPS_BINARY, TABLE_OSP_V2 as TABLE_OSP, TABLE_POS_V2 as TABLE_POS,
    TABLE_SPO_V2 as TABLE_SPO, TABLE_STR_TO_ID,
};
#[cfg(feature = "varint-keys")]
use crate::storage::varint_key::VarintTripleKey;
use crate::storage::{Hexastore, HexastoreIter};
use crate::triple::{Fact, Triple};

// Type alias for triple key based on feature
#[cfg(feature = "varint-keys")]
type TripleKey = VarintTripleKey;
#[cfg(not(feature = "varint-keys"))]
type TripleKey = (u64, u64, u64);

// Helper to create key from (a, b, c)
#[inline]
fn make_key(a: u64, b: u64, c: u64) -> TripleKey {
    #[cfg(feature = "varint-keys")]
    {
        VarintTripleKey::new(a, b, c)
    }
    #[cfg(not(feature = "varint-keys"))]
    {
        (a, b, c)
    }
}

// Helper to decode key to (a, b, c)
#[inline]
fn decode_key(key: TripleKey) -> (u64, u64, u64) {
    #[cfg(feature = "varint-keys")]
    {
        key.decode()
    }
    #[cfg(not(feature = "varint-keys"))]
    {
        key
    }
}
#[derive(Debug)]
pub struct DiskHexastore {
    db: Arc<Database>,
    read_cache: RwLock<Option<(u64, Arc<ReadHandles>)>>,
    generation: AtomicU64,
}

impl DiskHexastore {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Self::init_tables(&db)?;
        Ok(Self {
            db,
            read_cache: RwLock::new(None),
            generation: AtomicU64::new(0),
        })
    }

    fn init_tables(db: &Database) -> Result<()> {
        let write_txn = db.begin_write().map_err(|e| Error::Other(e.to_string()))?;
        {
            let _ = write_txn
                .open_table(TABLE_SPO)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_POS)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_OSP)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_ID_TO_STR)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_STR_TO_ID)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_NODE_PROPS)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_EDGE_PROPS)
                .map_err(|e| Error::Other(e.to_string()))?;
            // v2.0 binary property tables
            let _ = write_txn
                .open_table(TABLE_NODE_PROPS_BINARY)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_EDGE_PROPS_BINARY)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_META)
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    fn plan(
        subject_id: Option<u64>,
        predicate_id: Option<u64>,
        object_id: Option<u64>,
    ) -> QuerySpec {
        match (subject_id, predicate_id, object_id) {
            (Some(s), Some(p), Some(o)) => QuerySpec::Exact(Triple::new(s, p, o)),
            (Some(s), Some(p), None) => {
                QuerySpec::range(IndexKind::Spo, (s, p, u64::MIN), (s, p, u64::MAX))
            }
            (Some(s), None, Some(o)) => {
                QuerySpec::range(IndexKind::Osp, (o, s, u64::MIN), (o, s, u64::MAX))
            }
            (None, Some(p), Some(o)) => {
                QuerySpec::range(IndexKind::Pos, (p, o, u64::MIN), (p, o, u64::MAX))
            }
            (Some(s), None, None) => QuerySpec::range(
                IndexKind::Spo,
                (s, u64::MIN, u64::MIN),
                (s, u64::MAX, u64::MAX),
            ),
            (None, Some(p), None) => QuerySpec::range(
                IndexKind::Pos,
                (p, u64::MIN, u64::MIN),
                (p, u64::MAX, u64::MAX),
            ),
            (None, None, Some(o)) => QuerySpec::range(
                IndexKind::Osp,
                (o, u64::MIN, u64::MIN),
                (o, u64::MAX, u64::MAX),
            ),
            (None, None, None) => QuerySpec::range(
                IndexKind::Spo,
                (u64::MIN, u64::MIN, u64::MIN),
                (u64::MAX, u64::MAX, u64::MAX),
            ),
        }
    }

    fn invalidate_read_cache(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.read_cache.write().expect("read cache poisoned");
        *guard = None;
    }

    fn read_handles(&self) -> Result<Arc<ReadHandles>> {
        let r#gen = self.generation.load(Ordering::Relaxed);
        {
            let guard = self.read_cache.read().expect("read cache poisoned");
            if let Some((cached_gen, handles)) = guard.as_ref()
                && *cached_gen == r#gen
            {
                return Ok(handles.clone());
            }
        }

        let handles = Arc::new(ReadHandles::build(&self.db)?);
        let mut guard = self.read_cache.write().expect("read cache poisoned");
        *guard = Some((r#gen, handles.clone()));
        Ok(handles)
    }

    fn lookup_exact(&self, triple: &Triple) -> Result<bool> {
        let handles = self.read_handles()?;
        handles
            .spo
            .get(make_key(
                triple.subject_id,
                triple.predicate_id,
                triple.object_id,
            ))
            .map_err(|e| Error::Other(e.to_string()))
            .map(|opt| opt.is_some())
    }
}

impl Hexastore for DiskHexastore {
    fn insert(&mut self, triple: &Triple) -> Result<bool> {
        let mut write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        let inserted = insert_triple(&mut write_txn, triple)?;
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(inserted)
    }

    fn delete(&mut self, triple: &Triple) -> Result<bool> {
        let mut write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        let deleted = delete_triple(&mut write_txn, triple)?;
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(deleted)
    }

    fn insert_fact(&mut self, fact: Fact<'_>) -> Result<Triple> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        let mut handles = WriteTableHandles::new(&write_txn)?;
        let triple = handles.insert_fact(fact)?;
        drop(handles);
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(triple)
    }

    fn query(
        &self,
        subject_id: Option<u64>,
        predicate_id: Option<u64>,
        object_id: Option<u64>,
    ) -> HexastoreIter {
        match Self::plan(subject_id, predicate_id, object_id) {
            QuerySpec::Exact(triple) => match self.lookup_exact(&triple) {
                Ok(true) => Box::new(std::iter::once(triple)),
                Ok(false) | Err(_) => Box::new(std::iter::empty()),
            },
            QuerySpec::Range(range) => match self.read_handles() {
                Ok(handles) => match CachedCursor::create(handles, range) {
                    Ok(cursor) => Box::new(cursor),
                    Err(_) => Box::new(std::iter::empty()),
                },
                Err(_) => Box::new(std::iter::empty()),
            },
        }
    }

    fn resolve_str(&self, id: u64) -> Result<Option<String>> {
        let handles = self.read_handles()?;
        let result = handles
            .id_to_str
            .get(id)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value().to_string());
        Ok(result)
    }

    fn resolve_id(&self, value: &str) -> Result<Option<u64>> {
        let handles = self.read_handles()?;
        let result = handles
            .str_to_id
            .get(value)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value());
        Ok(result)
    }

    fn bulk_intern(&mut self, values: &[&str]) -> Result<Vec<u64>> {
        if values.is_empty() {
            return Ok(Vec::new());
        }

        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        let mut handles = WriteTableHandles::new(&write_txn)?;
        let ids = handles.intern_bulk(values)?;
        drop(handles);
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(ids)
    }

    fn intern(&mut self, value: &str) -> Result<u64> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        let mut handles = WriteTableHandles::new(&write_txn)?;
        let id = handles.intern(value)?;
        drop(handles);
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(id)
    }

    fn dictionary_size(&self) -> Result<u64> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = txn
            .open_table(TABLE_ID_TO_STR)
            .map_err(|e| Error::Other(e.to_string()))?;
        table.len().map_err(|e| Error::Other(e.to_string()))
    }

    fn set_node_property(&mut self, id: u64, value: &str) -> Result<()> {
        let tx = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut table = tx
                .open_table(TABLE_NODE_PROPS)
                .map_err(|e| Error::Other(e.to_string()))?;
            table
                .insert(id, value)
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        tx.commit().map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(())
    }

    fn get_node_property(&self, id: u64) -> Result<Option<String>> {
        let tx = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = tx
            .open_table(TABLE_NODE_PROPS)
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(table
            .get(id)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value().to_string()))
    }

    fn set_edge_property(&mut self, s: u64, p: u64, o: u64, value: &str) -> Result<()> {
        let tx = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut table = tx
                .open_table(TABLE_EDGE_PROPS)
                .map_err(|e| Error::Other(e.to_string()))?;
            table
                .insert((s, p, o), value)
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        tx.commit().map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(())
    }

    fn get_edge_property(&self, s: u64, p: u64, o: u64) -> Result<Option<String>> {
        let tx = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = tx
            .open_table(TABLE_EDGE_PROPS)
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(table
            .get((s, p, o))
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value().to_string()))
    }

    // Optimized batch operations using a single transaction
    // This is 10-100x faster than calling single operations in a loop
    // because it eliminates per-operation transaction overhead

    fn batch_insert(&mut self, triples: &[Triple]) -> Result<usize> {
        if triples.is_empty() {
            return Ok(0);
        }

        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        // Use cached table handles for performance
        let mut handles = WriteTableHandles::new(&write_txn)?;

        let mut count = 0;
        for triple in triples {
            if handles.insert_triple(triple)? {
                count += 1;
            }
        }

        drop(handles);
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(count)
    }

    fn batch_delete(&mut self, triples: &[Triple]) -> Result<usize> {
        if triples.is_empty() {
            return Ok(0);
        }

        let mut write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut count = 0;
        for triple in triples {
            if delete_triple(&mut write_txn, triple)? {
                count += 1;
            }
        }

        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(count)
    }

    fn batch_set_node_properties(&mut self, props: &[(u64, &str)]) -> Result<()> {
        if props.is_empty() {
            return Ok(());
        }

        let tx = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut table = tx
                .open_table(TABLE_NODE_PROPS)
                .map_err(|e| Error::Other(e.to_string()))?;
            for (id, value) in props {
                table
                    .insert(*id, *value)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }
        }
        tx.commit().map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    fn batch_set_edge_properties(&mut self, props: &[((u64, u64, u64), &str)]) -> Result<()> {
        if props.is_empty() {
            return Ok(());
        }

        let tx = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut table = tx
                .open_table(TABLE_EDGE_PROPS)
                .map_err(|e| Error::Other(e.to_string()))?;
            for ((s, p, o), value) in props {
                table
                    .insert((*s, *p, *o), *value)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }
        }
        tx.commit().map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    /// Optimized batch insert for facts using cached table handles
    fn batch_insert_facts(&mut self, facts: &[Fact<'_>]) -> Result<Vec<Triple>> {
        if facts.is_empty() {
            return Ok(Vec::new());
        }

        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut handles = WriteTableHandles::new(&write_txn)?;
        let mut results = Vec::with_capacity(facts.len());

        for fact in facts {
            let triple = handles.insert_fact(*fact)?;
            results.push(triple);
        }

        drop(handles);
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();

        Ok(results)
    }

    // Binary property methods (v2.0, FlexBuffers for performance)

    fn set_node_property_binary(&mut self, id: u64, value: &[u8]) -> Result<()> {
        let tx = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut table = tx
                .open_table(TABLE_NODE_PROPS_BINARY)
                .map_err(|e| Error::Other(e.to_string()))?;
            table
                .insert(id, value)
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        tx.commit().map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(())
    }

    fn get_node_property_binary(&self, id: u64) -> Result<Option<Vec<u8>>> {
        let tx = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;

        // Try binary table first (v2.0)
        let binary_table = tx
            .open_table(TABLE_NODE_PROPS_BINARY)
            .map_err(|e| Error::Other(e.to_string()))?;

        if let Some(value) = binary_table
            .get(id)
            .map_err(|e| Error::Other(e.to_string()))?
        {
            return Ok(Some(value.value().to_vec()));
        }

        // Fallback to legacy string table (v1.x backward compatibility)
        let string_table = tx
            .open_table(TABLE_NODE_PROPS)
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(string_table
            .get(id)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value().as_bytes().to_vec()))
    }

    fn delete_node_properties(&mut self, id: u64) -> Result<()> {
        let tx = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            // Delete from binary table (v2.0)
            let mut binary_table = tx
                .open_table(TABLE_NODE_PROPS_BINARY)
                .map_err(|e| Error::Other(e.to_string()))?;
            binary_table
                .remove(id)
                .map_err(|e| Error::Other(e.to_string()))?;

            // Delete from legacy string table (backward compatibility)
            let mut string_table = tx
                .open_table(TABLE_NODE_PROPS)
                .map_err(|e| Error::Other(e.to_string()))?;
            string_table
                .remove(id)
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        tx.commit().map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(())
    }

    fn set_edge_property_binary(&mut self, s: u64, p: u64, o: u64, value: &[u8]) -> Result<()> {
        let tx = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut table = tx
                .open_table(TABLE_EDGE_PROPS_BINARY)
                .map_err(|e| Error::Other(e.to_string()))?;
            table
                .insert((s, p, o), value)
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        tx.commit().map_err(|e| Error::Other(e.to_string()))?;
        self.invalidate_read_cache();
        Ok(())
    }

    fn get_edge_property_binary(&self, s: u64, p: u64, o: u64) -> Result<Option<Vec<u8>>> {
        let tx = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;

        // Try binary table first (v2.0)
        let binary_table = tx
            .open_table(TABLE_EDGE_PROPS_BINARY)
            .map_err(|e| Error::Other(e.to_string()))?;

        if let Some(value) = binary_table
            .get((s, p, o))
            .map_err(|e| Error::Other(e.to_string()))?
        {
            return Ok(Some(value.value().to_vec()));
        }

        // Fallback to legacy string table (v1.x backward compatibility)
        let string_table = tx
            .open_table(TABLE_EDGE_PROPS)
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(string_table
            .get((s, p, o))
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value().as_bytes().to_vec()))
    }

    fn after_write_commit(&self) {
        self.invalidate_read_cache();
    }
}

/// Cached table handles for write operations
/// Opens all tables once and reuses handles for maximum performance
pub(crate) struct WriteTableHandles<'txn> {
    pub spo: Table<'txn, TripleKey, ()>,
    pub pos: Table<'txn, TripleKey, ()>,
    pub osp: Table<'txn, TripleKey, ()>,
    pub str_to_id: Table<'txn, &'static str, u64>,
    pub id_to_str: Table<'txn, u64, &'static str>,
    string_cache: LruCache<String, u64>,
    next_id: u64,
}

const STRING_CACHE_LIMIT: usize = 100_000;

impl<'txn> WriteTableHandles<'txn> {
    pub fn new(txn: &'txn WriteTransaction) -> Result<Self> {
        let spo = txn
            .open_table(TABLE_SPO)
            .map_err(|e| Error::Other(e.to_string()))?;
        let pos = txn
            .open_table(TABLE_POS)
            .map_err(|e| Error::Other(e.to_string()))?;
        let osp = txn
            .open_table(TABLE_OSP)
            .map_err(|e| Error::Other(e.to_string()))?;
        let str_to_id = txn
            .open_table(TABLE_STR_TO_ID)
            .map_err(|e| Error::Other(e.to_string()))?;
        let id_to_str = txn
            .open_table(TABLE_ID_TO_STR)
            .map_err(|e| Error::Other(e.to_string()))?;

        // Get current max ID
        let next_id = id_to_str
            .last()
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|(id, _)| id.value() + 1)
            .unwrap_or(1);
        let string_cache = LruCache::new(
            NonZeroUsize::new(STRING_CACHE_LIMIT).expect("STRING_CACHE_LIMIT must be > 0"),
        );

        Ok(Self {
            spo,
            pos,
            osp,
            str_to_id,
            id_to_str,
            string_cache,
            next_id,
        })
    }

    /// Intern a string using cached table handles
    pub fn intern(&mut self, value: &str) -> Result<u64> {
        if let Some(id) = self.string_cache.get(value).copied() {
            return Ok(id);
        }

        let existing_id = self
            .str_to_id
            .get(value)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|id_guard| id_guard.value());
        if let Some(id) = existing_id {
            self.string_cache.put(value.to_owned(), id);
            return Ok(id);
        }

        let id = self.next_id;
        self.next_id += 1;
        self.str_to_id
            .insert(value, id)
            .map_err(|e| Error::Other(e.to_string()))?;
        self.id_to_str
            .insert(id, value)
            .map_err(|e| Error::Other(e.to_string()))?;
        self.string_cache.put(value.to_owned(), id);
        Ok(id)
    }

    /// Insert a triple using cached table handles
    pub fn insert_triple(&mut self, triple: &Triple) -> Result<bool> {
        let s = triple.subject_id;
        let p = triple.predicate_id;
        let o = triple.object_id;

        match self.spo.insert(make_key(s, p, o), ()) {
            Ok(Some(_)) => return Ok(false),
            Ok(None) => {}
            Err(e) => return Err(Error::Other(e.to_string())),
        }

        self.pos
            .insert(make_key(p, o, s), ())
            .map_err(|e| Error::Other(e.to_string()))?;
        self.osp
            .insert(make_key(o, s, p), ())
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(true)
    }

    /// Insert a fact (intern strings + insert triple)
    pub fn insert_fact(&mut self, fact: Fact<'_>) -> Result<Triple> {
        let s = self.intern(fact.subject)?;
        let p = self.intern(fact.predicate)?;
        let o = self.intern(fact.object)?;
        let triple = Triple::new(s, p, o);
        self.insert_triple(&triple)?;
        Ok(triple)
    }

    pub fn intern_bulk(&mut self, values: &[&str]) -> Result<Vec<u64>> {
        let mut out = Vec::with_capacity(values.len());
        for v in values {
            out.push(self.intern(v)?);
        }
        Ok(out)
    }
}

#[derive(Debug)]
struct ReadHandles {
    _txn: ReadTransaction,
    spo: redb::ReadOnlyTable<TripleKey, ()>,
    pos: redb::ReadOnlyTable<TripleKey, ()>,
    osp: redb::ReadOnlyTable<TripleKey, ()>,
    id_to_str: redb::ReadOnlyTable<u64, &'static str>,
    str_to_id: redb::ReadOnlyTable<&'static str, u64>,
}

impl ReadHandles {
    fn build(db: &Database) -> Result<Self> {
        let txn = db.begin_read().map_err(|e| Error::Other(e.to_string()))?;
        let spo = txn
            .open_table(TABLE_SPO)
            .map_err(|e| Error::Other(e.to_string()))?;
        let pos = txn
            .open_table(TABLE_POS)
            .map_err(|e| Error::Other(e.to_string()))?;
        let osp = txn
            .open_table(TABLE_OSP)
            .map_err(|e| Error::Other(e.to_string()))?;
        let id_to_str = txn
            .open_table(TABLE_ID_TO_STR)
            .map_err(|e| Error::Other(e.to_string()))?;
        let str_to_id = txn
            .open_table(TABLE_STR_TO_ID)
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(Self {
            _txn: txn,
            spo,
            pos,
            osp,
            id_to_str,
            str_to_id,
        })
    }
}

#[self_referencing]
struct CachedCursor {
    handles: Arc<ReadHandles>,
    index: IndexKind,
    start: TripleKey,
    end: TripleKey,
    #[borrows(handles)]
    #[covariant]
    iter: Range<'this, TripleKey, ()>,
}

impl CachedCursor {
    fn create(handles: Arc<ReadHandles>, range: QueryRange) -> Result<Self> {
        let QueryRange { index, start, end } = range;
        #[cfg(feature = "varint-keys")]
        let (start_bounds, end_bounds) = (start.clone(), end.clone());
        #[cfg(not(feature = "varint-keys"))]
        let (start_bounds, end_bounds) = (start, end);
        CachedCursorTryBuilder {
            handles,
            index,
            start,
            end,
            iter_builder: move |handles| {
                let bounds = start_bounds..=end_bounds;
                match index {
                    IndexKind::Spo => handles.spo.range(bounds),
                    IndexKind::Pos => handles.pos.range(bounds),
                    IndexKind::Osp => handles.osp.range(bounds),
                }
                .map_err(|e| Error::Other(e.to_string()))
            },
        }
        .try_build()
    }
}

impl Iterator for CachedCursor {
    type Item = Triple;

    fn next(&mut self) -> Option<Self::Item> {
        let index = *self.borrow_index();
        self.with_iter_mut(|iter| {
            if let Some((key, _)) = iter.by_ref().flatten().next() {
                let raw = decode_key(key.value());
                return Some(index.decode(raw));
            }
            None
        })
    }
}

pub(crate) fn intern_in_txn(txn: &mut redb::WriteTransaction, value: &str) -> Result<u64> {
    let mut str_to_id = txn
        .open_table(TABLE_STR_TO_ID)
        .map_err(|e| Error::Other(e.to_string()))?;
    if let Some(id) = str_to_id
        .get(value)
        .map_err(|e| Error::Other(e.to_string()))?
    {
        return Ok(id.value());
    }

    let mut id_to_str = txn
        .open_table(TABLE_ID_TO_STR)
        .map_err(|e| Error::Other(e.to_string()))?;
    let next_id = id_to_str
        .last()
        .map_err(|e| Error::Other(e.to_string()))?
        .map(|(id, _)| id.value() + 1)
        .unwrap_or(1);

    str_to_id
        .insert(value, next_id)
        .map_err(|e| Error::Other(e.to_string()))?;
    id_to_str
        .insert(next_id, value)
        .map_err(|e| Error::Other(e.to_string()))?;
    Ok(next_id)
}

pub(crate) fn insert_triple(txn: &mut redb::WriteTransaction, triple: &Triple) -> Result<bool> {
    let s = triple.subject_id;
    let p = triple.predicate_id;
    let o = triple.object_id;

    let mut spo = txn
        .open_table(TABLE_SPO)
        .map_err(|e| Error::Other(e.to_string()))?;

    match spo.insert(make_key(s, p, o), ()) {
        Ok(Some(_)) => return Ok(false),
        Ok(None) => {}
        Err(e) => return Err(Error::Other(e.to_string())),
    }

    let mut pos = txn
        .open_table(TABLE_POS)
        .map_err(|e| Error::Other(e.to_string()))?;
    pos.insert(make_key(p, o, s), ())
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut osp = txn
        .open_table(TABLE_OSP)
        .map_err(|e| Error::Other(e.to_string()))?;
    osp.insert(make_key(o, s, p), ())
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(true)
}

#[derive(Clone, Copy)]
enum IndexKind {
    Spo,
    Pos,
    Osp,
}

impl IndexKind {
    fn decode(self, raw: (u64, u64, u64)) -> Triple {
        match self {
            IndexKind::Spo => Triple::new(raw.0, raw.1, raw.2),
            IndexKind::Pos => Triple::new(raw.2, raw.0, raw.1),
            IndexKind::Osp => Triple::new(raw.1, raw.2, raw.0),
        }
    }
}

struct QueryRange {
    index: IndexKind,
    start: TripleKey,
    end: TripleKey,
}

enum QuerySpec {
    Exact(Triple),
    Range(QueryRange),
}

impl QuerySpec {
    fn range(index: IndexKind, start: (u64, u64, u64), end: (u64, u64, u64)) -> Self {
        QuerySpec::Range(QueryRange {
            index,
            start: make_key(start.0, start.1, start.2),
            end: make_key(end.0, end.1, end.2),
        })
    }
}

pub(crate) fn delete_triple(txn: &mut redb::WriteTransaction, triple: &Triple) -> Result<bool> {
    let s = triple.subject_id;
    let p = triple.predicate_id;
    let o = triple.object_id;

    let mut spo = txn
        .open_table(TABLE_SPO)
        .map_err(|e| Error::Other(e.to_string()))?;

    if spo
        .get(make_key(s, p, o))
        .map_err(|e| Error::Other(e.to_string()))?
        .is_none()
    {
        return Ok(false);
    }

    spo.remove(make_key(s, p, o))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut pos = txn
        .open_table(TABLE_POS)
        .map_err(|e| Error::Other(e.to_string()))?;
    pos.remove(make_key(p, o, s))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut osp = txn
        .open_table(TABLE_OSP)
        .map_err(|e| Error::Other(e.to_string()))?;
    osp.remove(make_key(o, s, p))
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(true)
}
