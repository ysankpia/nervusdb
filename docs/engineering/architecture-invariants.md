# Architecture Invariants

These rules are always true for the current architecture. Violating them
requires an ADR and updates to product, architecture, validation, and active plan
docs.

## Boundary Invariants

1. **nervusdb-storage owns durability.** File layout, WAL, recovery, page
   management, and format versioning live in nervusdb-storage. No other crate
   writes `.ndb` or `.wal` files directly.

2. **nervusdb-query owns only the Mini-Cypher path before 0.1.** Parser,
   planner, and executor serve the documented Mini-Cypher surface. Query
   behavior outside that surface is compatibility residue and not a 0.1 promise.

3. **nervusdb-api is the boundary between query and storage.** It defines
   `GraphSnapshot` and related traits. Neither nervusdb-query nor
   nervusdb-storage depends on the other directly.

4. **nervusdb is the Rust facade.** It should not grow platform SDK behavior
   (Python, Node.js, C wrappers). Bindings go through nervusdb-capi or their own
   crate.

5. **nervusdb-cli is a smoke/debug/import tool**, not a separate product
   surface. Its command set is limited to 0.1 core workflows.

## Data Invariants

6. **Format changes require an explicit epoch/version.** `STORAGE_FORMAT_EPOCH`
   is checked on open. Incompatible formats fail fast with a compatibility error.

7. **A committed write survives process failure and reopen.** The commit path
   appends graph changes to WAL, appends `CommitTx`, then calls `wal.fsync()`.

8. **A partial or uncommitted write must not become visible after recovery.**
   WAL replay skips incomplete `BeginTx ... CommitTx` sequences.

9. **Relationship direction and relationship type must survive reopen.** Edge
   storage encodes src/rel/dst explicitly and recovery tests prove round-trip.

10. **Recovery failure surfaces as an error**, not silent continuation or data
    corruption.

## Model Invariants

11. **One writer, snapshot readers.** Write transactions are serialized by
    `write_lock`. Reads use `DbSnapshot` / `ReadTxn` from a consistent point in
    time.

12. **Snapshot isolation.** A read started from a snapshot does not observe
    writes committed after that snapshot's point.

13. **Deterministic query results.** Mini-Cypher queries on the same snapshot
    with the same parameters return identical rows.

14. **Label/relationship-type names are interned.** All label and rel-type
    lookups go through `LabelInterner`; IDs are stable within a database session.

## Storage Invariants

15. **Single `.ndb` + `.wal` file pair per database.** Path derivation is
    deterministic: `Db::open(path)` derives `path.ndb` and `path.wal` unless
    `Db::open_paths` is used directly.

16. **Page-level write-ahead logging.** All mutations go through WAL before page
    store. The WAL format encodes transaction boundaries for crash replay.

17. **The `.ndb` meta page stores file magic, version, page size, bitmap page
    id, next page id, ID-map root/length, index catalog root, next index id, and
    `storage_format_epoch`.**

## Known Exceptions

- HNSW/vector index code lives inside `nervusdb-storage` (boundary invariant 1
  exception — storage owns it but the vector path is experimental, not core).
- `nervusdb-capi` wraps `nervusdb` for C ABI; it is not a separate SDK crate
  but exists for binding compatibility.
- `nervusdb-pyo3` and `nervusdb-node` directly depend on `nervusdb-capi` rather
  than `nervusdb` — this bypasses the Rust facade and is accepted before 0.1
  for maintenance only.
