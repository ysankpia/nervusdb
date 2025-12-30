use crate::{Error, FILE_MAGIC, PAGE_SIZE, Result, VERSION_MAJOR, VERSION_MINOR};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageId(u64);

impl PageId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

const META_PAGE_ID: PageId = PageId(0);
const BITMAP_PAGE_ID: PageId = PageId(1);
const FIRST_DATA_PAGE_ID: PageId = PageId(2);

const BITMAP_BITS: u64 = (PAGE_SIZE as u64) * 8;

#[derive(Debug, Clone, Copy)]
struct Meta {
    version_major: u32,
    version_minor: u32,
    page_size: u64,
    bitmap_page_id: u64,
    next_page_id: u64,
    i2e_start_page_id: u64,
    i2e_len: u64,
    next_internal_id: u64,
    index_catalog_root: u64,
    next_index_id: u32,
}

impl Meta {
    fn new() -> Self {
        Self {
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            page_size: PAGE_SIZE as u64,
            bitmap_page_id: BITMAP_PAGE_ID.as_u64(),
            next_page_id: FIRST_DATA_PAGE_ID.as_u64(),
            i2e_start_page_id: 0,
            i2e_len: 0,
            next_internal_id: 0,
            index_catalog_root: 0,
            next_index_id: 0,
        }
    }

    fn encode_page(self) -> [u8; PAGE_SIZE] {
        let mut page = [0u8; PAGE_SIZE];
        page[0..16].copy_from_slice(&FILE_MAGIC);
        page[16..20].copy_from_slice(&self.version_major.to_le_bytes());
        page[20..24].copy_from_slice(&self.version_minor.to_le_bytes());
        page[24..32].copy_from_slice(&self.page_size.to_le_bytes());
        page[32..40].copy_from_slice(&self.bitmap_page_id.to_le_bytes());
        page[40..48].copy_from_slice(&self.next_page_id.to_le_bytes());
        page[48..56].copy_from_slice(&self.i2e_start_page_id.to_le_bytes());
        page[56..64].copy_from_slice(&self.i2e_len.to_le_bytes());
        page[64..72].copy_from_slice(&self.next_internal_id.to_le_bytes());
        page[72..80].copy_from_slice(&self.index_catalog_root.to_le_bytes());
        page[80..84].copy_from_slice(&self.next_index_id.to_le_bytes());
        page
    }

    fn decode_page(page: &[u8; PAGE_SIZE]) -> Result<Self> {
        if page[0..16] != FILE_MAGIC {
            return Err(Error::InvalidMagic);
        }

        let version_major = u32::from_le_bytes(page[16..20].try_into().unwrap());
        let version_minor = u32::from_le_bytes(page[20..24].try_into().unwrap());
        let page_size = u64::from_le_bytes(page[24..32].try_into().unwrap());
        let bitmap_page_id = u64::from_le_bytes(page[32..40].try_into().unwrap());
        let next_page_id = u64::from_le_bytes(page[40..48].try_into().unwrap());
        let i2e_start_page_id = u64::from_le_bytes(page[48..56].try_into().unwrap());
        let i2e_len = u64::from_le_bytes(page[56..64].try_into().unwrap());
        let next_internal_id = u64::from_le_bytes(page[64..72].try_into().unwrap());
        let index_catalog_root = u64::from_le_bytes(page[72..80].try_into().unwrap());
        let next_index_id = u32::from_le_bytes(page[80..84].try_into().unwrap());

        if page_size != PAGE_SIZE as u64 {
            return Err(Error::UnsupportedPageSize(page_size));
        }

        let next_internal_id = if next_internal_id == 0 && i2e_len > 0 {
            i2e_len
        } else {
            next_internal_id
        };

        Ok(Self {
            version_major,
            version_minor,
            page_size,
            bitmap_page_id,
            next_page_id,
            i2e_start_page_id,
            i2e_len,
            next_internal_id,
            index_catalog_root,
            next_index_id,
        })
    }
}

#[derive(Debug, Clone)]
struct Bitmap {
    data: [u8; PAGE_SIZE],
}

impl Bitmap {
    fn new() -> Self {
        let mut bitmap = Self {
            data: [0u8; PAGE_SIZE],
        };
        bitmap.set_allocated(META_PAGE_ID, true);
        bitmap.set_allocated(BITMAP_PAGE_ID, true);
        bitmap
    }

    fn is_allocated(&self, page_id: PageId) -> bool {
        self.get_bit(page_id.as_u64())
    }

    fn set_allocated(&mut self, page_id: PageId, allocated: bool) {
        self.set_bit(page_id.as_u64(), allocated);
    }

    fn get_bit(&self, bit: u64) -> bool {
        let byte_index = (bit / 8) as usize;
        let mask = 1u8 << (bit % 8);
        (self.data[byte_index] & mask) != 0
    }

    fn set_bit(&mut self, bit: u64, value: bool) {
        let byte_index = (bit / 8) as usize;
        let mask = 1u8 << (bit % 8);
        if value {
            self.data[byte_index] |= mask;
        } else {
            self.data[byte_index] &= !mask;
        }
    }

    fn find_free_in_range(&self, start: u64, end: u64) -> Option<u64> {
        if start >= end {
            return None;
        }

        (start..end).find(|&id| !self.get_bit(id))
    }
}

#[derive(Debug)]
pub struct Pager {
    path: PathBuf,
    file: File,
    meta: Meta,
    bitmap: Bitmap,
}

impl Pager {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let existed = path.exists();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        if !existed || file.metadata()?.len() == 0 {
            let meta = Meta::new();
            let bitmap = Bitmap::new();
            file.set_len((PAGE_SIZE * 2) as u64)?;

            let mut pager = Self {
                path,
                file,
                meta,
                bitmap,
            };
            pager.flush_meta_and_bitmap()?;
            return Ok(pager);
        }

        if file.metadata()?.len() < (PAGE_SIZE * 2) as u64 {
            return Err(Error::WalProtocol("ndb file too small"));
        }

        let mut meta_page = [0u8; PAGE_SIZE];
        read_page_raw(&mut file, META_PAGE_ID, &mut meta_page)?;
        let meta = Meta::decode_page(&meta_page)?;

        let mut bitmap_page = [0u8; PAGE_SIZE];
        read_page_raw(&mut file, BITMAP_PAGE_ID, &mut bitmap_page)?;
        let bitmap = Bitmap { data: bitmap_page };

        Ok(Self {
            path,
            file,
            meta,
            bitmap,
        })
    }

    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[inline]
    pub fn i2e_start_page(&self) -> Option<PageId> {
        if self.meta.i2e_start_page_id == 0 {
            None
        } else {
            Some(PageId::new(self.meta.i2e_start_page_id))
        }
    }

    #[inline]
    pub fn i2e_len(&self) -> u64 {
        self.meta.i2e_len
    }

    #[inline]
    pub fn next_internal_id(&self) -> u32 {
        u32::try_from(self.meta.next_internal_id).unwrap_or(u32::MAX)
    }

    #[inline]
    pub fn index_catalog_root(&self) -> Option<PageId> {
        if self.meta.index_catalog_root == 0 {
            None
        } else {
            Some(PageId::new(self.meta.index_catalog_root))
        }
    }

    #[inline]
    pub fn next_index_id(&self) -> u32 {
        if self.meta.next_index_id == 0 {
            1
        } else {
            self.meta.next_index_id
        }
    }

    pub fn set_i2e_start_page(&mut self, start: Option<PageId>) -> Result<()> {
        self.meta.i2e_start_page_id = start.map(|p| p.as_u64()).unwrap_or(0);
        self.flush_meta_and_bitmap()
    }

    pub fn set_i2e_len(&mut self, len: u64) -> Result<()> {
        self.meta.i2e_len = len;
        self.flush_meta_and_bitmap()
    }

    pub fn set_next_internal_id(&mut self, next: u32) -> Result<()> {
        self.meta.next_internal_id = next as u64;
        self.flush_meta_and_bitmap()
    }

    pub fn set_index_catalog_root(&mut self, root: Option<PageId>) -> Result<()> {
        self.meta.index_catalog_root = root.map(|p| p.as_u64()).unwrap_or(0);
        self.flush_meta_and_bitmap()
    }

    pub fn allocate_index_id(&mut self) -> Result<u32> {
        let id = self.next_index_id();
        self.meta.next_index_id = id.saturating_add(1);
        self.flush_meta_and_bitmap()?;
        Ok(id)
    }

    pub fn allocate_page(&mut self) -> Result<PageId> {
        let max_pages = BITMAP_BITS;
        let candidate = self
            .bitmap
            .find_free_in_range(FIRST_DATA_PAGE_ID.as_u64(), self.meta.next_page_id)
            .unwrap_or(self.meta.next_page_id);

        if candidate >= max_pages {
            return Err(Error::PageIdOutOfRange(candidate));
        }

        if candidate == self.meta.next_page_id {
            self.meta.next_page_id = candidate + 1;
        }

        self.ensure_allocated(PageId::new(candidate))?;
        Ok(PageId::new(candidate))
    }

    pub fn free_page(&mut self, page_id: PageId) -> Result<()> {
        self.validate_data_page_id(page_id)?;
        if !self.bitmap.is_allocated(page_id) {
            return Err(Error::PageNotAllocated(page_id.as_u64()));
        }

        self.bitmap.set_allocated(page_id, false);
        self.flush_meta_and_bitmap()?;
        Ok(())
    }

    pub fn read_page(&mut self, page_id: PageId) -> Result<[u8; PAGE_SIZE]> {
        self.validate_data_page_id(page_id)?;
        if !self.bitmap.is_allocated(page_id) {
            return Err(Error::PageNotAllocated(page_id.as_u64()));
        }

        let mut page = [0u8; PAGE_SIZE];
        read_page_raw(&mut self.file, page_id, &mut page)?;
        Ok(page)
    }

    pub fn write_page(&mut self, page_id: PageId, page: &[u8; PAGE_SIZE]) -> Result<()> {
        self.validate_data_page_id(page_id)?;
        if !self.bitmap.is_allocated(page_id) {
            return Err(Error::PageNotAllocated(page_id.as_u64()));
        }

        write_page_raw(&mut self.file, page_id, page)?;
        Ok(())
    }

    pub fn sync(&mut self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }

    pub(crate) fn ensure_allocated(&mut self, page_id: PageId) -> Result<()> {
        self.validate_data_page_id(page_id)?;

        if page_id.as_u64() >= self.meta.next_page_id {
            self.meta.next_page_id = page_id.as_u64() + 1;
        }

        if !self.bitmap.is_allocated(page_id) {
            self.bitmap.set_allocated(page_id, true);
        }

        let required_bytes = (page_id.as_u64() + 1) * PAGE_SIZE as u64;
        let current_len = self.file.metadata()?.len();
        if current_len < required_bytes {
            self.file.set_len(required_bytes)?;
        }

        self.flush_meta_and_bitmap()
    }

    fn validate_data_page_id(&self, page_id: PageId) -> Result<()> {
        if page_id.as_u64() < FIRST_DATA_PAGE_ID.as_u64() || page_id.as_u64() >= BITMAP_BITS {
            return Err(Error::PageIdOutOfRange(page_id.as_u64()));
        }
        Ok(())
    }

    fn flush_meta_and_bitmap(&mut self) -> Result<()> {
        let meta_page = self.meta.encode_page();
        write_page_raw(&mut self.file, META_PAGE_ID, &meta_page)?;
        write_page_raw(&mut self.file, BITMAP_PAGE_ID, &self.bitmap.data)?;
        self.file.flush()?;
        Ok(())
    }
}

fn read_page_raw(file: &mut File, page_id: PageId, buf: &mut [u8; PAGE_SIZE]) -> Result<()> {
    let offset = page_id.as_u64() * PAGE_SIZE as u64;
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(buf)?;
    Ok(())
}

fn write_page_raw(file: &mut File, page_id: PageId, buf: &[u8; PAGE_SIZE]) -> Result<()> {
    let offset = page_id.as_u64() * PAGE_SIZE as u64;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(buf)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn allocate_write_read_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ndb");
        let mut pager = Pager::open(&path).unwrap();

        let pid = pager.allocate_page().unwrap();

        let mut data = [0u8; PAGE_SIZE];
        data[0] = 0xAB;
        data[PAGE_SIZE - 1] = 0xCD;
        pager.write_page(pid, &data).unwrap();

        let got = pager.read_page(pid).unwrap();
        assert_eq!(got[0], 0xAB);
        assert_eq!(got[PAGE_SIZE - 1], 0xCD);
    }
}
