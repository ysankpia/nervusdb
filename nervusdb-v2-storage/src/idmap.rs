use crate::pager::{PageId, Pager};
use crate::{Error, PAGE_SIZE, Result};
use std::collections::HashMap;

pub type ExternalId = u64;
pub type InternalNodeId = u32;
pub type LabelId = u32;

const I2E_RECORD_SIZE: usize = 16;
const I2E_RECORDS_PER_PAGE: usize = PAGE_SIZE / I2E_RECORD_SIZE;

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
        write_i2e_record(
            pager,
            start,
            internal_id as u64,
            I2eRecord {
                external_id,
                label_id: first_label,
                flags: 0,
            },
        )?;

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
            .ok_or_else(|| Error::WalProtocol("node not found"))?;

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
            .ok_or_else(|| Error::WalProtocol("node not found"))?;

        labels.retain(|&l| l != label);
        Ok(())
    }
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
}
