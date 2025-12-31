use crate::index::btree::BTree;
use crate::index::catalog::IndexCatalog;
use crate::pager::{PageId, Pager};
use crate::wal::{CommittedTx, SegmentPointer, WalRecord};
use crate::{Error, PAGE_SIZE, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct VacuumReport {
    pub ndb_path: PathBuf,
    pub backup_path: PathBuf,
    pub old_next_page_id: u64,
    pub new_next_page_id: u64,
    pub copied_data_pages: u64,
    pub old_file_pages: u64,
    pub new_file_pages: u64,
}

pub fn vacuum_in_place(
    ndb_path: impl AsRef<Path>,
    wal_path: impl AsRef<Path>,
) -> Result<VacuumReport> {
    let ndb_path = ndb_path.as_ref();
    let wal_path = wal_path.as_ref();

    if !ndb_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("ndb file not found: {}", ndb_path.display()),
        )));
    }

    let committed = if wal_path.exists() {
        crate::wal::Wal::replay_committed_from_path(wal_path)?
    } else {
        Vec::new()
    };
    let roots = scan_wal_roots(&committed);

    let pager = Pager::open(ndb_path)?;
    let reachable = mark_reachable_pages(&pager, &roots)?;

    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tmp_path = ndb_path.with_extension(format!("ndb.vacuum.tmp.{pid}.{nonce}"));
    let backup_path = ndb_path.with_extension(format!("ndb.bak.{pid}.{nonce}"));

    let stats = pager.write_vacuum_copy(&tmp_path, &reachable)?;
    drop(pager);

    std::fs::rename(ndb_path, &backup_path).map_err(Error::Io)?;
    if let Err(e) = std::fs::rename(&tmp_path, ndb_path) {
        let _ = std::fs::rename(&backup_path, ndb_path);
        let _ = std::fs::remove_file(&tmp_path);
        return Err(Error::Io(e));
    }

    Ok(VacuumReport {
        ndb_path: ndb_path.to_path_buf(),
        backup_path,
        old_next_page_id: stats.old_next_page_id,
        new_next_page_id: stats.new_next_page_id,
        copied_data_pages: stats.copied_data_pages,
        old_file_pages: stats.old_file_pages,
        new_file_pages: stats.new_file_pages,
    })
}

#[derive(Debug, Default)]
struct WalRoots {
    manifest_epoch: u64,
    segments: Vec<SegmentPointer>,
    properties_root: u64,
    stats_root: u64,
}

fn scan_wal_roots(committed: &[CommittedTx]) -> WalRoots {
    let mut state = WalRoots::default();
    for tx in committed {
        for op in &tx.ops {
            match op {
                WalRecord::ManifestSwitch {
                    epoch,
                    segments,
                    properties_root,
                    stats_root,
                } => {
                    if *epoch >= state.manifest_epoch {
                        state.manifest_epoch = *epoch;
                        state.segments = segments.clone();
                        state.properties_root = *properties_root;
                        state.stats_root = *stats_root;
                    }
                }
                WalRecord::Checkpoint {
                    epoch,
                    properties_root,
                    stats_root,
                    ..
                } => {
                    if *epoch == state.manifest_epoch {
                        state.properties_root = *properties_root;
                        state.stats_root = *stats_root;
                    }
                }
                _ => {}
            }
        }
    }
    state
}

fn mark_reachable_pages(pager: &Pager, roots: &WalRoots) -> Result<BTreeSet<PageId>> {
    let mut reachable: BTreeSet<PageId> = BTreeSet::new();
    // Always keep meta + bitmap pages.
    reachable.insert(PageId::new(0));
    reachable.insert(PageId::new(1));

    // IdMap i2e pages are rooted in meta.
    if let Some(start) = pager.i2e_start_page() {
        let len = pager.i2e_len();
        if len > 0 {
            const I2E_RECORD_SIZE: u64 = 16;
            let records_per_page = (PAGE_SIZE as u64) / I2E_RECORD_SIZE;
            let page_count = (len + records_per_page - 1) / records_per_page;
            for i in 0..page_count {
                reachable.insert(PageId::new(start.as_u64() + i));
            }
        }
    }

    // Index catalog page + all index tree pages.
    if let Some(page) = pager.index_catalog_root() {
        reachable.insert(page);
    }

    if let Some(catalog) = IndexCatalog::open_existing(pager)? {
        for (name, def) in &catalog.entries {
            if def.root.as_u64() == 0 {
                continue;
            }

            let tree = BTree::load(def.root);
            let mut payloads = Vec::new();
            let collect_payloads = name == "__sys_hnsw_vec" || name == "__sys_hnsw_graph";
            if collect_payloads {
                tree.mark_reachable_pages(pager, &mut reachable, Some(&mut payloads))?;
                for blob_id in payloads {
                    mark_blob_chain(pager, blob_id, &mut reachable)?;
                }
            } else {
                tree.mark_reachable_pages(pager, &mut reachable, None)?;
            }
        }
    }

    // Properties store B-Tree (payloads are blob ids).
    if roots.properties_root != 0 {
        let tree = BTree::load(PageId::new(roots.properties_root));
        let mut payloads = Vec::new();
        tree.mark_reachable_pages(pager, &mut reachable, Some(&mut payloads))?;
        for blob_id in payloads {
            mark_blob_chain(pager, blob_id, &mut reachable)?;
        }
    }

    // Graph statistics (blob chain).
    if roots.stats_root != 0 {
        mark_blob_chain(pager, roots.stats_root, &mut reachable)?;
    }

    // CSR segments from the current manifest.
    for seg in &roots.segments {
        if seg.meta_page_id == 0 {
            continue;
        }
        let meta = PageId::new(seg.meta_page_id);
        reachable.insert(meta);
        mark_csr_segment_pages(pager, meta, &mut reachable)?;
    }

    Ok(reachable)
}

fn mark_blob_chain(
    pager: &Pager,
    mut page_id: u64,
    reachable: &mut BTreeSet<PageId>,
) -> Result<()> {
    const HEADER_SIZE: usize = 8 + 2;
    const MAX_DATA_PER_PAGE: usize = PAGE_SIZE - HEADER_SIZE;

    while page_id != 0 {
        let pid = PageId::new(page_id);
        if !reachable.insert(pid) {
            return Err(Error::StorageCorrupted("cycle detected in blob chain"));
        }

        let page = pager.read_page(pid)?;
        let next_page_id = u64::from_le_bytes(page[0..8].try_into().unwrap());
        let data_len = u16::from_le_bytes(page[8..10].try_into().unwrap()) as usize;
        if data_len > MAX_DATA_PER_PAGE {
            return Err(Error::StorageCorrupted("invalid blob page data length"));
        }

        page_id = next_page_id;
    }

    Ok(())
}

fn mark_csr_segment_pages(
    pager: &Pager,
    meta_page_id: PageId,
    reachable: &mut BTreeSet<PageId>,
) -> Result<()> {
    const META_MAGIC: [u8; 8] = *b"NDBCSRv1";

    let meta = pager.read_page(meta_page_id)?;
    if meta[0..8] != META_MAGIC {
        return Err(Error::WalProtocol("invalid csr meta magic"));
    }

    let offsets_page_count = u32::from_le_bytes(meta[40..44].try_into().unwrap()) as usize;
    let edges_page_count = u32::from_le_bytes(meta[44..48].try_into().unwrap()) as usize;

    let needed = 48usize + (offsets_page_count + edges_page_count) * 8;
    if needed > PAGE_SIZE {
        return Err(Error::WalProtocol("csr meta page overflow"));
    }

    let mut off = 48usize;
    for _ in 0..offsets_page_count {
        let id = u64::from_le_bytes(meta[off..off + 8].try_into().unwrap());
        off += 8;
        if id != 0 {
            reachable.insert(PageId::new(id));
        }
    }
    for _ in 0..edges_page_count {
        let id = u64::from_le_bytes(meta[off..off + 8].try_into().unwrap());
        off += 8;
        if id != 0 {
            reachable.insert(PageId::new(id));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob_store::BlobStore;
    use crate::index::ordered_key::encode_ordered_value;
    use crate::property::PropertyValue;
    use tempfile::tempdir;

    #[test]
    fn t204_vacuum_reclaims_orphan_blob_pages() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("graph.ndb");
        let wal = dir.path().join("graph.wal");

        let orphan_blob_id;
        {
            let mut pager = Pager::open(&ndb).unwrap();
            let mut catalog = IndexCatalog::open_or_create(&mut pager).unwrap();

            // Reserve the HNSW vector index root; payloads are blob ids.
            let vec_def = catalog.get_or_create(&mut pager, "__sys_hnsw_vec").unwrap();

            // 1) Create an orphan blob page chain (allocated but unreachable).
            orphan_blob_id = BlobStore::write(&mut pager, b"orphan").unwrap();

            // 2) Create a reachable blob by inserting into the reserved index.
            let reachable_blob_id = BlobStore::write(&mut pager, b"reachable").unwrap();
            let mut tree = BTree::load(vec_def.root);
            let mut key = Vec::new();
            key.push(2u8); // TAG_VECTOR
            key.extend_from_slice(&1u32.to_be_bytes());
            key.extend_from_slice(&encode_ordered_value(&PropertyValue::Int(1)));
            tree.insert(&mut pager, &key, reachable_blob_id).unwrap();
            catalog
                .update_root(&mut pager, "__sys_hnsw_vec", tree.root())
                .unwrap();
        }

        let report = vacuum_in_place(&ndb, &wal).unwrap();
        assert!(report.backup_path.exists());

        // The orphan page should now be free and reused by the allocator.
        let mut pager = Pager::open(&ndb).unwrap();
        let pid = pager.allocate_page().unwrap();
        assert_eq!(pid.as_u64(), orphan_blob_id);
    }
}
