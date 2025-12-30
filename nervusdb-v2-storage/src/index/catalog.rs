use crate::PAGE_SIZE;
use crate::error::{Error, Result};
use crate::index::btree::BTree;
use crate::pager::{PageId, Pager};
use std::collections::BTreeMap;

const MAGIC: [u8; 8] = *b"NDBXCAT1";
const HEADER_SIZE: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    pub id: u32,
    pub root: PageId,
}

/// Index catalog persisted inside the pager.
///
/// MVP: single-page catalog that is rewritten atomically on update.
#[derive(Debug)]
pub struct IndexCatalog {
    page: PageId,
    pub(crate) entries: BTreeMap<String, IndexDef>,
}

impl IndexCatalog {
    pub fn open_or_create(pager: &mut Pager) -> Result<Self> {
        let page = match pager.index_catalog_root() {
            Some(p) => p,
            None => {
                let p = pager.allocate_page()?;
                let mut buf = [0u8; PAGE_SIZE];
                init_empty_catalog_page(&mut buf);
                pager.write_page(p, &buf)?;
                pager.set_index_catalog_root(Some(p))?;
                p
            }
        };

        let buf = pager.read_page(page)?;
        let entries = decode_catalog_page(&buf)?;
        Ok(Self { page, entries })
    }

    pub fn get(&self, name: &str) -> Option<&IndexDef> {
        self.entries.get(name)
    }

    pub fn get_or_create(&mut self, pager: &mut Pager, name: &str) -> Result<IndexDef> {
        if let Some(def) = self.entries.get(name) {
            return Ok(def.clone());
        }

        let id = pager.allocate_index_id()?;
        let tree = BTree::create(pager)?;
        let def = IndexDef {
            id,
            root: tree.root(),
        };
        self.entries.insert(name.to_string(), def.clone());
        self.flush(pager)?;
        Ok(def)
    }

    pub fn update_root(&mut self, pager: &mut Pager, name: &str, new_root: PageId) -> Result<()> {
        let Some(def) = self.entries.get_mut(name) else {
            return Err(Error::WalProtocol("index catalog: missing entry"));
        };
        def.root = new_root;
        self.flush(pager)
    }

    pub fn flush(&self, pager: &mut Pager) -> Result<()> {
        let mut buf = [0u8; PAGE_SIZE];
        encode_catalog_page(&self.entries, &mut buf)?;
        pager.write_page(self.page, &buf)?;
        Ok(())
    }
}

fn init_empty_catalog_page(buf: &mut [u8; PAGE_SIZE]) {
    buf.fill(0);
    buf[0..8].copy_from_slice(&MAGIC);
    buf[8..10].copy_from_slice(&0u16.to_le_bytes());
}

fn decode_catalog_page(buf: &[u8; PAGE_SIZE]) -> Result<BTreeMap<String, IndexDef>> {
    if buf[0..8] != MAGIC {
        return Err(Error::WalProtocol("index catalog: bad magic"));
    }
    let count = u16::from_le_bytes(buf[8..10].try_into().unwrap()) as usize;
    let mut off = HEADER_SIZE;
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        if off + 2 > PAGE_SIZE {
            return Err(Error::WalProtocol("index catalog: truncated"));
        }
        let name_len = u16::from_le_bytes(buf[off..off + 2].try_into().unwrap()) as usize;
        off += 2;
        if off + name_len + 4 + 8 > PAGE_SIZE {
            return Err(Error::WalProtocol("index catalog: truncated"));
        }
        let name = std::str::from_utf8(&buf[off..off + name_len])
            .map_err(|_| Error::WalProtocol("index catalog: invalid utf8"))?
            .to_string();
        off += name_len;
        let id = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let root = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        off += 8;
        entries.insert(
            name,
            IndexDef {
                id,
                root: PageId::new(root),
            },
        );
    }
    Ok(entries)
}

fn encode_catalog_page(
    entries: &BTreeMap<String, IndexDef>,
    out: &mut [u8; PAGE_SIZE],
) -> Result<()> {
    out.fill(0);
    out[0..8].copy_from_slice(&MAGIC);
    let count = u16::try_from(entries.len())
        .map_err(|_| Error::WalProtocol("index catalog: too many entries"))?;
    out[8..10].copy_from_slice(&count.to_le_bytes());

    let mut off = HEADER_SIZE;
    for (name, def) in entries {
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| Error::WalProtocol("index catalog: name too long"))?;
        let rec_len = 2 + name_bytes.len() + 4 + 8;
        if off + rec_len > PAGE_SIZE {
            return Err(Error::WalProtocol("index catalog: page full"));
        }
        out[off..off + 2].copy_from_slice(&name_len.to_le_bytes());
        off += 2;
        out[off..off + name_bytes.len()].copy_from_slice(name_bytes);
        off += name_bytes.len();
        out[off..off + 4].copy_from_slice(&def.id.to_le_bytes());
        off += 4;
        out[off..off + 8].copy_from_slice(&def.root.as_u64().to_le_bytes());
        off += 8;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::ordered_key::{encode_index_key, encode_ordered_value};
    use crate::property::PropertyValue;
    use tempfile::tempdir;

    #[test]
    fn catalog_persists_across_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cat.ndb");
        {
            let mut pager = Pager::open(&path).unwrap();
            let mut cat = IndexCatalog::open_or_create(&mut pager).unwrap();
            let a = cat.get_or_create(&mut pager, "age").unwrap();
            let b = cat.get_or_create(&mut pager, "name").unwrap();
            assert_ne!(a.id, b.id);
            assert_ne!(a.root, b.root);
        }

        let mut pager = Pager::open(&path).unwrap();
        let cat = IndexCatalog::open_or_create(&mut pager).unwrap();
        assert!(cat.get("age").is_some());
        assert!(cat.get("name").is_some());
    }

    #[test]
    fn equality_seek_and_delete_via_rebuild() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cat2.ndb");
        let mut pager = Pager::open(&path).unwrap();
        let mut cat = IndexCatalog::open_or_create(&mut pager).unwrap();
        let def = cat.get_or_create(&mut pager, "age").unwrap();

        let mut tree = BTree::load(def.root);
        for (id, age) in [(10u64, 30i64), (11, 30), (12, 31)] {
            let k = encode_index_key(def.id, &PropertyValue::Int(age), id);
            tree.insert(&mut pager, &k, id).unwrap();
        }
        cat.update_root(&mut pager, "age", tree.root()).unwrap();

        let prefix = {
            let mut p = Vec::new();
            p.extend_from_slice(&def.id.to_be_bytes());
            p.extend_from_slice(&encode_ordered_value(&PropertyValue::Int(30)));
            p
        };

        let mut cur = tree.cursor_lower_bound(&mut pager, &prefix).unwrap();
        let mut got = Vec::new();
        while cur.is_valid().unwrap() {
            let k = cur.key().unwrap();
            if !k.starts_with(&prefix) {
                break;
            }
            got.push(cur.payload().unwrap());
            if !cur.advance().unwrap() {
                break;
            }
        }
        got.sort();
        assert_eq!(got, vec![10, 11]);

        // Delete one tuple by rebuilding the tree (implemented in btree.rs).
        let del_key = encode_index_key(def.id, &PropertyValue::Int(30), 10);
        assert!(tree.delete_exact_rebuild(&mut pager, &del_key, 10).unwrap());
        cat.update_root(&mut pager, "age", tree.root()).unwrap();

        let mut cur = tree.cursor_lower_bound(&mut pager, &prefix).unwrap();
        let mut got2 = Vec::new();
        while cur.is_valid().unwrap() {
            let k = cur.key().unwrap();
            if !k.starts_with(&prefix) {
                break;
            }
            got2.push(cur.payload().unwrap());
            if !cur.advance().unwrap() {
                break;
            }
        }
        assert_eq!(got2, vec![11]);
    }
}
