use crate::PAGE_SIZE;
use crate::error::{Error, Result};
use crate::pager::{PageId, Pager};

const MAGIC: [u8; 4] = *b"NDBI";
const VERSION: u8 = 1;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageKind {
    Leaf = 0,
    Internal = 1,
}

const COMMON_HEADER_SIZE: usize = 24;
const INTERNAL_HEADER_SIZE: usize = 32;

// Common header offsets.
const OFF_MAGIC: usize = 0; // [u8;4]
const OFF_KIND: usize = 4; // u8
const OFF_VERSION: usize = 5; // u8
const OFF_CELL_COUNT: usize = 6; // u16
const OFF_CELL_CONTENT_BEGIN: usize = 8; // u16
const OFF_FREE_BYTES: usize = 10; // u16
const OFF_RESERVED: usize = 12; // u32
const OFF_RIGHT_SIBLING: usize = 16; // u64

// Internal-only header.
const OFF_LEFTMOST_CHILD: usize = 24; // u64

fn header_size(kind: PageKind) -> usize {
    match kind {
        PageKind::Leaf => COMMON_HEADER_SIZE,
        PageKind::Internal => INTERNAL_HEADER_SIZE,
    }
}

fn read_u16_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().unwrap())
}

fn write_u16_le(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn write_u32_le(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn read_u64_le(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}

fn write_u64_le(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn varint_u32_len(mut v: u32) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

fn write_varint_u32(mut v: u32, out: &mut [u8]) -> usize {
    let mut i = 0;
    while v >= 0x80 {
        out[i] = (v as u8) | 0x80;
        v >>= 7;
        i += 1;
    }
    out[i] = v as u8;
    i + 1
}

fn read_varint_u32(buf: &[u8]) -> Option<(u32, usize)> {
    let mut v: u32 = 0;
    let mut shift = 0;
    for (i, &b) in buf.iter().enumerate() {
        let chunk = u32::from(b & 0x7F);
        v |= chunk.checked_shl(shift)?;
        if (b & 0x80) == 0 {
            return Some((v, i + 1));
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
    None
}

struct Page<'a> {
    buf: &'a mut [u8; PAGE_SIZE],
}

impl<'a> Page<'a> {
    fn new(buf: &'a mut [u8; PAGE_SIZE]) -> Self {
        Self { buf }
    }

    fn kind(&self) -> Result<PageKind> {
        if self.buf[OFF_MAGIC..OFF_MAGIC + 4] != MAGIC {
            return Err(Error::WalProtocol("index page: bad magic"));
        }
        if self.buf[OFF_VERSION] != VERSION {
            return Err(Error::WalProtocol("index page: bad version"));
        }
        match self.buf[OFF_KIND] {
            0 => Ok(PageKind::Leaf),
            1 => Ok(PageKind::Internal),
            _ => Err(Error::WalProtocol("index page: bad kind")),
        }
    }

    fn init_leaf(&mut self) {
        self.buf.fill(0);
        self.buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC);
        self.buf[OFF_KIND] = PageKind::Leaf as u8;
        self.buf[OFF_VERSION] = VERSION;
        write_u16_le(self.buf, OFF_CELL_COUNT, 0);
        write_u16_le(self.buf, OFF_CELL_CONTENT_BEGIN, PAGE_SIZE as u16);
        write_u16_le(self.buf, OFF_FREE_BYTES, 0);
        write_u32_le(self.buf, OFF_RESERVED, 0);
        write_u64_le(self.buf, OFF_RIGHT_SIBLING, 0);
    }

    fn init_internal(&mut self, leftmost_child: PageId) {
        self.buf.fill(0);
        self.buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC);
        self.buf[OFF_KIND] = PageKind::Internal as u8;
        self.buf[OFF_VERSION] = VERSION;
        write_u16_le(self.buf, OFF_CELL_COUNT, 0);
        write_u16_le(self.buf, OFF_CELL_CONTENT_BEGIN, PAGE_SIZE as u16);
        write_u16_le(self.buf, OFF_FREE_BYTES, 0);
        write_u32_le(self.buf, OFF_RESERVED, 0);
        write_u64_le(self.buf, OFF_RIGHT_SIBLING, 0);
        write_u64_le(self.buf, OFF_LEFTMOST_CHILD, leftmost_child.as_u64());
    }

    fn cell_count(&self) -> usize {
        read_u16_le(self.buf, OFF_CELL_COUNT) as usize
    }

    fn cell_content_begin(&self) -> usize {
        read_u16_le(self.buf, OFF_CELL_CONTENT_BEGIN) as usize
    }

    fn set_cell_content_begin(&mut self, v: usize) {
        write_u16_le(self.buf, OFF_CELL_CONTENT_BEGIN, v as u16);
    }

    fn right_sibling(&self) -> PageId {
        PageId::new(read_u64_le(self.buf, OFF_RIGHT_SIBLING))
    }

    fn set_right_sibling(&mut self, id: PageId) {
        write_u64_le(self.buf, OFF_RIGHT_SIBLING, id.as_u64());
    }

    fn leftmost_child(&self) -> Result<PageId> {
        if self.kind()? != PageKind::Internal {
            return Err(Error::WalProtocol("index page: not internal"));
        }
        Ok(PageId::new(read_u64_le(self.buf, OFF_LEFTMOST_CHILD)))
    }

    fn slots_off(&self) -> Result<usize> {
        Ok(header_size(self.kind()?))
    }

    fn slot_get(&self, i: usize) -> Result<usize> {
        let slots = self.slots_off()?;
        let off = slots + i * 2;
        Ok(read_u16_le(self.buf, off) as usize)
    }

    fn slot_set(&mut self, i: usize, v: usize) -> Result<()> {
        let slots = self.slots_off()?;
        let off = slots + i * 2;
        write_u16_le(self.buf, off, v as u16);
        Ok(())
    }

    fn free_space(&self) -> Result<usize> {
        let slots = self.slots_off()?;
        let ptr_end = slots + self.cell_count() * 2;
        let begin = self.cell_content_begin();
        Ok(begin.saturating_sub(ptr_end))
    }

    fn shift_slots_right(&mut self, idx: usize) -> Result<()> {
        let count = self.cell_count();
        if idx > count {
            return Err(Error::WalProtocol("index page: slot idx out of bounds"));
        }
        if idx == count {
            return Ok(());
        }

        let slots = self.slots_off()?;
        let src = slots + idx * 2;
        let len = (count - idx) * 2;
        self.buf.copy_within(src..src + len, src + 2);
        Ok(())
    }

    fn set_cell_count(&mut self, count: usize) {
        write_u16_le(self.buf, OFF_CELL_COUNT, count as u16);
    }

    fn leaf_cell_key_and_payload(&self, idx: usize) -> Result<(&[u8], u64)> {
        if self.kind()? != PageKind::Leaf {
            return Err(Error::WalProtocol("index page: not leaf"));
        }
        if idx >= self.cell_count() {
            return Err(Error::WalProtocol("index page: cell idx out of bounds"));
        }
        let cell_off = self.slot_get(idx)?;
        let (key_len, var_len) =
            read_varint_u32(&self.buf[cell_off..]).ok_or(Error::WalProtocol("bad varint"))?;
        let key_len = key_len as usize;
        let key_start = cell_off + var_len;
        let key_end = key_start + key_len;
        if key_end + 8 > PAGE_SIZE {
            return Err(Error::WalProtocol("index page: cell out of range"));
        }
        let payload = read_u64_le(self.buf, key_end);
        Ok((&self.buf[key_start..key_end], payload))
    }

    fn internal_cell_key_and_right_child(&self, idx: usize) -> Result<(&[u8], PageId)> {
        if self.kind()? != PageKind::Internal {
            return Err(Error::WalProtocol("index page: not internal"));
        }
        if idx >= self.cell_count() {
            return Err(Error::WalProtocol("index page: cell idx out of bounds"));
        }
        let cell_off = self.slot_get(idx)?;
        if cell_off + 8 >= PAGE_SIZE {
            return Err(Error::WalProtocol("index page: cell out of range"));
        }
        let right_child = PageId::new(read_u64_le(self.buf, cell_off));
        let (key_len, var_len) =
            read_varint_u32(&self.buf[cell_off + 8..]).ok_or(Error::WalProtocol("bad varint"))?;
        let key_len = key_len as usize;
        let key_start = cell_off + 8 + var_len;
        let key_end = key_start + key_len;
        if key_end > PAGE_SIZE {
            return Err(Error::WalProtocol("index page: cell out of range"));
        }
        Ok((&self.buf[key_start..key_end], right_child))
    }

    fn leaf_lower_bound(&self, target: &[u8]) -> Result<usize> {
        let n = self.cell_count();
        let mut lo = 0usize;
        let mut hi = n;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let (k, _) = self.leaf_cell_key_and_payload(mid)?;
            if k < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Ok(lo)
    }

    fn internal_child_for_key(&self, target: &[u8]) -> Result<(PageId, usize)> {
        if self.kind()? != PageKind::Internal {
            return Err(Error::WalProtocol("index page: not internal"));
        }
        let n = self.cell_count();
        // upper_bound: first key > target
        let mut lo = 0usize;
        let mut hi = n;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let (k, _) = self.internal_cell_key_and_right_child(mid)?;
            if k <= target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        // child_pos: 0 means leftmost; otherwise cell[child_pos-1].right_child
        let child_pos = lo;
        if child_pos == 0 {
            Ok((self.leftmost_child()?, 0))
        } else {
            let (_, child) = self.internal_cell_key_and_right_child(child_pos - 1)?;
            Ok((child, child_pos))
        }
    }

    fn leaf_insert_at(&mut self, idx: usize, key: &[u8], payload: u64) -> Result<()> {
        if self.kind()? != PageKind::Leaf {
            return Err(Error::WalProtocol("index page: not leaf"));
        }

        let key_len = u32::try_from(key.len()).map_err(|_| Error::WalProtocol("key too large"))?;
        let var_len = varint_u32_len(key_len);
        let cell_len = var_len + key.len() + 8;

        let free = self.free_space()?;
        if free < cell_len + 2 {
            return Err(Error::WalProtocol("index page: no space"));
        }

        let count = self.cell_count();
        if idx > count {
            return Err(Error::WalProtocol("index page: insert idx out of bounds"));
        }

        let new_begin = self
            .cell_content_begin()
            .checked_sub(cell_len)
            .ok_or(Error::WalProtocol("index page: cell content underflow"))?;
        self.set_cell_content_begin(new_begin);

        // Write cell bytes.
        let cell_off = new_begin;
        let wrote = write_varint_u32(key_len, &mut self.buf[cell_off..cell_off + var_len]);
        debug_assert_eq!(wrote, var_len);
        let key_start = cell_off + var_len;
        self.buf[key_start..key_start + key.len()].copy_from_slice(key);
        write_u64_le(self.buf, key_start + key.len(), payload);

        // Insert slot.
        self.shift_slots_right(idx)?;
        self.slot_set(idx, cell_off)?;
        self.set_cell_count(count + 1);
        Ok(())
    }

    fn internal_insert_at(&mut self, idx: usize, key: &[u8], right_child: PageId) -> Result<()> {
        if self.kind()? != PageKind::Internal {
            return Err(Error::WalProtocol("index page: not internal"));
        }

        let key_len = u32::try_from(key.len()).map_err(|_| Error::WalProtocol("key too large"))?;
        let var_len = varint_u32_len(key_len);
        let cell_len = 8 + var_len + key.len();

        let free = self.free_space()?;
        if free < cell_len + 2 {
            return Err(Error::WalProtocol("index page: no space"));
        }

        let count = self.cell_count();
        if idx > count {
            return Err(Error::WalProtocol("index page: insert idx out of bounds"));
        }

        let new_begin = self
            .cell_content_begin()
            .checked_sub(cell_len)
            .ok_or(Error::WalProtocol("index page: cell content underflow"))?;
        self.set_cell_content_begin(new_begin);

        let cell_off = new_begin;
        write_u64_le(self.buf, cell_off, right_child.as_u64());
        let wrote = write_varint_u32(key_len, &mut self.buf[cell_off + 8..cell_off + 8 + var_len]);
        debug_assert_eq!(wrote, var_len);
        let key_start = cell_off + 8 + var_len;
        self.buf[key_start..key_start + key.len()].copy_from_slice(key);

        self.shift_slots_right(idx)?;
        self.slot_set(idx, cell_off)?;
        self.set_cell_count(count + 1);
        Ok(())
    }

    fn rebuild_leaf(&mut self, right_sibling: PageId, entries: &[(Vec<u8>, u64)]) {
        self.init_leaf();
        self.set_right_sibling(right_sibling);
        // Insert in order, no need for binary search.
        for (i, (k, v)) in entries.iter().enumerate() {
            // Guaranteed to fit because split/build ensures it.
            self.leaf_insert_at(i, k, *v).unwrap();
        }
    }

    fn rebuild_internal(
        &mut self,
        leftmost_child: PageId,
        cells: &[(Vec<u8>, PageId)],
    ) -> Result<()> {
        self.init_internal(leftmost_child);
        for (i, (k, child)) in cells.iter().enumerate() {
            self.internal_insert_at(i, k, *child)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct BTree {
    root: PageId,
}

#[derive(Clone, Copy, Debug)]
struct PathEntry {
    page: PageId,
    child_pos: usize, // 0..=N, 0 means leftmost_child
}

impl BTree {
    pub fn root(&self) -> PageId {
        self.root
    }

    pub fn load(root: PageId) -> Self {
        Self { root }
    }

    pub fn create(pager: &mut Pager) -> Result<Self> {
        let root = pager.allocate_page()?;
        let mut buf = [0u8; PAGE_SIZE];
        Page::new(&mut buf).init_leaf();
        pager.write_page(root, &buf)?;
        Ok(Self { root })
    }

    pub fn insert(&mut self, pager: &mut Pager, key: &[u8], payload: u64) -> Result<()> {
        let mut path: Vec<PathEntry> = Vec::new();
        let mut cur = self.root;

        loop {
            let mut buf = pager.read_page(cur)?;
            let kind = Page::new(&mut buf).kind()?;
            match kind {
                PageKind::Leaf => {
                    let mut page = Page::new(&mut buf);
                    let idx = page.leaf_lower_bound(key)?;
                    match page.leaf_insert_at(idx, key, payload) {
                        Ok(()) => {
                            pager.write_page(cur, &buf)?;
                            return Ok(());
                        }
                        Err(_) => {
                            // Split leaf.
                            let old_right = page.right_sibling();
                            let mut entries: Vec<(Vec<u8>, u64)> = (0..page.cell_count())
                                .map(|i| {
                                    let (k, v) = page.leaf_cell_key_and_payload(i).unwrap();
                                    (k.to_vec(), v)
                                })
                                .collect();
                            // Insert new entry into the sorted list.
                            let pos = entries
                                .binary_search_by(|(k, _)| k.as_slice().cmp(key))
                                .unwrap_or_else(|p| p);
                            entries.insert(pos, (key.to_vec(), payload));

                            let mid = entries.len() / 2;
                            let left_entries = entries[..mid].to_vec();
                            let right_entries = entries[mid..].to_vec();
                            let sep_key = right_entries[0].0.clone();

                            let right_id = pager.allocate_page()?;
                            let mut right_buf = [0u8; PAGE_SIZE];
                            Page::new(&mut right_buf).rebuild_leaf(old_right, &right_entries);
                            page.rebuild_leaf(right_id, &left_entries);

                            pager.write_page(cur, &buf)?;
                            pager.write_page(right_id, &right_buf)?;

                            self.insert_into_parent(pager, &mut path, cur, sep_key, right_id)?;
                            return Ok(());
                        }
                    }
                }
                PageKind::Internal => {
                    let page = Page::new(&mut buf);
                    let (child, child_pos) = page.internal_child_for_key(key)?;
                    path.push(PathEntry {
                        page: cur,
                        child_pos,
                    });
                    cur = child;
                }
            }
        }
    }

    /// Delete an exact `(key, payload)` tuple by rebuilding the whole tree.
    ///
    /// This is intentionally brute-force: it keeps the page layout and separator invariants
    /// correct without implementing complex in-place rebalancing yet.
    ///
    /// Downside: pages are not reclaimed (pager has no vacuum yet). This is acceptable for MVP.
    pub fn delete_exact_rebuild(
        &mut self,
        pager: &mut Pager,
        key: &[u8],
        payload: u64,
    ) -> Result<bool> {
        let mut entries = self.scan_all(pager)?;
        let pos = entries
            .binary_search_by(|(k, v)| (k.as_slice(), *v).cmp(&(key, payload)))
            .ok();
        let Some(i) = pos else {
            return Ok(false);
        };
        // Guard: remove only if full key matches and payload matches (already ensured by binary_search).
        entries.remove(i);
        self.root = Self::build_from_sorted_entries(pager, &entries)?;
        Ok(true)
    }

    fn scan_all(&self, pager: &mut Pager) -> Result<Vec<(Vec<u8>, u64)>> {
        let mut cur = self.cursor_lower_bound(pager, &[])?;
        let mut out = Vec::new();
        while cur.is_valid()? {
            out.push((cur.key()?, cur.payload()?));
            if !cur.advance()? {
                break;
            }
        }
        Ok(out)
    }

    fn build_from_sorted_entries(pager: &mut Pager, entries: &[(Vec<u8>, u64)]) -> Result<PageId> {
        // Build leaf pages.
        let mut leaf_pages: Vec<(PageId, Vec<u8>)> = Vec::new(); // (page_id, min_key)
        let mut cur_leaf_id = pager.allocate_page()?;
        let mut cur_leaf_buf = [0u8; PAGE_SIZE];
        let mut cur_leaf = Page::new(&mut cur_leaf_buf);
        cur_leaf.init_leaf();

        let mut leaf_entries: Vec<(Vec<u8>, u64)> = Vec::new();
        let mut leaf_min_key: Option<Vec<u8>> = None;

        for (k, v) in entries {
            if leaf_min_key.is_none() {
                leaf_min_key = Some(k.clone());
            }
            let idx = leaf_entries.len();
            if cur_leaf.leaf_insert_at(idx, k, *v).is_ok() {
                leaf_entries.push((k.clone(), *v));
                continue;
            }

            // Finalize current leaf and start a new one.
            let next_leaf_id = pager.allocate_page()?;
            cur_leaf.rebuild_leaf(next_leaf_id, &leaf_entries);
            pager.write_page(cur_leaf_id, &cur_leaf_buf)?;
            leaf_pages.push((cur_leaf_id, leaf_min_key.take().unwrap()));

            cur_leaf_id = next_leaf_id;
            cur_leaf_buf = [0u8; PAGE_SIZE];
            cur_leaf = Page::new(&mut cur_leaf_buf);
            cur_leaf.init_leaf();
            leaf_entries.clear();

            leaf_min_key = Some(k.clone());
            cur_leaf.leaf_insert_at(0, k, *v)?;
            leaf_entries.push((k.clone(), *v));
        }

        // Finalize the last leaf.
        cur_leaf.rebuild_leaf(PageId::new(0), &leaf_entries);
        pager.write_page(cur_leaf_id, &cur_leaf_buf)?;
        if let Some(min) = leaf_min_key {
            leaf_pages.push((cur_leaf_id, min));
        } else {
            // Empty tree: keep a single empty leaf.
            leaf_pages.push((cur_leaf_id, Vec::new()));
        }

        // Build internal levels until one root remains.
        let mut level: Vec<(PageId, Vec<u8>)> = leaf_pages;
        while level.len() > 1 {
            let mut next_level: Vec<(PageId, Vec<u8>)> = Vec::new();

            let mut i = 0usize;
            while i < level.len() {
                // Start a new internal page with first child.
                let (first_child_id, first_min_key) = &level[i];
                let internal_id = pager.allocate_page()?;
                let mut buf = [0u8; PAGE_SIZE];
                let mut page = Page::new(&mut buf);
                page.init_internal(*first_child_id);

                let page_min_key = first_min_key.clone();
                i += 1;

                // Fill keys for subsequent children as long as there is space.
                while i < level.len() {
                    let (child_id, child_min_key) = &level[i];
                    let idx = page.cell_count();
                    if page
                        .internal_insert_at(idx, child_min_key, *child_id)
                        .is_ok()
                    {
                        i += 1;
                        continue;
                    }
                    break;
                }

                pager.write_page(internal_id, &buf)?;
                next_level.push((internal_id, page_min_key));
            }

            level = next_level;
        }

        Ok(level[0].0)
    }

    fn insert_into_parent(
        &mut self,
        pager: &mut Pager,
        path: &mut Vec<PathEntry>,
        left_id: PageId,
        sep_key: Vec<u8>,
        right_id: PageId,
    ) -> Result<()> {
        let Some(parent) = path.pop() else {
            // New root.
            let new_root = pager.allocate_page()?;
            let mut buf = [0u8; PAGE_SIZE];
            let mut page = Page::new(&mut buf);
            page.init_internal(left_id);
            page.internal_insert_at(0, &sep_key, right_id)?;
            pager.write_page(new_root, &buf)?;
            self.root = new_root;
            return Ok(());
        };

        let parent_id = parent.page;
        let child_pos = parent.child_pos;

        let mut buf = pager.read_page(parent_id)?;
        let kind = Page::new(&mut buf).kind()?;
        if kind != PageKind::Internal {
            return Err(Error::WalProtocol("index: parent not internal"));
        }

        let mut page = Page::new(&mut buf);
        match page.internal_insert_at(child_pos, &sep_key, right_id) {
            Ok(()) => {
                pager.write_page(parent_id, &buf)?;
                Ok(())
            }
            Err(_) => {
                // Split internal.
                let leftmost = page.leftmost_child()?;
                let mut keys: Vec<Vec<u8>> = Vec::with_capacity(page.cell_count() + 1);
                let mut children: Vec<PageId> = Vec::with_capacity(page.cell_count() + 2);
                children.push(leftmost);
                for i in 0..page.cell_count() {
                    let (k, right) = page.internal_cell_key_and_right_child(i)?;
                    keys.push(k.to_vec());
                    children.push(right);
                }

                // Insert new separator and new child into keys/children.
                // child_pos is in 0..=keys.len(), where 0 is before key0.
                keys.insert(child_pos, sep_key);
                children.insert(child_pos + 1, right_id);

                let mid = keys.len() / 2;
                let promote = keys[mid].clone();

                let left_keys = keys[..mid].to_vec();
                let right_keys = keys[mid + 1..].to_vec();

                let left_children = children[..mid + 1].to_vec();
                let right_children = children[mid + 1..].to_vec();

                // Rebuild left (in place) and build right.
                let right_page_id = pager.allocate_page()?;
                let mut right_buf = [0u8; PAGE_SIZE];

                let left_cells: Vec<(Vec<u8>, PageId)> = left_keys
                    .into_iter()
                    .zip(left_children.iter().skip(1).copied())
                    .collect();
                page.rebuild_internal(left_children[0], &left_cells)?;

                let right_cells: Vec<(Vec<u8>, PageId)> = right_keys
                    .into_iter()
                    .zip(right_children.iter().skip(1).copied())
                    .collect();
                Page::new(&mut right_buf).rebuild_internal(right_children[0], &right_cells)?;

                pager.write_page(parent_id, &buf)?;
                pager.write_page(right_page_id, &right_buf)?;

                // Propagate.
                self.insert_into_parent(pager, path, parent_id, promote, right_page_id)
            }
        }
    }

    pub fn cursor_lower_bound<'a>(
        &self,
        pager: &'a mut Pager,
        key: &[u8],
    ) -> Result<BTreeCursor<'a>> {
        let mut cur = self.root;
        loop {
            let mut buf = pager.read_page(cur)?;
            let kind = Page::new(&mut buf).kind()?;
            match kind {
                PageKind::Leaf => {
                    let page = Page::new(&mut buf);
                    let mut slot = page.leaf_lower_bound(key)? as u16;
                    let mut leaf_id = cur;
                    let mut leaf_buf = buf;
                    loop {
                        let page = Page::new(&mut leaf_buf);
                        let count = page.cell_count() as u16;
                        if count == 0 {
                            let next = page.right_sibling();
                            if next.as_u64() == 0 {
                                break;
                            }
                            leaf_id = next;
                            leaf_buf = pager.read_page(leaf_id)?;
                            slot = 0;
                            continue;
                        }

                        if slot < count {
                            break;
                        }

                        let next = page.right_sibling();
                        if next.as_u64() == 0 {
                            break;
                        }
                        leaf_id = next;
                        leaf_buf = pager.read_page(leaf_id)?;
                        slot = 0;
                    }

                    return Ok(BTreeCursor {
                        pager,
                        leaf: leaf_id,
                        buf: leaf_buf,
                        slot,
                    });
                }
                PageKind::Internal => {
                    let page = Page::new(&mut buf);
                    let (child, _) = page.internal_child_for_key(key)?;
                    cur = child;
                }
            }
        }
    }
}

pub struct BTreeCursor<'a> {
    pager: &'a mut Pager,
    leaf: PageId,
    buf: [u8; PAGE_SIZE],
    slot: u16,
}

impl<'a> BTreeCursor<'a> {
    pub fn is_valid(&mut self) -> Result<bool> {
        let page = Page::new(&mut self.buf);
        Ok(self.slot < page.cell_count() as u16)
    }

    pub fn key(&mut self) -> Result<Vec<u8>> {
        let page = Page::new(&mut self.buf);
        let (k, _) = page.leaf_cell_key_and_payload(self.slot as usize)?;
        Ok(k.to_vec())
    }

    pub fn payload(&mut self) -> Result<u64> {
        let page = Page::new(&mut self.buf);
        let (_, v) = page.leaf_cell_key_and_payload(self.slot as usize)?;
        Ok(v)
    }

    pub fn advance(&mut self) -> Result<bool> {
        let page = Page::new(&mut self.buf);
        let count = page.cell_count() as u16;
        if self.slot + 1 < count {
            self.slot += 1;
            return Ok(true);
        }

        let next = page.right_sibling();
        if next.as_u64() == 0 {
            self.slot = count;
            return Ok(false);
        }

        self.leaf = next;
        self.buf = self.pager.read_page(self.leaf)?;
        self.slot = 0;
        self.is_valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::ordered_key::encode_index_key;
    use crate::property::PropertyValue;
    use tempfile::tempdir;

    #[test]
    fn cursor_iterates_in_sorted_order_single_leaf() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("btree.ndb");
        let mut pager = Pager::open(&path).unwrap();
        let mut tree = BTree::create(&mut pager).unwrap();

        let mut keys = Vec::new();
        for (i, v) in [10i64, -1, 7, 100, 0].iter().enumerate() {
            let k = encode_index_key(1, &PropertyValue::Int(*v), i as u64);
            keys.push((k.clone(), i as u64));
            tree.insert(&mut pager, &k, i as u64).unwrap();
        }

        keys.sort_by(|a, b| a.0.cmp(&b.0));

        let mut cur = tree.cursor_lower_bound(&mut pager, &[]).unwrap();
        let mut got = Vec::new();
        while cur.is_valid().unwrap() {
            got.push((cur.key().unwrap(), cur.payload().unwrap()));
            if !cur.advance().unwrap() {
                break;
            }
        }
        assert_eq!(got, keys);
    }

    #[test]
    fn seek_lower_bound_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("btree2.ndb");
        let mut pager = Pager::open(&path).unwrap();
        let mut tree = BTree::create(&mut pager).unwrap();

        let k1 = encode_index_key(1, &PropertyValue::Int(1), 1);
        let k2 = encode_index_key(1, &PropertyValue::Int(2), 2);
        let k3 = encode_index_key(1, &PropertyValue::Int(3), 3);
        tree.insert(&mut pager, &k1, 1).unwrap();
        tree.insert(&mut pager, &k3, 3).unwrap();
        tree.insert(&mut pager, &k2, 2).unwrap();

        let needle = encode_index_key(1, &PropertyValue::Int(2), 0);
        let mut cur = tree.cursor_lower_bound(&mut pager, &needle).unwrap();
        assert!(cur.is_valid().unwrap());
        assert_eq!(cur.key().unwrap(), k2);
    }

    #[test]
    fn insert_triggers_leaf_and_internal_splits() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("btree-split.ndb");
        let mut pager = Pager::open(&path).unwrap();
        let mut tree = BTree::create(&mut pager).unwrap();

        // Use large keys to force small fanout, so tests hit split paths quickly.
        // Each key is ~2000 bytes, so a page only fits a handful of cells.
        let mut keys = Vec::new();
        for i in 0..40u64 {
            let mut k = vec![b'k'; 2000];
            k[0..8].copy_from_slice(&i.to_be_bytes());
            keys.push((k.clone(), i));
            tree.insert(&mut pager, &k, i).unwrap();
        }

        keys.sort_by(|a, b| a.0.cmp(&b.0));

        let mut cur = tree.cursor_lower_bound(&mut pager, &[]).unwrap();
        let mut got = Vec::new();
        while cur.is_valid().unwrap() {
            got.push((cur.key().unwrap(), cur.payload().unwrap()));
            if !cur.advance().unwrap() {
                break;
            }
        }
        assert_eq!(got, keys);
    }
}
