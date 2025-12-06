use std::sync::Arc;

use ouroboros::self_referencing;
use redb::{
    Database, Range, ReadTransaction, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition,
};

use crate::error::{Error, Result};
use crate::storage::schema::{
    TABLE_EDGE_PROPS, TABLE_EDGE_PROPS_BINARY, TABLE_ID_TO_STR, TABLE_META, TABLE_NODE_PROPS,
    TABLE_NODE_PROPS_BINARY, TABLE_OSP, TABLE_POS, TABLE_PSO, TABLE_SOP, TABLE_SPO,
    TABLE_STR_TO_ID,
};
use crate::storage::{Hexastore, HexastoreIter};
use crate::triple::{Fact, Triple};
#[derive(Debug)]
pub struct DiskHexastore {
    db: Arc<Database>,
}

impl DiskHexastore {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Self::init_tables(&db)?;
        Ok(Self { db })
    }

    fn init_tables(db: &Database) -> Result<()> {
        let write_txn = db.begin_write().map_err(|e| Error::Other(e.to_string()))?;
        {
            let _ = write_txn
                .open_table(TABLE_SPO)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_SOP)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_POS)
                .map_err(|e| Error::Other(e.to_string()))?;
            let _ = write_txn
                .open_table(TABLE_PSO)
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
                QuerySpec::range(IndexKind::Sop, (s, o, u64::MIN), (s, o, u64::MAX))
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
                IndexKind::Pso,
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

    fn lookup_exact(&self, triple: &Triple) -> Result<bool> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = txn
            .open_table(TABLE_SPO)
            .map_err(|e| Error::Other(e.to_string()))?;
        table
            .get((triple.subject_id, triple.predicate_id, triple.object_id))
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
        Ok(deleted)
    }

    fn insert_fact(&mut self, fact: Fact<'_>) -> Result<Triple> {
        let mut write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let s = intern_in_txn(&mut write_txn, fact.subject)?;
        let p = intern_in_txn(&mut write_txn, fact.predicate)?;
        let o = intern_in_txn(&mut write_txn, fact.object)?;
        let triple = Triple::new(s, p, o);
        insert_triple(&mut write_txn, &triple)?;
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
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
            QuerySpec::Range(range) => match DiskCursor::create(&self.db, range) {
                Ok(cursor) => Box::new(cursor),
                Err(_) => Box::new(std::iter::empty()),
            },
        }
    }

    fn resolve_str(&self, id: u64) -> Result<Option<String>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = txn
            .open_table(TABLE_ID_TO_STR)
            .map_err(|e| Error::Other(e.to_string()))?;
        let result = table
            .get(id)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value().to_string());
        Ok(result)
    }

    fn resolve_id(&self, value: &str) -> Result<Option<u64>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Other(e.to_string()))?;
        let table = txn
            .open_table(TABLE_STR_TO_ID)
            .map_err(|e| Error::Other(e.to_string()))?;
        let result = table
            .get(value)
            .map_err(|e| Error::Other(e.to_string()))?
            .map(|v| v.value());
        Ok(result)
    }

    fn intern(&mut self, value: &str) -> Result<u64> {
        let mut write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        let id = intern_in_txn(&mut write_txn, value)?;
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
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

        let mut write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut count = 0;
        for triple in triples {
            if insert_triple(&mut write_txn, triple)? {
                count += 1;
            }
        }

        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;
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

    if spo
        .get((s, p, o))
        .map_err(|e| Error::Other(e.to_string()))?
        .is_some()
    {
        return Ok(false);
    }

    spo.insert((s, p, o), ())
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut sop = txn
        .open_table(TABLE_SOP)
        .map_err(|e| Error::Other(e.to_string()))?;
    sop.insert((s, o, p), ())
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut pos = txn
        .open_table(TABLE_POS)
        .map_err(|e| Error::Other(e.to_string()))?;
    pos.insert((p, o, s), ())
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut pso = txn
        .open_table(TABLE_PSO)
        .map_err(|e| Error::Other(e.to_string()))?;
    pso.insert((p, s, o), ())
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut osp = txn
        .open_table(TABLE_OSP)
        .map_err(|e| Error::Other(e.to_string()))?;
    osp.insert((o, s, p), ())
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(true)
}

#[derive(Clone, Copy)]
enum IndexKind {
    Spo,
    Sop,
    Pos,
    Pso,
    Osp,
}

impl IndexKind {
    fn table(self) -> TableDefinition<'static, (u64, u64, u64), ()> {
        match self {
            IndexKind::Spo => TABLE_SPO,
            IndexKind::Sop => TABLE_SOP,
            IndexKind::Pos => TABLE_POS,
            IndexKind::Pso => TABLE_PSO,
            IndexKind::Osp => TABLE_OSP,
        }
    }

    fn decode(self, raw: (u64, u64, u64)) -> Triple {
        match self {
            IndexKind::Spo => Triple::new(raw.0, raw.1, raw.2),
            IndexKind::Sop => Triple::new(raw.0, raw.2, raw.1),
            IndexKind::Pos => Triple::new(raw.2, raw.0, raw.1),
            IndexKind::Pso => Triple::new(raw.1, raw.0, raw.2),
            IndexKind::Osp => Triple::new(raw.1, raw.2, raw.0),
        }
    }
}

struct QueryRange {
    index: IndexKind,
    start: (u64, u64, u64),
    end: (u64, u64, u64),
}

enum QuerySpec {
    Exact(Triple),
    Range(QueryRange),
}

impl QuerySpec {
    fn range(index: IndexKind, start: (u64, u64, u64), end: (u64, u64, u64)) -> Self {
        QuerySpec::Range(QueryRange { index, start, end })
    }
}

#[self_referencing]
struct DiskCursor {
    txn: ReadTransaction,
    table: redb::ReadOnlyTable<(u64, u64, u64), ()>,
    #[borrows(table)]
    #[covariant]
    iter: Range<'this, (u64, u64, u64), ()>,
    index: IndexKind,
}

impl DiskCursor {
    fn create(db: &Database, range: QueryRange) -> Result<Self> {
        let QueryRange { index, start, end } = range;
        let bounds = start..=end;
        let txn = db.begin_read().map_err(|e| Error::Other(e.to_string()))?;
        let table = txn
            .open_table(index.table())
            .map_err(|e| Error::Other(e.to_string()))?;
        DiskCursorTryBuilder {
            txn,
            table,
            iter_builder: move |table| {
                table
                    .range(bounds.clone())
                    .map_err(|e| Error::Other(e.to_string()))
            },
            index,
        }
        .try_build()
    }
}

impl Iterator for DiskCursor {
    type Item = Triple;

    fn next(&mut self) -> Option<Self::Item> {
        let index = *self.borrow_index();
        self.with_iter_mut(|iter| {
            while let Some(entry) = iter.next() {
                if let Ok((key, _)) = entry {
                    let raw = key.value();
                    return Some(index.decode(raw));
                }
            }
            None
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
        .get((s, p, o))
        .map_err(|e| Error::Other(e.to_string()))?
        .is_none()
    {
        return Ok(false);
    }

    spo.remove((s, p, o))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut sop = txn
        .open_table(TABLE_SOP)
        .map_err(|e| Error::Other(e.to_string()))?;
    sop.remove((s, o, p))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut pos = txn
        .open_table(TABLE_POS)
        .map_err(|e| Error::Other(e.to_string()))?;
    pos.remove((p, o, s))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut pso = txn
        .open_table(TABLE_PSO)
        .map_err(|e| Error::Other(e.to_string()))?;
    pso.remove((p, s, o))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut osp = txn
        .open_table(TABLE_OSP)
        .map_err(|e| Error::Other(e.to_string()))?;
    osp.remove((o, s, p))
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(true)
}
