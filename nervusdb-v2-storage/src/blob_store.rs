use crate::Result;
use crate::pager::{PageId, Pager};
use std::sync::MutexGuard;

/// BlobStore manages large variable-length data by chaining 4KB pages.
/// Each page has a header: [next_page_id: u64 (0 if last)][data_len: u16][data...]
pub struct BlobStore;

const HEADER_SIZE: usize = 8 + 2;
const MAX_DATA_PER_PAGE: usize = crate::PAGE_SIZE - HEADER_SIZE;

impl BlobStore {
    /// Writes a blob to the pager and returns the first page ID.
    /// Requires a MutexGuard (for use through GraphEngine).
    pub fn write(pager: &mut MutexGuard<'_, Pager>, data: &[u8]) -> Result<u64> {
        Self::write_direct(pager, data)
    }

    /// Writes a blob to the pager and returns the first page ID.
    /// Direct pager access (for bulk loading).
    pub fn write_direct(pager: &mut Pager, data: &[u8]) -> Result<u64> {
        // Write from last to first to build the chain
        if data.is_empty() {
            // Handle empty blob
            let pid = pager.allocate_page()?;
            let page = [0u8; 8192];
            // next_page = 0, data_len = 0
            pager.write_page(pid, &page)?;
            return Ok(pid.as_u64());
        }

        // Re-implementing simply:
        let chunks: Vec<&[u8]> = data.chunks(MAX_DATA_PER_PAGE).collect();
        let mut last_pid = 0u64;

        // Write from last to first to build the chain
        for chunk in chunks.into_iter().rev() {
            let pid = pager.allocate_page()?;
            let mut page = [0u8; 8192];
            page[0..8].copy_from_slice(&last_pid.to_le_bytes());
            let chunk_len = chunk.len() as u16;
            page[8..10].copy_from_slice(&chunk_len.to_le_bytes());
            page[10..10 + chunk.len()].copy_from_slice(chunk);
            pager.write_page(pid, &page)?;
            last_pid = pid.as_u64();
        }

        Ok(last_pid)
    }

    /// Reads a blob starting from the given page ID.
    pub fn read(pager: &mut MutexGuard<'_, Pager>, mut page_id: u64) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        while page_id != 0 {
            let page = pager.read_page(PageId::new(page_id))?;
            let next_page_id = u64::from_le_bytes(page[0..8].try_into().unwrap());
            let data_len = u16::from_le_bytes(page[8..10].try_into().unwrap()) as usize;
            if data_len > MAX_DATA_PER_PAGE {
                return Err(crate::Error::StorageCorrupted(
                    "invalid blob page data length",
                ));
            }
            out.extend_from_slice(&page[10..10 + data_len]);
            page_id = next_page_id;
        }
        Ok(out)
    }

    /// Frees all pages in a blob chain.
    pub fn delete(pager: &mut MutexGuard<'_, Pager>, mut page_id: u64) -> Result<()> {
        while page_id != 0 {
            let page = pager.read_page(PageId::new(page_id))?;
            let next_page_id = u64::from_le_bytes(page[0..8].try_into().unwrap());
            pager.free_page(PageId::new(page_id))?;
            page_id = next_page_id;
        }
        Ok(())
    }
}
