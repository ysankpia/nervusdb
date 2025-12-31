use crate::idmap::InternalNodeId;
use crate::pager::{PageId, Pager};
use crate::snapshot::{EdgeKey, RelTypeId};
use crate::{Error, PAGE_SIZE, Result};
use std::io::Cursor;
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeRecord {
    pub rel: RelTypeId,
    pub dst: InternalNodeId,
}

#[derive(Debug, Clone)]
pub struct CsrSegment {
    pub id: SegmentId,
    pub meta_page_id: u64,
    pub min_src: InternalNodeId,
    pub max_src: InternalNodeId,
    pub min_dst: InternalNodeId,
    pub max_dst: InternalNodeId,
    pub offsets: Vec<u64>,
    pub edges: Vec<EdgeRecord>,
    pub in_offsets: Vec<u64>,
    pub in_edges: Vec<EdgeRecord>,
}

impl CsrSegment {
    pub fn neighbors(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_> {
        if src < self.min_src || src > self.max_src {
            return Box::new(std::iter::empty());
        }

        let idx = (src - self.min_src) as usize;
        let start = self.offsets[idx] as usize;
        let end = self.offsets[idx + 1] as usize;

        Box::new(
            self.edges[start..end]
                .iter()
                .filter(move |e| rel.is_none_or(|r| e.rel == r))
                .map(move |e| EdgeKey {
                    src,
                    rel: e.rel,
                    dst: e.dst,
                }),
        )
    }

    pub fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_> {
        if dst < self.min_dst || dst > self.max_dst {
            return Box::new(std::iter::empty());
        }

        let idx = (dst - self.min_dst) as usize;
        let start = self.in_offsets[idx] as usize;
        let end = self.in_offsets[idx + 1] as usize;

        Box::new(
            self.in_edges[start..end]
                .iter()
                .filter(move |e| rel.is_none_or(|r| e.rel == r))
                .map(move |e| EdgeKey {
                    src: e.dst,
                    rel: e.rel,
                    dst,
                }),
        )
    }

    pub fn load(pager: &mut Pager, meta_page_id: u64) -> Result<Self> {
        let page = pager.read_page(PageId::new(meta_page_id))?;
        let mut seg = decode_segment(&page, pager)?;
        seg.meta_page_id = meta_page_id;
        Ok(seg)
    }

    pub fn persist(&mut self, pager: &mut Pager) -> Result<()> {
        // Build reverse index if we have edges but no reverse index
        if !self.edges.is_empty() && self.in_edges.is_empty() {
            let mut edges_with_src: Vec<EdgeKey> = self
                .edges
                .iter()
                .enumerate()
                .map(|(idx, e)| {
                    // We need to find the src for each edge.
                    // self.offsets[i] tells us where edges for min_src + i start.
                    // This is slow but persist is only called once.
                    let mut src = self.min_src;
                    for i in 0..self.offsets.len() - 1 {
                        if idx >= self.offsets[i] as usize && idx < self.offsets[i + 1] as usize {
                            src = self.min_src + i as u32;
                            break;
                        }
                    }
                    EdgeKey {
                        src,
                        rel: e.rel,
                        dst: e.dst,
                    }
                })
                .collect();

            // Sort by dst
            edges_with_src.sort_by_key(|e| (e.dst, e.rel, e.src));

            if let Some(first) = edges_with_src.first() {
                self.min_dst = first.dst;
                self.max_dst = edges_with_src.last().unwrap().dst;

                let range = (self.max_dst - self.min_dst + 1) as usize;
                let mut in_offsets = vec![0u64; range + 1];
                let mut in_edges = Vec::with_capacity(edges_with_src.len());

                let mut current_dst = self.min_dst;
                let mut count = 0u64;
                for e in edges_with_src {
                    while current_dst < e.dst {
                        current_dst += 1;
                        in_offsets[(current_dst - self.min_dst) as usize] = count;
                    }
                    in_edges.push(EdgeRecord {
                        rel: e.rel,
                        dst: e.src,
                    }); // Stores SRC in DST field
                    count += 1;
                }
                while current_dst <= self.max_dst {
                    current_dst += 1;
                    in_offsets[(current_dst - self.min_dst).min(range as u32) as usize] = count;
                }
                self.in_offsets = in_offsets;
                self.in_edges = in_edges;
            }
        }

        // Write both forward and reverse index
        let offsets_bytes = encode_offsets(&self.offsets);
        let edges_bytes = encode_edges(&self.edges);
        let in_offsets_bytes = encode_offsets(&self.in_offsets);
        let in_edges_bytes = encode_edges(&self.in_edges);

        let offsets_pages = write_blob_pages(pager, &offsets_bytes)?;
        let edges_pages = write_blob_pages(pager, &edges_bytes)?;
        let in_offsets_pages = write_blob_pages(pager, &in_offsets_bytes)?;
        let in_edges_pages = write_blob_pages(pager, &in_edges_bytes)?;

        let meta_page_id = pager.allocate_page()?.as_u64();
        self.meta_page_id = meta_page_id;

        let mut meta_page = [0u8; PAGE_SIZE];
        encode_meta(
            &mut meta_page,
            self.id,
            self.min_src,
            self.max_src,
            self.min_dst,
            self.max_dst,
            self.offsets.len() as u64,
            self.edges.len() as u64,
            self.in_offsets.len() as u64,
            self.in_edges.len() as u64,
            &offsets_pages,
            &edges_pages,
            &in_offsets_pages,
            &in_edges_pages,
        )?;
        pager.write_page(PageId::new(meta_page_id), &meta_page)?;
        Ok(())
    }
}

const META_MAGIC: [u8; 8] = *b"NDBCSRv2";

#[allow(clippy::too_many_arguments)]
fn encode_meta(
    out: &mut [u8; PAGE_SIZE],
    id: SegmentId,
    min_src: InternalNodeId,
    max_src: InternalNodeId,
    min_dst: InternalNodeId,
    max_dst: InternalNodeId,
    offsets_len: u64,
    edges_len: u64,
    in_offsets_len: u64,
    in_edges_len: u64,
    offsets_pages: &[u64],
    edges_pages: &[u64],
    in_offsets_pages: &[u64],
    in_edges_pages: &[u64],
) -> Result<()> {
    let needed = 80usize
        + (offsets_pages.len() + edges_pages.len() + in_offsets_pages.len() + in_edges_pages.len())
            * 8;
    if needed > PAGE_SIZE {
        return Err(Error::WalProtocol("csr meta page overflow"));
    }

    let mut cur = Cursor::new(out.as_mut_slice());
    cur.write_all(&META_MAGIC).map_err(Error::Io)?;
    cur.write_all(&id.0.to_le_bytes()).map_err(Error::Io)?;
    cur.write_all(&min_src.to_le_bytes()).map_err(Error::Io)?;
    cur.write_all(&max_src.to_le_bytes()).map_err(Error::Io)?;
    cur.write_all(&min_dst.to_le_bytes()).map_err(Error::Io)?;
    cur.write_all(&max_dst.to_le_bytes()).map_err(Error::Io)?;
    cur.write_all(&offsets_len.to_le_bytes())
        .map_err(Error::Io)?;
    cur.write_all(&edges_len.to_le_bytes()).map_err(Error::Io)?;
    cur.write_all(&in_offsets_len.to_le_bytes())
        .map_err(Error::Io)?;
    cur.write_all(&in_edges_len.to_le_bytes())
        .map_err(Error::Io)?;

    let offsets_page_count: u32 = offsets_pages.len() as u32;
    let edges_page_count: u32 = edges_pages.len() as u32;
    let in_offsets_page_count: u32 = in_offsets_pages.len() as u32;
    let in_edges_page_count: u32 = in_edges_pages.len() as u32;

    cur.write_all(&offsets_page_count.to_le_bytes())
        .map_err(Error::Io)?;
    cur.write_all(&edges_page_count.to_le_bytes())
        .map_err(Error::Io)?;
    cur.write_all(&in_offsets_page_count.to_le_bytes())
        .map_err(Error::Io)?;
    cur.write_all(&in_edges_page_count.to_le_bytes())
        .map_err(Error::Io)?;

    for p in offsets_pages {
        cur.write_all(&p.to_le_bytes()).map_err(Error::Io)?;
    }
    for p in edges_pages {
        cur.write_all(&p.to_le_bytes()).map_err(Error::Io)?;
    }
    for p in in_offsets_pages {
        cur.write_all(&p.to_le_bytes()).map_err(Error::Io)?;
    }
    for p in in_edges_pages {
        cur.write_all(&p.to_le_bytes()).map_err(Error::Io)?;
    }

    Ok(())
}

fn decode_segment(meta_page: &[u8; PAGE_SIZE], pager: &mut Pager) -> Result<CsrSegment> {
    if meta_page[0..8] != META_MAGIC {
        return Err(Error::WalProtocol("invalid csr meta magic"));
    }

    let id = u64::from_le_bytes(meta_page[8..16].try_into().unwrap());
    let min_src = u32::from_le_bytes(meta_page[16..20].try_into().unwrap());
    let max_src = u32::from_le_bytes(meta_page[20..24].try_into().unwrap());
    let min_dst = u32::from_le_bytes(meta_page[24..28].try_into().unwrap());
    let max_dst = u32::from_le_bytes(meta_page[28..32].try_into().unwrap());
    let offsets_len = u64::from_le_bytes(meta_page[32..40].try_into().unwrap()) as usize;
    let edges_len = u64::from_le_bytes(meta_page[40..48].try_into().unwrap()) as usize;
    let in_offsets_len = u64::from_le_bytes(meta_page[48..56].try_into().unwrap()) as usize;
    let in_edges_len = u64::from_le_bytes(meta_page[56..64].try_into().unwrap()) as usize;
    let offsets_page_count = u32::from_le_bytes(meta_page[64..68].try_into().unwrap()) as usize;
    let edges_page_count = u32::from_le_bytes(meta_page[68..72].try_into().unwrap()) as usize;
    let in_offsets_page_count = u32::from_le_bytes(meta_page[72..76].try_into().unwrap()) as usize;
    let in_edges_page_count = u32::from_le_bytes(meta_page[76..80].try_into().unwrap()) as usize;

    let needed = 80usize
        + (offsets_page_count + edges_page_count + in_offsets_page_count + in_edges_page_count) * 8;
    if needed > PAGE_SIZE {
        return Err(Error::WalProtocol("csr meta page overflow"));
    }

    let mut offset = 80;
    let mut offsets_pages = Vec::with_capacity(offsets_page_count);
    for _ in 0..offsets_page_count {
        offsets_pages.push(u64::from_le_bytes(
            meta_page[offset..offset + 8].try_into().unwrap(),
        ));
        offset += 8;
    }
    let mut edges_pages = Vec::with_capacity(edges_page_count);
    for _ in 0..edges_page_count {
        edges_pages.push(u64::from_le_bytes(
            meta_page[offset..offset + 8].try_into().unwrap(),
        ));
        offset += 8;
    }
    let mut in_offsets_pages = Vec::with_capacity(in_offsets_page_count);
    for _ in 0..in_offsets_page_count {
        in_offsets_pages.push(u64::from_le_bytes(
            meta_page[offset..offset + 8].try_into().unwrap(),
        ));
        offset += 8;
    }
    let mut in_edges_pages = Vec::with_capacity(in_edges_page_count);
    for _ in 0..in_edges_page_count {
        in_edges_pages.push(u64::from_le_bytes(
            meta_page[offset..offset + 8].try_into().unwrap(),
        ));
        offset += 8;
    }

    let offsets_bytes = read_blob_pages(pager, &offsets_pages)?;
    let edges_bytes = read_blob_pages(pager, &edges_pages)?;
    let in_offsets_bytes = read_blob_pages(pager, &in_offsets_pages)?;
    let in_edges_bytes = read_blob_pages(pager, &in_edges_pages)?;

    let offsets = decode_offsets(&offsets_bytes, offsets_len)?;
    let edges = decode_edges(&edges_bytes, edges_len)?;
    let in_offsets = decode_offsets(&in_offsets_bytes, in_offsets_len)?;
    let in_edges = decode_edges(&in_edges_bytes, in_edges_len)?;

    Ok(CsrSegment {
        id: SegmentId(id),
        meta_page_id: 0,
        min_src,
        max_src,
        min_dst,
        max_dst,
        offsets,
        edges,
        in_offsets,
        in_edges,
    })
}

fn encode_offsets(offsets: &[u64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(offsets.len() * 8);
    for v in offsets {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn decode_offsets(bytes: &[u8], len: usize) -> Result<Vec<u64>> {
    if bytes.len() < len * 8 {
        return Err(Error::WalProtocol("offset blob too small"));
    }
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let start = i * 8;
        out.push(u64::from_le_bytes(
            bytes[start..start + 8].try_into().unwrap(),
        ));
    }
    Ok(out)
}

fn encode_edges(edges: &[EdgeRecord]) -> Vec<u8> {
    let mut out = Vec::with_capacity(edges.len() * 8);
    for e in edges {
        out.extend_from_slice(&e.rel.to_le_bytes());
        out.extend_from_slice(&e.dst.to_le_bytes());
    }
    out
}

fn decode_edges(bytes: &[u8], len: usize) -> Result<Vec<EdgeRecord>> {
    if bytes.len() < len * 8 {
        return Err(Error::WalProtocol("edge blob too small"));
    }
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let start = i * 8;
        let rel = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap());
        let dst = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
        out.push(EdgeRecord { rel, dst });
    }
    Ok(out)
}

fn write_blob_pages(pager: &mut Pager, blob: &[u8]) -> Result<Vec<u64>> {
    let mut pages = Vec::new();
    let mut pos = 0;
    while pos < blob.len() {
        let page_id = pager.allocate_page()?;
        let mut page = [0u8; PAGE_SIZE];
        let n = (blob.len() - pos).min(PAGE_SIZE);
        page[..n].copy_from_slice(&blob[pos..pos + n]);
        pager.write_page(page_id, &page)?;
        pages.push(page_id.as_u64());
        pos += n;
    }
    if pages.is_empty() {
        // zero-length blob still needs a place-holder page list; keep empty.
    }
    Ok(pages)
}

fn read_blob_pages(pager: &mut Pager, pages: &[u64]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for p in pages {
        let page = pager.read_page(PageId::new(*p))?;
        out.extend_from_slice(&page);
    }
    Ok(out)
}
