use std::path::Path;

use ouroboros::self_referencing;
use redb::{Database, Range, ReadTransaction, ReadableDatabase, ReadableTable, TableDefinition};

use crate::error::{Error, Result};
use crate::storage::{Hexastore, HexastoreIter};
use crate::triple::Triple;

const TABLE_SPO: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("spo");
const TABLE_SOP: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("sop");
const TABLE_POS: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("pos");
const TABLE_PSO: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("pso");
const TABLE_OSP: TableDefinition<(u64, u64, u64), ()> = TableDefinition::new("osp");
#[derive(Debug)]
pub struct DiskHexastore {
    db: Database,
}

impl DiskHexastore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::create(path)
            .map_err(|e| Error::Other(format!("failed to open redb: {e}")))?;
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
        let s = triple.subject_id;
        let p = triple.predicate_id;
        let o = triple.object_id;

        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut spo = write_txn
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

            let mut sop = write_txn
                .open_table(TABLE_SOP)
                .map_err(|e| Error::Other(e.to_string()))?;
            sop.insert((s, o, p), ())
                .map_err(|e| Error::Other(e.to_string()))?;

            let mut pos = write_txn
                .open_table(TABLE_POS)
                .map_err(|e| Error::Other(e.to_string()))?;
            pos.insert((p, o, s), ())
                .map_err(|e| Error::Other(e.to_string()))?;

            let mut pso = write_txn
                .open_table(TABLE_PSO)
                .map_err(|e| Error::Other(e.to_string()))?;
            pso.insert((p, s, o), ())
                .map_err(|e| Error::Other(e.to_string()))?;

            let mut osp = write_txn
                .open_table(TABLE_OSP)
                .map_err(|e| Error::Other(e.to_string()))?;
            osp.insert((o, s, p), ())
                .map_err(|e| Error::Other(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(true)
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
