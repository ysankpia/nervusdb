use crate::{
    Error, FILE_MAGIC, PAGE_SIZE, Result, STORAGE_FORMAT_EPOCH, VERSION_MAJOR, VERSION_MINOR,
};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::FileExt as _;
#[cfg(windows)]
use std::os::windows::fs::FileExt as _;

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
    storage_format_epoch: u64,
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
            storage_format_epoch: STORAGE_FORMAT_EPOCH,
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
        page[84..92].copy_from_slice(&self.storage_format_epoch.to_le_bytes());
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
        let storage_format_epoch = u64::from_le_bytes(page[84..92].try_into().unwrap());

        if page_size != PAGE_SIZE as u64 {
            return Err(Error::UnsupportedPageSize(page_size));
        }

        if storage_format_epoch != STORAGE_FORMAT_EPOCH {
            return Err(Error::StorageFormatMismatch {
                expected: STORAGE_FORMAT_EPOCH,
                found: storage_format_epoch,
            });
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
            storage_format_epoch,
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct VacuumCopyStats {
    pub old_next_page_id: u64,
    pub new_next_page_id: u64,
    pub copied_data_pages: u64,
    pub old_file_pages: u64,
    pub new_file_pages: u64,
}

impl Pager {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let existed = path.exists();
        let file = OpenOptions::new()
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
        read_page_raw(&file, META_PAGE_ID, &mut meta_page)?;
        let meta = Meta::decode_page(&meta_page)?;

        let mut bitmap_page = [0u8; PAGE_SIZE];
        read_page_raw(&file, BITMAP_PAGE_ID, &mut bitmap_page)?;
        let bitmap = Bitmap { data: bitmap_page };

        Ok(Self {
            path,
            file,
            meta,
            bitmap,
        })
    }

    pub(crate) fn write_vacuum_copy(
        &self,
        target_path: &Path,
        reachable: &BTreeSet<PageId>,
    ) -> Result<VacuumCopyStats> {
        let old_file_pages = self.file.metadata()?.len() / PAGE_SIZE as u64;
        let old_next_page_id = self.meta.next_page_id;

        let mut max_page_id = BITMAP_PAGE_ID.as_u64();
        let mut copied_data_pages = 0u64;
        for p in reachable {
            let id = p.as_u64();
            if id >= BITMAP_BITS {
                return Err(Error::PageIdOutOfRange(id));
            }
            max_page_id = max_page_id.max(id);
            if id >= FIRST_DATA_PAGE_ID.as_u64() {
                copied_data_pages = copied_data_pages.saturating_add(1);
            }
        }

        let new_next_page_id = max_page_id
            .saturating_add(1)
            .max(FIRST_DATA_PAGE_ID.as_u64());

        let mut meta = self.meta;
        meta.next_page_id = new_next_page_id;

        let mut bitmap = Bitmap::new();
        for p in reachable {
            if p.as_u64() >= FIRST_DATA_PAGE_ID.as_u64() {
                bitmap.set_allocated(*p, true);
            }
        }

        let out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .truncate(false)
            .open(target_path)?;

        out.set_len(new_next_page_id.saturating_mul(PAGE_SIZE as u64))?;

        let meta_page = meta.encode_page();
        write_page_raw(&out, META_PAGE_ID, &meta_page)?;
        write_page_raw(&out, BITMAP_PAGE_ID, &bitmap.data)?;

        for p in reachable {
            if p.as_u64() < FIRST_DATA_PAGE_ID.as_u64() {
                continue;
            }
            let page = self.read_page(*p)?;
            write_page_raw(&out, *p, &page)?;
        }

        out.sync_data()?;

        Ok(VacuumCopyStats {
            old_next_page_id,
            new_next_page_id,
            copied_data_pages,
            old_file_pages,
            new_file_pages: new_next_page_id,
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

    pub fn read_page(&self, page_id: PageId) -> Result<[u8; PAGE_SIZE]> {
        self.validate_data_page_id(page_id)?;
        if !self.bitmap.is_allocated(page_id) {
            return Err(Error::PageNotAllocated(page_id.as_u64()));
        }

        let mut page = [0u8; PAGE_SIZE];
        read_page_raw(&self.file, page_id, &mut page)?;
        Ok(page)
    }

    pub fn write_page(&mut self, page_id: PageId, page: &[u8; PAGE_SIZE]) -> Result<()> {
        self.validate_data_page_id(page_id)?;
        if !self.bitmap.is_allocated(page_id) {
            return Err(Error::PageNotAllocated(page_id.as_u64()));
        }

        write_page_raw(&self.file, page_id, page)?;
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
        write_page_raw(&self.file, META_PAGE_ID, &meta_page)?;
        write_page_raw(&self.file, BITMAP_PAGE_ID, &self.bitmap.data)?;
        // Ensure meta + bitmap durability. WAL replay can recover data pages, but
        // durable metadata reduces recovery work and avoids pathological re-scan.
        self.file.sync_data()?;
        Ok(())
    }
}

fn read_page_raw(file: &File, page_id: PageId, buf: &mut [u8; PAGE_SIZE]) -> Result<()> {
    let offset = page_id.as_u64() * PAGE_SIZE as u64;
    read_exact_at(file, offset, buf).map_err(Error::Io)?;
    Ok(())
}

fn write_page_raw(file: &File, page_id: PageId, buf: &[u8; PAGE_SIZE]) -> Result<()> {
    let offset = page_id.as_u64() * PAGE_SIZE as u64;
    write_all_at(file, offset, buf).map_err(Error::Io)?;
    Ok(())
}

fn read_exact_at(file: &File, mut offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let n = read_at(file, offset, buf)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "read_at returned 0 bytes",
            ));
        }
        offset = offset.saturating_add(n as u64);
        buf = &mut buf[n..];
    }
    Ok(())
}

fn write_all_at(file: &File, mut offset: u64, mut buf: &[u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let n = write_at(file, offset, buf)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "write_at returned 0 bytes",
            ));
        }
        offset = offset.saturating_add(n as u64);
        buf = &buf[n..];
    }
    Ok(())
}

#[cfg(unix)]
fn read_at(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    file.read_at(buf, offset)
}

#[cfg(windows)]
fn read_at(file: &File, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    file.seek_read(buf, offset)
}

#[cfg(unix)]
fn write_at(file: &File, offset: u64, buf: &[u8]) -> io::Result<usize> {
    file.write_at(buf, offset)
}

#[cfg(windows)]
fn write_at(file: &File, offset: u64, buf: &[u8]) -> io::Result<usize> {
    file.seek_write(buf, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

    #[test]
    fn reject_storage_format_epoch_mismatch() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("epoch_mismatch.ndb");

        {
            let _pager = Pager::open(&path).unwrap();
        }

        let mut bytes = fs::read(&path).unwrap();
        bytes[84..92].copy_from_slice(&0u64.to_le_bytes());
        fs::write(&path, bytes).unwrap();

        let err = Pager::open(&path).unwrap_err();
        assert!(
            err.to_string()
                .to_lowercase()
                .contains("storage format mismatch"),
            "unexpected error: {err}"
        );
    }
}
