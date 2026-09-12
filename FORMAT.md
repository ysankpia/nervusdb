# GraphLite On-Disk Format (version 4)

> **This document is a commitment, not a description.** It specifies the bytes on
> disk so that a future version — or a third-party reader — can be written without
> reverse-engineering the current code. If the code and this document disagree,
> that is a bug in one of them.

---

## 1. Stability promise

**Starting with 1.0.0, the GraphLite on-disk format does not change in
incompatible ways.** Newer versions will always read and write files created by
1.0.0.

This mirrors SQLite's guarantee, which is why SQLite files remain readable after
two decades. It is the reason you can put data in this database and stop worrying
about it.

Two consequences, stated plainly:

- **The limits in §6 are permanent.** They are sized far beyond any single-machine
  workload this engine targets (see the headroom column). They are documented
  rather than hidden, because an explicit boundary is a feature: it turns a
  possible silent corruption into a clear error at a known point.
- **Changing the format requires an explicit opt-in.** If a future feature
  genuinely cannot be expressed in these bytes, the new version will expose a
  storage-version selector (as DuckDB does with `STORAGE_VERSION`) and keep
  reading version 4. It will not silently reinterpret existing files.

### What "version" means here

`DB_PAGE_VERSION` occupies bytes 4..8 of Page 0. The current value is **4**.

| Version | Meaning                                                    | Readable by 1.0.0?                  |
| ------- | ---------------------------------------------------------- | ----------------------------------- |
| 1       | One 4 KiB property page per entity (prototype)             | No                                  |
| 2       | Slotted property pages                                     | No                                  |
| 3       | Adds page-level CRC32                                      | No                                  |
| **4**   | **Owns its own encoding; 24-bit overflow is a hard error** | **Yes — this is the frozen format** |

Opening a file whose version differs from the running build's is a hard error
(`GraphLite::open` returns before writing anything, including WAL replay — see
§7). The error names both versions and the migration path.

---

## 2. Files

Exactly two files. There is no sidecar, no lock file, no temporary directory.

| File         | Contents                                                          |
| ------------ | ----------------------------------------------------------------- |
| `{path}`     | The database: fixed 4 KiB pages                                   |
| `{path}.wal` | Page-level write-ahead log: 4 KiB page images with a frame header |

A process-level exclusive lock is taken on `{path}` itself (via `File::try_lock`),
so the two-file rule holds even under concurrent opens.

All multi-byte integers are **little-endian**.

---

## 3. Page 0 — header

Offsets are byte offsets within the first 4096-byte page.

| Offset | Size | Field                                        |
| ------ | ---- | -------------------------------------------- |
| 0      | 4    | Magic: `GLDB` (legacy: `GLP4`)               |
| 4      | 4    | `DB_PAGE_VERSION` (u32)                      |
| 8      | 4    | Page size (4096)                             |
| 12     | 4    | Total pages                                  |
| 16     | 8    | Node freelist head                           |
| 24     | 8    | Edge freelist head                           |
| 32     | 4    | Page freelist head                           |
| 36     | 4    | Dictionary overflow page                     |
| 40     | 8    | `node_count`                                 |
| 48     | 8    | `edge_count`                                 |
| 56     | 8    | `next_node_id`                               |
| 64     | 8    | `next_edge_id`                               |
| 72     | 4    | Node directory root                          |
| 76     | 4    | Edge directory root                          |
| 80     | 4    | Overflow freelist head                       |
| 84     | 4    | Inline dictionary length                     |
| 88     | 4    | Inline index-catalog length                  |
| 92     | 4    | Index-catalog overflow page                  |
| 96     | 128  | Direct node pages: 32 × u32                  |
| 224    | 128  | Direct edge pages: 32 × u32                  |
| 352    | 4    | Slotted-property-page freelist head          |
| 356    | 4    | Last allocated property page (write hint)    |
| 360    | 4    | CRC directory root (L1 chain head)           |
| 364    | 1024 | Inline CRC32: 256 × u32, index = page number |
| 1388   | 2708 | Inline dictionary, then inline index catalog |

**The first 32 node pages need no directory** — their page numbers live directly
in Page 0. That keeps a small database to 4 pages total (16 KiB), which is a
hard requirement pinned by `test_sqlite_compact_file_size`.

---

## 4. Records

### NodeRecord — 32 bytes, 128 per page

| Offset | Size | Field                     |
| ------ | ---- | ------------------------- |
| 0      | 1    | `in_use` (1 = live)       |
| 1      | 3    | reserved                  |
| 4      | 4    | `label_id` (0 = none)     |
| 8      | 8    | `first_outgoing_edge_id`  |
| 16     | 8    | `first_incoming_edge_id`  |
| 24     | 4    | Property pointer (see §5) |
| 28     | 4    | Inline integer value      |

Addressing is arithmetic, no index:

```text
PageId = BasePage + (N × 32) / 4096
Offset = (N × 32) mod 4096
```

### EdgeRecord — 64 bytes, 64 per page

| Offset | Size | Field              |
| ------ | ---- | ------------------ |
| 0      | 1    | `in_use`           |
| 1      | 3    | reserved           |
| 4      | 4    | `edge_type_id`     |
| 8      | 4    | Property pointer   |
| 12     | 4    | reserved           |
| 16     | 8    | `src_id`           |
| 24     | 8    | `dst_id`           |
| 32     | 8    | `weight` (f64)     |
| 40     | 8    | `src_prev_edge_id` |
| 48     | 8    | `src_next_edge_id` |
| 56     | 8    | `dst_next_edge_id` |

Neighbours are found by walking these pointer chains — never by scanning.

---

## 5. Property storage

### Packed property pointer (u32)

```text
bits 31..8   page number  (24 bits)
bits  7..0   slot number  (8 bits)
```

- `0` — no properties
- `0xFFFFFFFF` — record lives in an overflow chain
- slot `0xFF` (255) — the overflow marker; slot pages allocate `0..=254`

**Slot 255 is a legitimate input** to the packer: the overflow path passes it
explicitly. It cannot collide with a real slot because `SlottedPropPage::insert`
refuses to allocate beyond 254.

### Slotted property page (`GLSP`)

```text
+--------+------------------+------+------------------+
| Header | Slot array (down)| free | Payload (up)     |
+--------+------------------+------+------------------+
   24 B        4 B/slot                 variable
```

Several records share one page; a whole page is never allocated per entity.
Records larger than 1 KiB go to a `PropertyPage` overflow chain instead.

### Payload encoding (`PropCodec`)

Compact varint framing with ZigZag-encoded integers. Deliberately not a
general-purpose serializer: the framing overhead is a few bytes per entity, and
property keys are stored inline rather than interned (interning would grow the
header dictionary and cause overflow-page churn).

---

## 6. Limits

| Limit                  | Value                                    | Typical single-machine workload | Headroom |
| ---------------------- | ---------------------------------------- | ------------------------------- | -------- |
| **Database file size** | **64 GiB**                               | hundreds of MB                  | ~100×    |
| Page number            | 2²⁴ (24 bits, from the property pointer) | —                               | —        |
| Nodes                  | ~2.1 billion                             | tens of thousands               | ~10⁵×    |
| Edges                  | ~2.1 billion                             | hundreds of thousands           | ~10⁴×    |
| Single property record | unbounded (overflow chain)               | a few KiB                       | —        |
| Inline property record | 1 KiB                                    | —                               | —        |

**Why 64 GiB and not more.** The property pointer packs a page number into 24
bits because `NodeRecord` is 32 bytes and `EdgeRecord` is 64 — every byte counts
at 128 records per page. Widening the pointer to 32 bits would grow `NodeRecord`
to 40 bytes, dropping each page from 128 records to 102: a **20% permanent
capacity loss** on the hot path, paid by every deployment, to buy address space
that the target workload does not use.

SQLite has a comparable limit (281 TB) and documents it. An explicit, tested
boundary is safer than an implicit one that fails unpredictably.

**Both limits are enforced, not assumed:**

- File exceeding 64 GiB → `open` refuses with an explanatory error
  (`check_file_size_limit`).
- Page number exceeding 24 bits → the write fails loudly rather than silently
  wrapping (`pack_prop_ptr` returns `Result`).

The second matters most. The earlier implementation used `debug_assert!` plus a
`& 0x00FFFFFF` mask, and `debug_assert!` is compiled out in release builds — so an
out-of-range page number would have been **silently truncated to a wrong page,
returning another entity's data with no error at all.** That is the worst failure
mode a database can have, and it is why this is now a runtime check.

---

## 7. Write-ahead log

Frames are appended to `{path}.wal`. Each frame:

| Offset | Size | Field                |
| ------ | ---- | -------------------- |
| 0      | 4    | Magic `GWAL`         |
| 4      | 4    | Payload length       |
| 8      | 4    | CRC32 of the payload |
| 12     | n    | Payload              |

A frame whose CRC does not match is treated as a torn tail and stops replay.

### Payload encoding (version 4)

A one-byte tag followed by the fields. **Tag values are part of the format and
are never reused**; a retired tag must keep its slot rather than be reassigned,
or an old WAL would decode as the wrong record type.

| Tag | Record       | Fields                                          |
| --- | ------------ | ----------------------------------------------- |
| 1   | `TxBegin`    | `tx_id:u64`                                     |
| 2   | `PageWrite`  | `tx_id:u64`, `page_id:u32`, `crc32:u32`, `data` |
| 3   | `TxCommit`   | `tx_id:u64`                                     |
| 4   | `TxRollback` | `tx_id:u64`                                     |
| 5   | `Checkpoint` | _(none)_                                        |

Variable-length fields carry a `u32` length prefix. A decoder that finds trailing
bytes after a record rejects the frame rather than ignoring them.

Recovery is streaming and two-pass: the first pass collects committed and rolled
back transaction ids, the second applies committed pages. Peak memory is
`O(transactions)`, not `O(WAL size)`.

### Page 0 metadata encoding (version 4)

`StringDict`:

```text
next_id:u32
count:u32
count × ( id:u32, len:u32 + UTF-8 key )
```

`IndexCatalog`:

```text
labels_count:u32      labels_count × (len:u32 + UTF-8)
properties_count:u32  properties_count × (len(u32) + label, len(u32) + key)
edge_types_count:u32  edge_types_count × (len:u32 + UTF-8)
```

Both decoders validate declared counts against remaining bytes before allocating,
so a corrupted length prefix fails immediately instead of requesting gigabytes.

---

## 8. Page checksums

Every data page carries a CRC32 (IEEE, polynomial `0xEDB88320`):

| Pages   | Storage                                               |
| ------- | ----------------------------------------------------- |
| 1..=255 | Inline array in Page 0 at offset 364                  |
| ≥ 256   | Two-level radix directory rooted at Page 0 offset 360 |

A stored value of `0` means **not recorded** and the page is skipped, never
reported as corrupt. That asymmetry is deliberate: after a crash the engine must
not refuse to start over a page it never got around to recording.

Checksums are computed lazily — on eviction, on flush, and during WAL replay — so
the write hot path pays nothing.

Directory pages carry their own `self_crc`, sealed on every write and verified on
every load. Without it, one corrupted L2 page would report every data page it
covers as a mismatch: thousands of false alarms naming innocent pages while the
real culprit stayed hidden.

**WAL replay refreshes checksums.** Replay runs before the CRC store exists (it
must: replay extends the file, and the page high-water mark derives from its
length), so `open` re-walks the committed frames afterwards and re-registers their
checksums. Skipping that leaves replayed pages holding their _previous_ checksums,
which makes them unreadable — and because `get_node` folds read errors into `None`,
that surfaces as committed data vanishing after a crash.

---

## 9. Zero dependencies

The core library depends on **no** third-party crates. The encoders in
`src/codec.rs`, `src/json.rs`, and `src/crc32.rs` are part of this repository, and
this document specifies their output.

This is a durability requirement, not a preference. The format's bytes were
previously produced by `bincode`, which **ceased maintenance in December 2025**
(its final "release" contains only a compiler error and a notice). Had the format
been frozen first, this database's lifetime would have been tied to an abandoned
crate with no security updates — the same shape of risk that left Kuzu's users
stranded when that project was archived.

`tests/zero_dependency_tests.rs` enforces the invariant, including a negative test
that adding a dependency makes the guard fail.

---

## 10. Changing this format

A change that alters any byte specified above requires:

1. A `DB_PAGE_VERSION` bump.
2. An entry in `CHANGELOG.md` under **Storage format**, stating plainly that older
   databases need a `.dump` / re-import.
3. A migration path — or, for a change that cannot break readers, a
   storage-version selector so both layouts remain readable.
4. Updating this document **in the same commit**. A format change that leaves this
   file stale is worse than no document at all, because the next reader will trust it.
