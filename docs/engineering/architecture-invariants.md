# Architecture Invariants

These rules are always true for the current architecture. Violating them
requires an ADR and updates to product, architecture, validation, and active plan
docs.

## Boundary Invariants

1. **nervusdb-storage owns graph persistence.** Keyspace layout, committed
   durability behavior, recovery-facing behavior, labels, relationship types,
   properties, traversal storage, and logical format versioning live in
   `nervusdb-storage`.

2. **Fjall owns low-level KV persistence.** NervusDB must not reintroduce a
   self-built Pager, WAL, B+Tree, or CSR storage engine for 0.1.

3. **nervusdb-query owns only the Mini-Cypher path before 0.1.** Parser,
   planner, and executor serve the documented Mini-Cypher surface. Query
   behavior outside that surface must fail fast or live outside the current
   main path until a future ADR promotes it.

4. **nervusdb-api is the boundary between query and storage.** It defines
   shared IDs, `PropertyValue`, `GraphSnapshot`, `GraphStore`, and write-boundary
   traits. Neither `nervusdb-query` nor `nervusdb-storage` depends on the other
   directly.

5. **nervusdb is the Rust facade.** It should not grow platform SDK behavior.

6. **nervusdb-cli is a smoke/debug/import tool**, not a separate product
   surface. Its command set is limited to 0.1 core workflows.

## Data Invariants

7. **The public storage path is a directory.** `Db::open(path)` opens a local
   database directory. `.ndb + .wal` is not the current public storage contract.

8. **Format changes require an explicit logical epoch/version.** Incompatible
   graph storage formats fail fast with a compatibility error.

9. **A committed write survives process failure and reopen.** Validation proves
   this through graph-level reopen/crash smoke, not by inspecting backend files.

10. **A partial or uncommitted write must not become visible after recovery.**

11. **Relationship direction and relationship type must survive reopen.** Edge
    storage encodes src/rel/dst explicitly and recovery tests prove round-trip.

12. **Recovery failure surfaces as an error**, not silent continuation or data
    corruption.

## Model Invariants

13. **One writer, snapshot readers.** Write transactions are serialized. Reads
    use `DbSnapshot` / `ReadTxn` from a consistent point in time.

14. **Snapshot isolation.** A read started from a snapshot does not observe
    writes committed after that snapshot's point.

15. **Deterministic query results.** Mini-Cypher queries on the same snapshot
    with the same parameters return identical rows.

16. **Labels and relationship types are separate namespaces.**

17. **0.1 edge identity is `(src, rel, dst)`.** There is no independent edge ID
    and no parallel edge support before a future ADR.

18. **Property keys are original strings, not hashes.** Hashing keys as logical
    identity is forbidden.

19. **Label scan uses storage-level label access.** `MATCH (n:Label)` goes
    through `GraphSnapshot::nodes_with_label(label_id)` and storage-backed
    `label_nodes` data.

## Known Exceptions

- During the Fjall refactor, old storage files may exist in the working tree
  until D4. They are not the current architecture once ADR 0005 is accepted.
- `lookup_index` and `create_index` may remain as compatibility hooks, but
  property indexes are not 0.1 core until a future ADR defines them.
