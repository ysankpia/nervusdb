# Code Architecture

> Historical snapshot: this document describes the pre-ADR-0005 code structure.
> It is not the current storage or query architecture. Current architecture is
> `docs/architecture/overview.md`, `docs/architecture/storage-model.md`, and
> `docs/architecture/query-model.md`. The active refactor plan is
> `docs/plans/active/010-fjall-storage-refactor.md`.

NervusDB is a Rust-first embedded graph database organized into five crates.
This document covers the internal structure, key data flows, and design rules
for every core module.

## Workspace Layout

```
nervusdb/              # Rust facade — high-level Db + WriteTxn
nervusdb-api/          # trait-only crate  (GraphSnapshot, GraphStore)
nervusdb-storage/      # durability, page store, WAL, traversal storage
nervusdb-query/        # Mini-Cypher parser → planner → executor
nervusdb-cli/          # smoke / debug / import binary
```

No crate outside `nervusdb` depends on `nervusdb-storage` or `nervusdb-query`
directly — all access goes through the facade or the API traits.

---

## nervusdb (facade)

**Entry points:** `Db::open`, `Db::begin_write`, `Db::snapshot`

```
nervusdb/src/
  lib.rs      # Db (wraps GraphEngine), WriteTxn (wraps engine WriteTxn)
  error.rs    # Error enum (Io, Storage, Compatibility, Query, Other)
```

`Db` owns a `nervusdb_storage::engine::GraphEngine` and exposes:
- `snapshot()` → `Snapshot` (read-only view)
- `begin_write()` → `WriteTxn` (exclusive-writer transaction)
- `close()` → checkpoint-on-close
- `create_index(label, property)` → index creation

`WriteTxn` delegates every mutation to the storage engine's `WriteTxn` and
calls `.commit()` to persist.

---

## nervusdb-api

**Pure trait crate** — no logic, no storage dependencies.

```
nervusdb-api/src/lib.rs
  trait GraphSnapshot   — read interface (neighbors, properties, labels, nodes)
  trait GraphStore      — snapshot factory
```

`InternalNodeId` is `u32`, `ExternalId` is `u64`. Property values are an enum
(`Null`, `Bool`, `Int`, `Float`, `String`, `List`, `Map`, `Date`, `Time`,
`DateTime`, `Duration`, `Bytes`, `Point`) — `nervusdb_api::PropertyValue`.

---

## nervusdb-storage (engine)

**39 source files.** The largest and most complex crate.

### Module Tree

```
nervusdb-storage/src/
  lib.rs          — re-exports (PAGE_SIZE, api, engine, etc.)
  error.rs        — Error enum (Io, WalProtocol, StorageFormatMismatch, etc.)
  pager.rs        — PageId, Pager (page allocation, read/write, bitmap, meta)
  wal.rs          — Wal, WalRecord enum, recovery (replay_committed)
  idmap.rs        — IdMap: external_id ↔ internal_node_id, labels
  label_interner.rs  — LabelSnapshot: string ↔ LabelId
  memtable.rs     — MemTable: in-memory write buffer before commit
  csr.rs          — CsrSegment: compressed-sparse-row adjacency
  property.rs     — PropertyValue (storage variant)
  snapshot.rs     — Snapshot: published read view
  stats.rs        — GraphStatistics (node/edge counts)
  engine.rs       — GraphEngine (open, begin_read, begin_write, commit)
  blob_store.rs   — Large property blob storage
  api.rs          — GraphSnapshot impl for external code

  index/          — B-tree index (btree.rs, catalog.rs, ordered_key.rs)

  read_path_*.rs  — 16 files of read-path helper functions
    read_path_api_iter.rs, read_path_api_props.rs, read_path_api_stats.rs
    read_path_convert.rs, read_path_engine_idmap.rs, read_path_engine_labels.rs
    read_path_engine_view.rs, read_path_iters.rs, read_path_labels.rs
    read_path_neighbors.rs, read_path_nodes.rs, read_path_overlay.rs
    read_path_property_store.rs, read_path_run_edges.rs, read_path_run_iters.rs
    read_path_run_property_maps.rs, read_path_run_props.rs, read_path_run_state.rs
    read_path_stats.rs, read_path_symbols.rs, read_path_tombstones.rs
```

### Core Data Structures

#### Pager (`pager.rs`)

Flat page file. Pages are fixed-size (`PAGE_SIZE = 4096`). Layout:

```
Page 0 (META)     — version, next_page_id, i2e_start, index_catalog_root, etc.
Page 1 (BITMAP)   — free/allocated bitmap
Page 2+ (DATA)    — application data
```

`Pager` supports `read_page`, `write_page`, `allocate_page`, `free_page`,
`sync` (fsync). Thread-safe via `Arc<RwLock<Pager>>`.

#### Wal (`wal.rs`)

Append-only write-ahead log. Records are binary-serialized with CRC32:

```
WalRecord enum:
  BeginTx(txid), CommitTx(txid),
  CreateNode, AddNodeLabel, RemoveNodeLabel,
  CreateEdge, TombstoneNode, TombstoneEdge,
  SetNodeProperty, SetEdgeProperty, RemoveNodeProperty, RemoveEdgeProperty,
  ManifestSwitch, Checkpoint, PageWrite, PageFree, CreateLabel
```

Recovery (`replay_committed`) scans the WAL, splits by `BeginTx`/`CommitTx`,
returns only committed transactions. Each committed tx produces an `L0Run`.

#### IdMap (`idmap.rs`)

Bidirectional mapping: `ExternalId (u64) ↔ InternalNodeId (u32)`.

Internal IDs are dense (0..n). The `i2e` array is stored in pages starting at
`i2e_start_page`. Each `I2eRecord` is 16 bytes: `external_id (u8) | label_id
(u4) | flags (u4)`. `E2i` is a `HashMap<ExternalId, InternalNodeId>` for O(1)
lookup.

#### LabelInterner (`label_interner.rs`)

Bidirectional string↔LabelId: `s2i: HashMap<String, LabelId>` and
`i2s: Vec<String>`. Snapshots are `Arc`-wrapped for lock-free reads.

IdMap and LabelInterner are **different** — IdMap stores per-node primary
labels as part of its record; LabelInterner stores the name↔ID registry.

#### CSR Segments (`csr.rs`)

Compressed sparse row (CSR) adjacency storage. Each `CsrSegment` covers a
range of source nodes (`min_src..max_src`) and contains:

```
offsets[src - min_src]  → index into edges[] where this src's edges start
edges[]                 → (rel, dst) pairs
in_offsets[]            → same for incoming
in_edges[]              → (rel, src) pairs
```

Used for efficient neighbor iteration: O(1) lookup to find the edge range for
a source node, then scan edges within that range.

#### MemTable (`memtable.rs`)

In-memory write buffer inside `WriteTxn`. Collects:

- `out, in_: HashMap<InternalNodeId, Vec<EdgeKey>>` — edge additions
- `tombstoned_nodes/edges` — deletions
- `node_properties, edge_properties` — property sets
- `removed_node_properties, removed_edge_properties` — property removals

On `commit()`, MemTable is frozen into an `L0Run`.

#### L0Run & Snapshot (`snapshot.rs`)

An `L0Run` is an immutable, published run from one committed transaction.
Contains all edges, tombstones, and properties for that tx.

`Snapshot` is the composite read view: a vector of `Arc<L0Run>` (newest
first) plus `Arc<CsrSegment[]>` plus label metadata. Read operations search
runs in order and return the first match.

### Write Path

```
Db::begin_write()
  → engine.begin_write()
    → acquires write_lock (Mutex)
    → allocates txid from AtomicU64 counter
    → returns WriteTxn { engine, memtable, txid }

WriteTxn::create_node / create_edge / set_node_property / tombstone_node / etc.
  → mutates engine.idmap (pending) or engine.memtable

WriteTxn::commit()
  1. Serialize all ops to WAL (BeginTx, records, CommitTx)
  2. wal.fsync()  ← durability point
  3. Freeze memtable into L0Run
  4. Apply index updates (B-tree insert/delete)
  5. Flush IdMap pages, pager.sync()
  6. Publish updated snapshots via ArcSwap
  7. If threshold triggered: flush CSR segments + manifest switch
```

### Read Path

```
Db::snapshot()  or  GraphEngine::begin_read()
  → Snapshot {
      runs:        ArcSwap<PublishedRuns>,         // L0Run[]
      segments:    ArcSwap<PublishedSegments>,       // CsrSegment[]
      labels:      Arc<LabelSnapshot>,
      node_labels: Arc<Vec<Vec<LabelId>>>,
    }

Snapshot::neighbors(src, rel)
  → NeighborsIter that merges:
      (1) CSR segments — fast adjacency lookup
      (2) L0 runs — newest-first, for edge additions

Snapshot::node_property / edge_property
  → scan L0 runs (newest first), return first match

Snapshot::nodes()
  → iterate dense ID range, skip tombstoned nodes
```

Reads are always lock-free — every published field is behind `ArcSwap` or
`Arc`.

---

## nervusdb-query (Mini-Cypher)

**26 module files** in the query crate.

### Module Tree

```
nervusdb-query/src/
  lib.rs                  — re-exports, parse()
  error.rs                — Error, ResourceLimitKind
  lexer.rs                — Lexer → Token[]
  parser.rs               — Parser → AST Query
  ast.rs                  — AST types (Query, Clause, Match, Where, etc.)
  evaluator/              — Expression evaluation
  executor/
    plan_types.rs          — Plan enum (22 variants), PlanIterator enum
    plan_dispatch.rs       — execute_plan() dispatcher
    plan_head.rs           — NodeScan, CartesianProduct
    plan_mid.rs            — Filter, Project, OrderBy, Aggregate, OptionalWhereFixup
    plan_tail.rs           — Skip, Limit, Distinct, Unwind, Union, Values
    match_out_plan.rs      — MatchOut (single-hop), MatchOutVarLen
    match_bound_rel_plan.rs — MatchBoundRel (bound-relationship pattern)
    create_delete_ops.rs   — write ops implementation
    write_dispatch.rs      — execute_write() dispatcher
    write_forwarders.rs    — delegate to write_path
    write_path.rs           — SET, REMOVE implementation
    read_path.rs            — ExpandIter, MatchOutVarLenIter
    core_types.rs           — Row, Value, NodeValue, etc.
    txn_engine_impl.rs      — WriteableGraph impl
    property_bridge.rs      — PropertyValue conversion
    label_constraint.rs     — LabelConstraint matching
    plan_iterators.rs       — misc iterator types
  query_api/
    prepare_entry.rs        — prepare() → PreparedQuery
    compile_core.rs         — compile_m3_plan (clause → Plan pipeline)
    planner.rs              — build_logical + build_physical
    plan/                   — LogicalPlan, PhysicalPlan, optimizer
    match_compile.rs        — match pattern → Plan compilation
    write_compile.rs        — DELETE/SET/REMOVE → Plan compilation
    write_create_merge.rs   — CREATE/MERGE → Plan compilation
    projection_compile.rs   — RETURN/With projections
    return_with.rs          — RETURN/WITH compilation
    binding_analysis.rs     — variable binding inference
    where_validation.rs     — WHERE expression validation
    type_validation.rs      — expression type checking
    foreach_compile.rs      — FOREACH compilation
    merge_set.rs            — ON CREATE / ON MATCH compilation
    aggregate_parse.rs      — COUNT/SUM/AVG etc.
    plan_render.rs          — EXPLAIN output
    plan_introspection.rs   — plan_contains_write()
    pattern_predicate.rs    — pattern predicate validation
    internal_alias.rs       — internal path alias generation
    match_anchor.rs         — pattern re-anchoring
    ast_walk.rs             — tree walking helpers
    prepared_query_impl.rs  — PreparedQuery::execute_streaming
  facade.rs                 — query_collect(), type re-exports
```

### Query Pipeline

```
Cypher string
    │
    ▼
Parser.parse()       ──→ AST (Query with Clause[])
    │
    ▼
prepare()            ──→ PreparedQuery
  │  Parser.parse_with_merge_subclauses() → (Query, MergeSubclauses)
  │  build_logical(query, merge_subclauses) → LogicalPlan
  │  optimizer::optimize(logical)            → LogicalPlan (optimized)
  │  build_physical(optimized)              → PhysicalPlan
  │                                         → PreparedQuery { plan, write_semantics, merge_* }
    │
    ▼
PreparedQuery::execute_streaming(snapshot, params)
  │  execute_plan(snapshot, plan, params)   → PlanIterator (lazy rows)
  │  or execute_write(plan, snapshot, txn, params)  → u32 (rows affected)
```

### Plan Variants (22)

The `Plan` enum in `plan_types.rs`:

| Variant | Purpose | Status |
|---|---|---|
| `ReturnOne` | `RETURN 1` — constant row | Active |
| `NodeScan` | `MATCH (n)` / `MATCH (n:Label)` | Active |
| `MatchOut` | `(a)-[:TYPE]->(b)` | Active |
| `MatchOutVarLen` | `(a)-[:TYPE*1..3]->(b)` | Active |
| `MatchBoundRel` | `(a)-[r]->(b)` | Active |
| `MatchIn` | `(a)<-[:TYPE]-(b)` | Frozen: errors at dispatch |
| `MatchUndirected` | `(a)-[:TYPE]-(b)` | Frozen: errors at dispatch |
| `Filter` | `WHERE expr` | Active |
| `OptionalWhereFixup` | OPTIONAL + WHERE semantics | Active |
| `Project` | `RETURN expr AS alias` | Active |
| `Aggregate` | `COUNT`, `SUM`, etc. | Frozen: errors at dispatch |
| `OrderBy` | `ORDER BY expr` | Active |
| `Skip` | `SKIP n` | Active |
| `Limit` | `LIMIT n` | Active |
| `Distinct` | `RETURN DISTINCT` | Active |
| `Unwind` | `UNWIND list AS item` | Active |
| `Union` | `UNION` / `UNION ALL` | Active |
| `Delete` | `DELETE n` | Active |
| `SetProperty` | `SET n.prop = val` | Active |
| `SetPropertiesFromMap` | `SET n = {…}` | Active |
| `SetLabels` | `SET n:Label` | Active |
| `RemoveProperty` | `REMOVE n.prop` | Active |
| `RemoveLabels` | `REMOVE n:Label` | Active |
| `IndexSeek` | Index-driven scan | Frozen: errors at dispatch |
| `CartesianProduct` | `MATCH a, b` | Active |
| `Create` | `CREATE (n:Label)` | Active |
| `Merge` | `MERGE (n:Label)` | Active (via compile_m3_plan) |
| `Apply` | Correlated subquery | Frozen: errors at dispatch |
| `ProcedureCall` | `CALL proc()` | Frozen: errors at dispatch |
| `Foreach` | `FOREACH` | Frozen: errors at dispatch |
| `Values` | Literal rows (internal) | Active |
| `OptionalWhereFixup` | Null-row injection for OPTIONAL | Active |

"Frozen" variants still exist in the enum but return an error in
`plan_dispatch::execute_plan`. They remain to avoid cascade refactors in the
planner and are removed when the planner is fully tight.

### Expression Evaluator

`evaluator/` evaluates `Expression` AST nodes against a `Row` + `Params`:

```
Expression enum:
  Variable, Property, Parameter, Literal,
  UnaryOp, BinaryOp, FunctionCall,
  CaseExpr, ListComprehension, MapProjection,
  PatternPredicate, ExistsSubquery, CountStar
```

Supported functions: `id`, `type`, `labels`, `properties`, `timestamp`,
`coalesce`, `toInteger`, `toFloat`, `toString`, `toBoolean`, `keys`,
`head`, `last`, `range`, `size`, `reverse`, `duration`, `date`, `time`,
`datetime`, `localdatetime`, `localTime`.

### Write Dispatch

`execute_write()` dispatches `Plan::Create`, `Delete`, `SetProperty`,
`SetPropertiesFromMap`, `SetLabels`, `RemoveProperty`, `RemoveLabels` to the
corresponding functions in `write_path.rs` and `create_delete_ops.rs`.

Every write function receives a `&mut dyn WriteableGraph` (implemented by the
storage engine's `WriteTxn`).

---

## nervusdb-cli

```
nervusdb-cli/src/main.rs
  Cli → Commands → V2Args → V2Commands { Query, Write, Repl }
```

- `nervusdb v2 query "MATCH …"` — execute read-only MINI-Cypher
- `nervusdb v2 write "CREATE …"` — execute write query
- `nervusdb v2 repl` — interactive REPL

All CLI commands open a `Db` at a hard-coded or argument-provided path.

---

## Data Flow Summary

### Read: `MATCH (n:Person) WHERE n.age > 30 RETURN n.name`

```
Cypher string
  → Parser.parse() → Query { Match, Where, Return }
  → prepare() → CompiledQuery { Plan::Project { input: Plan::Filter {
      input: Plan::NodeScan { label: "Person" },
      predicate: BinaryOp(GT, Property("n.age"), Literal(30))
    }, projections: [("n.name", Property("n.name"))] }}
→ execute_streaming(snapshot, params)
  → plan_dispatch::execute_plan
    → NodeScanIter: scan node_labels[], skip tombstones → emit Row{ n }
    → FilterIter: evaluate predicate per row → pass if true
    → ProjectIter: evaluate "n.name" expression → emit Row{ n.name }
```

### Write: `CREATE (p:Person {name: "Alice"})`

```
  → prepare() → Plan::Create { pattern: Person(name: "Alice") }
  → execute_write → write_dispatch::execute_write
    → execute_create(snapshot, txn, pattern)
      → txn.create_node(eid, label_id)
      → txn.set_node_property(node, "name", "Alice")
  → txn.commit()
    → WAL (BeginTx, CreateNode, SetNodeProperty, CommitTx)
    → memtable.freeze_into_run → L0Run
    → pager.sync()
    → ArcSwap publish
```

### Recovery (on `Db::open`)

```
GraphEngine::open(path)
  → Pager::open(.ndb)
  → Wal::open(.wal)
  → wal.replay_committed()
    → scan WAL, group by BeginTx/CommitTx
    → return Vec<Vec<WalRecord>> (committed tx only)
  → replay_label_transactions → build LabelInterner
  → replay_graph_transactions → produce L0Run[]
  → load CSR segments from manifest
  → publish everything via ArcSwap
```

---

## Concurrency Model

| Resource | Protection | Details |
|---|---|---|
| Page file (pager) | `Arc<RwLock<Pager>>` | Multiple readers, exclusive writer |
| WAL | `Mutex<Wal>` | Serialized append |
| IdMap | `Mutex<IdMap>` | Modified only during commit |
| LabelInterner | `Mutex<LabelInterner>` | Modified only during commit |
| Published state | `ArcSwap` | Lock-free reads, atomic swap on publish |
| Writer lock | `Mutex<()>` | One write transaction at a time |
| `next_txid` | `AtomicU64` | Contention-free counter |

Snapshots are created by cloning `Arc` handles — no lock is held. This gives
readers a consistent point-in-time view without blocking writers.

---

## Key Design Rules (source of truth: code)

- **Page format is flat** — no b-tree storage for nodes/edges. CSR segments
  handle adjacency; L0 runs handle incremental edge/property mutations.
- **WAL is the commit log** — durability comes from WAL fsync, not page file
  writes. Pages are flushed lazily.
- **Snapshots are free** — cloning an `Arc` is O(1). Readers never see partial
  writes.
- **Plans are trees** — every `Plan` variant has an `input: Box<Plan>` (or
  equivalent). The `execute_plan` dispatcher pattern-matches and delegates to
  specialized iterator types.
- **Frozen ≠ dead** — `MatchIn`, `MatchUndirected`, `IndexSeek`, `Apply`,
  `ProcedureCall`, `Foreach`, `Aggregate` remain in the Plan enum but return
  errors at dispatch. They are removed when the planner is fully tight.
- **Tests are integration-level** — 16 tests: 9 for Mini-Cypher (via the
  `nervusdb` facade), 6 for storage, 1 Rust API. No per-module unit tests.
