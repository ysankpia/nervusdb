use crate::pager::{PageId, Pager};
use crate::{Error, PAGE_SIZE, Result};
use std::collections::HashMap;

pub type ExternalId = u64;
pub type InternalNodeId = u32;
pub type LabelId = u32;

const I2E_RECORD_SIZE: usize = 16;
const I2E_RECORDS_PER_PAGE: usize = PAGE_SIZE / I2E_RECORD_SIZE;
const FIRST_DATA_PAGE_ID_U64: u64 = 2;
const MAX_DATA_PAGES: u64 = (PAGE_SIZE as u64) * 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct I2eRecord {
    pub external_id: ExternalId,
    pub label_id: LabelId,
    pub flags: u32,
}

impl I2eRecord {
    pub fn encode(self) -> [u8; I2E_RECORD_SIZE] {
        let mut out = [0u8; I2E_RECORD_SIZE];
        out[0..8].copy_from_slice(&self.external_id.to_le_bytes());
        out[8..12].copy_from_slice(&self.label_id.to_le_bytes());
        out[12..16].copy_from_slice(&self.flags.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8; I2E_RECORD_SIZE]) -> Self {
        let external_id = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let label_id = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let flags = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        Self {
            external_id,
            label_id,
            flags,
        }
    }
}

#[derive(Debug)]
pub struct IdMap {
    e2i: HashMap<ExternalId, InternalNodeId>,
    /// Internal node ID → Label IDs (sorted, deduplicated)
    i2l: Vec<Vec<LabelId>>,
    i2e: Vec<I2eRecord>,
    i2e_start: Option<PageId>,
    i2e_len: u64,
}

impl IdMap {
    pub fn load(pager: &mut Pager) -> Result<Self> {
        let i2e_start = pager.i2e_start_page();
        let i2e_len = pager.i2e_len();

        let mut e2i = HashMap::with_capacity(i2e_len as usize);
        let mut i2l = Vec::with_capacity(i2e_len as usize);
        let mut i2e = Vec::with_capacity(i2e_len as usize);

        if let Some(start) = i2e_start {
            for internal_id_u64 in 0..i2e_len {
                let record = read_i2e_record(pager, start, internal_id_u64)?;
                i2e.push(record);
                if record.external_id != 0 {
                    e2i.insert(record.external_id, internal_id_u64 as u32);
                }
                // Convert single label to vec for backward compat
                i2l.push(vec![record.label_id]);
            }
        }

        Ok(Self {
            e2i,
            i2l,
            i2e,
            i2e_start,
            i2e_len,
        })
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.i2e_len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.i2e_len == 0
    }

    #[inline]
    pub fn next_internal_id(&self) -> InternalNodeId {
        u32::try_from(self.i2e_len).unwrap_or(u32::MAX)
    }

    #[inline]
    pub fn lookup(&self, external_id: ExternalId) -> Option<InternalNodeId> {
        self.e2i.get(&external_id).copied()
    }

    /// Get the first label ID for an internal node ID (backward compat).
    #[deprecated(note = "Use get_labels() for multi-label support")]
    #[inline]
    pub fn get_label(&self, internal_id: InternalNodeId) -> Option<LabelId> {
        self.i2l.get(internal_id as usize)?.first().copied()
    }

    /// Get all label IDs for an internal node ID.
    #[inline]
    pub fn get_labels(&self, internal_id: InternalNodeId) -> Option<Vec<LabelId>> {
        self.i2l.get(internal_id as usize).cloned()
    }

    /// Get the entire label mapping vector (for snapshotting).
    pub fn get_i2l_snapshot(&self) -> Vec<Vec<LabelId>> {
        self.i2l.clone()
    }

    pub fn get_i2e_snapshot(&self) -> Vec<I2eRecord> {
        self.i2e.clone()
    }

    /// Create node with single label (backward compat).
    pub fn apply_create_node(
        &mut self,
        pager: &mut Pager,
        external_id: ExternalId,
        label_id: LabelId,
        internal_id: InternalNodeId,
    ) -> Result<()> {
        self.apply_create_node_multi_label(pager, external_id, vec![label_id], internal_id)
    }

    /// Create node with multiple labels.
    pub fn apply_create_node_multi_label(
        &mut self,
        pager: &mut Pager,
        external_id: ExternalId,
        mut labels: Vec<LabelId>,
        internal_id: InternalNodeId,
    ) -> Result<()> {
        let expected = self.next_internal_id();
        if internal_id != expected {
            return Err(Error::WalProtocol("non-dense internal id"));
        }
        if self.e2i.contains_key(&external_id) {
            return Err(Error::WalProtocol("duplicate external id"));
        }

        // Sort and deduplicate labels
        labels.sort_unstable();
        labels.dedup();

        let start = match self.i2e_start {
            Some(p) => p,
            None => {
                let p = pager.allocate_page()?;
                pager.set_i2e_start_page(Some(p))?;
                self.i2e_start = Some(p);
                p
            }
        };

        // For now, only persist first label in I2E (backward compat)
        let first_label = labels.first().copied().unwrap_or(0);
        let record = I2eRecord {
            external_id,
            label_id: first_label,
            flags: 0,
        };

        let current_pages = if self.i2e_start.is_some() {
            required_i2e_pages(self.i2e_len).max(1)
        } else {
            0
        };
        let required_pages = required_i2e_pages(self.i2e_len + 1);

        let start = if required_pages > current_pages {
            let target = PageId::new(start.as_u64() + (required_pages - 1) as u64);
            if !pager.is_page_allocated(target) {
                pager.ensure_allocated(target)?;
                write_i2e_record(pager, start, internal_id as u64, record)?;
                start
            } else {
                self.relocate_i2e_and_write_record(
                    pager,
                    start,
                    current_pages,
                    required_pages,
                    internal_id as u64,
                    record,
                )?
            }
        } else {
            write_i2e_record(pager, start, internal_id as u64, record)?;
            start
        };
        self.i2e_start = Some(start);

        self.i2e_len += 1;
        pager.set_i2e_len(self.i2e_len)?;
        pager.set_next_internal_id(self.next_internal_id())?;

        self.e2i.insert(external_id, internal_id);
        self.i2l.push(labels.clone());
        self.i2e.push(I2eRecord {
            external_id,
            label_id: first_label,
            flags: 0,
        });
        Ok(())
    }

    /// Add a label to an existing node.
    pub fn apply_add_label(
        &mut self,
        _pager: &mut Pager,
        internal_id: InternalNodeId,
        label: LabelId,
    ) -> Result<()> {
        let labels = self
            .i2l
            .get_mut(internal_id as usize)
            .ok_or(Error::WalProtocol("node not found"))?;

        if !labels.contains(&label) {
            labels.push(label);
            labels.sort_unstable();
        }
        Ok(())
    }

    /// Remove a label from an existing node.
    pub fn apply_remove_label(
        &mut self,
        _pager: &mut Pager,
        internal_id: InternalNodeId,
        label: LabelId,
    ) -> Result<()> {
        let labels = self
            .i2l
            .get_mut(internal_id as usize)
            .ok_or(Error::WalProtocol("node not found"))?;

        labels.retain(|&l| l != label);
        Ok(())
    }

    fn relocate_i2e_and_write_record(
        &mut self,
        pager: &mut Pager,
        old_start: PageId,
        used_pages: usize,
        required_pages: usize,
        internal_id_u64: u64,
        record: I2eRecord,
    ) -> Result<PageId> {
        let new_start = find_contiguous_free(pager, required_pages)
            .ok_or(Error::WalProtocol("I2E extent collision: no contiguous free extent"))?;

        for offset in 0..required_pages {
            let page_id = PageId::new(new_start.as_u64() + offset as u64);
            pager.ensure_allocated(page_id)?;
        }

        for offset in 0..used_pages {
            let old_page_id = PageId::new(old_start.as_u64() + offset as u64);
            let new_page_id = PageId::new(new_start.as_u64() + offset as u64);
            let page = pager.read_page(old_page_id)?;
            pager.write_page(new_page_id, &page)?;
        }

        pager.set_i2e_start_page(Some(new_start))?;
        self.i2e_start = Some(new_start);
        write_i2e_record(pager, new_start, internal_id_u64, record)?;

        for offset in 0..used_pages {
            let old_page_id = PageId::new(old_start.as_u64() + offset as u64);
            pager.free_page(old_page_id)?;
        }

        Ok(new_start)
    }
}

fn required_i2e_pages(len: u64) -> usize {
    if len == 0 {
        0
    } else {
        ((len as usize - 1) / I2E_RECORDS_PER_PAGE) + 1
    }
}

fn find_contiguous_free(pager: &Pager, count: usize) -> Option<PageId> {
    if count == 0 {
        return None;
    }

    let file_pages = std::fs::metadata(pager.path())
        .ok()?
        .len()
        .checked_div(PAGE_SIZE as u64)?
        .max(FIRST_DATA_PAGE_ID_U64);

    scan_for_contiguous_free(pager, file_pages, MAX_DATA_PAGES, count)
        .or_else(|| scan_for_contiguous_free(pager, FIRST_DATA_PAGE_ID_U64, file_pages, count))
}

fn scan_for_contiguous_free(pager: &Pager, start: u64, end: u64, count: usize) -> Option<PageId> {
    let count_u64 = u64::try_from(count).ok()?;
    if start >= end || end.saturating_sub(start) < count_u64 {
        return None;
    }

    let mut run_start = 0u64;
    let mut run_len = 0u64;

    for page_id in start..end {
        let allocated = pager.is_page_allocated(PageId::new(page_id));
        if allocated {
            run_len = 0;
            continue;
        }

        if run_len == 0 {
            run_start = page_id;
        }
        run_len += 1;
        if run_len == count_u64 {
            return Some(PageId::new(run_start));
        }
    }

    None
}

fn read_i2e_record(pager: &mut Pager, start: PageId, internal_id_u64: u64) -> Result<I2eRecord> {
    let (page_id, offset) = i2e_location(start, internal_id_u64)?;
    let page = pager.read_page(page_id)?;
    let bytes: [u8; I2E_RECORD_SIZE] = page[offset..offset + I2E_RECORD_SIZE].try_into().unwrap();
    Ok(I2eRecord::decode(&bytes))
}

fn write_i2e_record(
    pager: &mut Pager,
    start: PageId,
    internal_id_u64: u64,
    record: I2eRecord,
) -> Result<()> {
    let (page_id, offset) = i2e_location(start, internal_id_u64)?;
    pager.ensure_allocated(page_id)?;
    let mut page = pager.read_page(page_id)?;
    let encoded = record.encode();
    page[offset..offset + I2E_RECORD_SIZE].copy_from_slice(&encoded);
    pager.write_page(page_id, &page)?;
    Ok(())
}

fn i2e_location(start: PageId, internal_id_u64: u64) -> Result<(PageId, usize)> {
    let index = usize::try_from(internal_id_u64).map_err(|_| Error::WalProtocol("id too large"))?;
    let page_offset = index / I2E_RECORDS_PER_PAGE;
    let in_page_index = index % I2E_RECORDS_PER_PAGE;
    let page_id = PageId::new(start.as_u64() + page_offset as u64);
    Ok((page_id, in_page_index * I2E_RECORD_SIZE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob_store::BlobStore;
    use tempfile::tempdir;

    #[test]
    fn idmap_persists_i2e_and_rebuilds_e2i() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("test.ndb");
        let mut pager = Pager::open(&ndb).unwrap();

        let mut idmap = IdMap::load(&mut pager).unwrap();
        assert_eq!(idmap.len(), 0);

        idmap.apply_create_node(&mut pager, 100, 7, 0).unwrap();
        idmap.apply_create_node(&mut pager, 200, 7, 1).unwrap();

        drop(pager);

        let mut pager = Pager::open(&ndb).unwrap();
        let idmap2 = IdMap::load(&mut pager).unwrap();
        assert_eq!(idmap2.lookup(100), Some(0));
        assert_eq!(idmap2.lookup(200), Some(1));
        assert_eq!(idmap2.len(), 2);
    }

    #[test]
    fn idmap_interleaved_blob_writes_survive_boundary_extension() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("i2e-boundary.ndb");
        let mut pager = Pager::open(&ndb).unwrap();
        let mut idmap = IdMap::load(&mut pager).unwrap();

        let mut blobs = Vec::new();
        for internal_id in 0..513u32 {
            let external_id = 10_000 + internal_id as u64;
            idmap
                .apply_create_node(&mut pager, external_id, 7, internal_id)
                .unwrap();

            let payload = vec![(internal_id % 251) as u8; 256];
            let blob_id = BlobStore::write_direct(&mut pager, &payload).unwrap();
            blobs.push((blob_id, payload));
        }

        for (blob_id, expected) in &blobs {
            let actual = BlobStore::read_direct(&pager, *blob_id).unwrap();
            assert_eq!(actual, *expected);
        }

        drop(pager);

        let mut pager = Pager::open(&ndb).unwrap();
        let idmap2 = IdMap::load(&mut pager).unwrap();
        assert_eq!(idmap2.lookup(10_000), Some(0));
        assert_eq!(idmap2.lookup(10_512), Some(512));
        assert_eq!(idmap2.len(), 513);
    }

    #[test]
    fn idmap_relocates_when_next_contiguous_page_is_taken() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("i2e-relocate.ndb");
        let mut pager = Pager::open(&ndb).unwrap();
        let mut idmap = IdMap::load(&mut pager).unwrap();

        idmap.apply_create_node(&mut pager, 100, 7, 0).unwrap();
        let old_start = idmap.i2e_start.unwrap();

        let payload = vec![0xAB; 256];
        let blob_id = BlobStore::write_direct(&mut pager, &payload).unwrap();
        assert_eq!(blob_id, old_start.as_u64() + 1);

        for internal_id in 1..=512u32 {
            let external_id = 100 + internal_id as u64;
            idmap
                .apply_create_node(&mut pager, external_id, 7, internal_id)
                .unwrap();
        }

        let new_start = idmap.i2e_start.unwrap();
        assert_ne!(new_start, old_start);
        assert!(!pager.is_page_allocated(old_start));
        assert!(pager.is_page_allocated(PageId::new(blob_id)));
        assert_eq!(BlobStore::read_direct(&pager, blob_id).unwrap(), payload);

        drop(pager);

        let mut pager = Pager::open(&ndb).unwrap();
        let idmap2 = IdMap::load(&mut pager).unwrap();
        assert_eq!(idmap2.lookup(100), Some(0));
        assert_eq!(idmap2.lookup(612), Some(512));
        assert_eq!(idmap2.len(), 513);
    }

    #[test]
    fn idmap_interleaved_blob_writes_survive_multiple_relocations() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("i2e-multi-relocate.ndb");
        let mut pager = Pager::open(&ndb).unwrap();
        let mut idmap = IdMap::load(&mut pager).unwrap();

        let mut blobs = Vec::new();
        for internal_id in 0..1025u32 {
            let external_id = 20_000 + internal_id as u64;
            idmap
                .apply_create_node(&mut pager, external_id, 9, internal_id)
                .unwrap();

            let payload = vec![(internal_id % 239) as u8; 256];
            let blob_id = BlobStore::write_direct(&mut pager, &payload).unwrap();
            blobs.push((blob_id, payload));
        }

        for (blob_id, expected) in &blobs {
            let actual = BlobStore::read_direct(&pager, *blob_id).unwrap();
            assert_eq!(actual, *expected);
        }

        drop(pager);

        let mut pager = Pager::open(&ndb).unwrap();
        let idmap2 = IdMap::load(&mut pager).unwrap();
        assert_eq!(idmap2.lookup(20_000), Some(0));
        assert_eq!(idmap2.lookup(21_024), Some(1024));
        assert_eq!(idmap2.len(), 1025);
    }
}
