# Design T102: IndexCatalog & On-Disk Index API

> **Status**: Draft
> **Parent**: T100 (Architecture)
> **Depends On**: T101 (B+Tree page layout & cursor)
> **Implementation Target**: `nervusdb-storage/src/index/catalog.rs`

## 0. Scope (Don’t Make This a Tar Pit)

T102 introduces a minimal **IndexCatalog** so the storage engine can:

1. Create/load an index by name (e.g. node property key `"age"`).
2. Get its `index_id` (stable ordering key prefix).
3. Get its B+Tree root page id to perform reads/writes.

Non-goals for T102:

- No optimizer integration (query planner changes are later tasks).
- No multi-tenant / schema versioning.
- No online concurrent structural modifications (single-writer anyway).
- No durability guarantees beyond existing WAL/checkpoint boundaries (CoW is a later refinement).

## 1. Data Model

**IndexName**: `String` (UTF-8)

**IndexId**: `u32` monotonically assigned (starts at 1). Used as the first component of the composite key.

**IndexRoot**: `PageId` (u64). Root page for the B+Tree of that index.

Catalog entry:

```text
{ name: String, id: u32, root: u64 }
```

## 2. Persistence Strategy

We store the catalog pointer in the Pager meta page as **optional appended fields**.
Existing `.ndb` files have zeros there, meaning “no catalog”.

Pager meta appended fields (LE):

- `index_catalog_root: u64` (0 if none)
- `next_index_id: u32` (0 treated as 1)

This is backward-compatible for v2: old files still open, new fields default to 0.

Catalog page format is a simple slotted record list stored in one (or more) pager pages:

```text
[ magic: "NDBXCAT1" (8 bytes) ]
[ count: u16 ]
[ reserved: ... ]
[ records... ]

record:
  [ name_len: u16 ]
  [ name_bytes ]
  [ index_id: u32 ]
  [ root_page: u64 ]
```

MVP rule: one page is enough; if it grows, allocate overflow pages (T102.1+).

## 3. Index Key Convention

Re-use T101 composite key:

```text
[ index_id: u32 BE ][ ordered_value ][ internal_node_id: u64 BE ]
```

For equality lookup by value:

- Seek lower bound for prefix `[index_id][ordered_value]` with node_id = 0
- Iterate while key starts with that prefix; yield node ids

## 4. Public API (Storage)

Expose minimal entry points:

- `IndexCatalog::open_or_create(pager)`
- `IndexCatalog::get_or_create(name) -> IndexHandle`
- `IndexHandle::insert(value, internal_node_id)`
- `IndexHandle::seek_equal(value) -> Iterator<internal_node_id>`
- `IndexHandle::delete_exact(value, internal_node_id)` (naive leaf delete, no rebalance)

Deletion is explicitly “best-effort MVP”: it removes the entry if found, without tree rebalancing.
Compaction (T103) will be the long-term “clean up” path.

## 5. Tests

- Catalog persists across reopen (create index, close pager, reopen, find same id/root).
- Equality seek returns all ids for same value.
- Delete removes one exact tuple and does not affect others.

