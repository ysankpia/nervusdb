# Design T101: B+Tree Page Layout & Cursor

> **Status**: Draft
> **Parent**: T100 (Architecture)
> **Implementation Target**: `nervusdb-storage/src/index/btree.rs`

## 0. Non-Goals (Read This First)

This design is about **layout** and **cursor semantics**, not “a full database index feature set”.

- No global index catalog design here (that’s T102+).
- No fancy page compression.
- No background threads.
- No “online” structural modification requirements (v2 is Single Writer; compaction is the natural place to build/merge).

If we can’t explain a rule in one paragraph, it’s too complex and should be cut.

## 1. Overview

We store secondary indexes as **pager pages** inside the `.ndb` file (8KB pages, shared with graph data).
The B+Tree pages are a **slotted-page** design to support variable-length keys and in-page reordering without rewriting the whole page every time.

The *only* job of this layer is:

- Seek by key (point lookup).
- Iterate keys in order (range scan).
- Support bulk-build (sorted keys) with predictable I/O.

Everything else is policy above it.

## 2. Page Invariants

1. All integers in the page are **Little Endian** (Rust-native on all supported targets).
2. A page is either **Leaf** or **Internal**.
3. The slot array is **dense** and sorted by key order. No “holes”, no tombstone markers.
4. Cell payload area grows **downwards** from the page tail; slot array grows **upwards** from the header.
5. Any fragmentation is handled by **compaction** (repack cells) when needed; do not invent micro-free-lists.

## 3. Page Layout (Slotted Page)

**Visual Layout (8KB)**:

```text
[ Header ]                       fixed
[ Slot 0 ][ Slot 1 ] ...         u16 offsets (dense)
[ Free Space ]                   variable
[ Cell Bytes ] ...               variable (grows down)
```

### 3.1 Common Header (24 bytes)

| Offset | Size | Field | Description |
| :--- | :--- | :--- | :--- |
| 0 | 4 | `magic` | `b"NDBI"` |
| 4 | 1 | `kind` | `0 = Leaf`, `1 = Internal` |
| 5 | 1 | `version` | `1` |
| 6 | 2 | `cell_count` | number of slots/cells |
| 8 | 2 | `cell_content_begin` | offset to start of cell bytes (grows down) |
| 10 | 2 | `free_bytes` | bytes of fragmented cell area (hint) |
| 12 | 4 | `reserved` | future use |
| 16 | 8 | `right_sibling` | leaf: next leaf page id; internal: 0 |

Notes:

- `right_sibling` is only meaningful for leaf pages (range scans).
- `free_bytes` is a hint; correctness must never depend on it.

### 3.2 Internal Header Extension (8 bytes)

Internal pages need `N+1` child pointers for `N` separator keys.
To avoid a special-case cell for “first child”, store the left-most child in the header.

| Offset | Size | Field | Description |
| :--- | :--- | :--- | :--- |
| 24 | 8 | `leftmost_child` | page id of child `< first key` |

So:

- Leaf header size = 24 bytes.
- Internal header size = 32 bytes.

That’s “good taste”: the special case becomes normal data.

### 3.3 Slot Array

- Each slot is a `u16` offset to the corresponding cell payload.
- Slot array starts immediately after the header.
- `slot[i]` points to a cell payload; keys are compared by reading the cell’s key bytes.

### 3.4 Free Space Check

Free space is:

```text
free = cell_content_begin - (header_size + cell_count * 2)
```

Insert requires:

- `free >= (2 /*new slot*/ + cell_payload_len)`
- Otherwise: try `compact()`; if still insufficient, split.

## 4. Cell Formats

### 4.1 VarInt

We need a compact length encoding for keys.

Use a simple unsigned LEB128 for lengths:

- 1–5 bytes for `u32`.
- It’s standard, it’s boring, and it works.

### 4.2 Leaf Cell

Leaf stores full keys and a payload (`u64`) which is the row id / node id.

We require uniqueness for index entries by defining the B+Tree ordering key as:

```text
Key = (IndexId, OrderedPropValue, InternalNodeId)
Payload = Empty (or duplicated id, but don’t unless you must)
```

This makes duplicates impossible without extra logic.

Layout:

```text
[ key_len: varint ]
[ key_bytes: [u8; key_len] ]
[ payload: u64 ]    // for now: InternalNodeId as u64, or future RowId
```

Even if payload duplicates a piece of the key, keep it: it simplifies future “covering indexes” and avoids layout churn.

### 4.3 Internal Cell

Internal nodes map `separator_key -> right_child`.

Layout:

```text
[ right_child: u64 ]
[ key_len: varint ]
[ key_bytes: [u8; key_len] ]
```

Lookup rule:

- If `target < key0` => go to `leftmost_child`.
- Else find the **last** key `<= target` and take its `right_child`.

No ambiguity. No “<= in one place, < in another”.

## 5. Ordered Key Encoding (`memcmp` Requirement)

The B+Tree comparator is **byte-wise** (`memcmp`), nothing else.
So the encoding **must preserve order**.

The existing `PropertyValue::encode()` is not suitable because little-endian numeric bytes do not sort naturally.

### 5.1 Type Tags

We define a total order across types via a 1-byte tag:

```text
0x00 Null < 0x01 Bool < 0x02 Int < 0x03 Float < 0x04 String
```

### 5.2 Int (i64)

Encode to sortable unsigned:

- Convert to `u64` by flipping the sign bit: `v ^ 0x8000_0000_0000_0000`.
- Store **big-endian** bytes (network order).

This makes lexicographic order match numeric order.

### 5.3 Float (f64)

IEEE-754 sortable transform:

- Interpret bits as `u64`.
- If sign bit is set (negative): invert all bits.
- Else (positive): flip sign bit.
- Store big-endian bytes.

This is the standard trick used by many KV stores.

### 5.4 String (UTF-8) — Don’t Invent Bugs

Using a single `0x00` terminator is wrong because Rust strings can contain `\0`.

Use **byte-stuffing** (FoundationDB-style):

- Encode bytes, replacing `0x00` with `0x00 0xFF`.
- Terminate with `0x00 0x00`.

This preserves prefix ordering and allows arbitrary bytes.

### 5.5 Composite Key Encoding

Compose:

```text
[ index_id: u32 BE ]
[ ordered_prop_value: bytes ]
[ internal_node_id: u64 BE ]
```

Notes:

- `index_id` in big-endian gives correct ordering by index id.
- Node id in big-endian ensures stable ordering for duplicates (and stable iteration).

## 6. Cursor Abstraction

We need a cursor that:

- can `seek(lower_bound)`
- can `next()` across leaf siblings
- hides page parsing details

### 6.1 Cursor State

```rust
pub struct BTreeCursor {
    root: PageId,
    leaf: PageId,
    slot: u16,
    // Stack for upward traversal is optional initially; range scan only needs leaf+slot.
}
```

### 6.2 Cursor Operations

- `seek_lower_bound(key) -> (leaf, slot)`:
  - descend internal pages using binary search on slots
  - land on leaf page, find first slot with key >= target
- `next()`:
  - if `slot + 1 < cell_count`: `slot += 1`
  - else follow `right_sibling` to next leaf and set `slot = 0`

No recursion required. No heap allocations required in steady state.

## 7. Page Mutations (Minimal Set)

We need exactly three primitives:

1. `insert(cell)` into a leaf (and internal when splitting).
2. `split(page) -> (left, sep_key, right)` for leaf and internal.
3. `compact(page)` to defragment and reclaim space.

Deletion/merge/rebalance can be deferred until we actually need it. Don’t preemptively build a tar pit.

### 7.1 Split Policy

Split is deterministic:

- Build a list of (key, payload) from old page + new cell, sorted.
- Split at median.
- Leaf split:
  - left keeps lower half, right keeps upper half
  - `left.right_sibling = right`
  - separator key = first key in right
- Internal split:
  - promote the median separator key to parent
  - left/right keep their halves
  - `leftmost_child` is handled explicitly in header

### 7.2 Write Safety

v2 has a WAL and a manifest; index pages are updated in compaction/system transactions.
This design assumes we can rely on **transactional page replacement** (CoW pages) at the pager level or the compaction boundary.

Do not attempt “in-place atomic multi-page update”. That way lies corruption.

## 8. Tests (What Locks This Down)

Layout without tests is wishful thinking.

1. Page roundtrip: build page in memory, write, read, parse, compare.
2. Ordered encoding: random values, encode, sort by bytes, decode order matches Rust order.
3. Cursor: insert N keys, iterate, ensure stable sorted order and correct `seek_lower_bound`.
4. Split: force split by small page size in tests, verify invariants and sibling links.

## 9. Implementation Plan (T101)

1. `ordered_key`: implement ordered encoding for `PropertyValue` + composite keys.
2. `btree::page`: header + slot + cell parse/write + `compact`.
3. `btree::cursor`: `seek_lower_bound` + `next`.
4. `btree::builder`: bulk-build from sorted keys (compaction path).
