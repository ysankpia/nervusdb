# NervusDB 0.1 Technical Constitution

This specification is the current engineering constitution for NervusDB.

NervusDB 0.1 is not a full Cypher platform, multi-language SDK platform, vector
database, or release-gate demonstration project. It is a Rust-first embedded
property graph database: SQLite-style local files, crash-safe persistence, and a
small graph query surface.

## 1. Mission

Build SQLite for property graphs:

```text
open(path) -> write graph data -> query one/two-hop relationships -> survive crash -> reopen
```

The project is successful when a Rust application can embed NervusDB, persist
graph data locally, recover safely after process failure, and get trustworthy
results for the 0.1 query surface.

## 2. In Scope Before 0.1

- Rust embedded API for opening a local database path.
- Local file storage with explicit file format versioning.
- WAL-backed crash recovery and reopen correctness.
- Node, relationship, label, and property persistence.
- Single-writer transactions and snapshot-style reads.
- Label scans and neighbor traversal by relationship type.
- Basic property filtering for local graph queries.
- Mini-Cypher for simple reads/writes:
  - `RETURN 1`
  - `MATCH (n)`
  - `MATCH (a)-[:TYPE]->(b)`
  - two-hop traversal such as `MATCH (a)-[:TYPE]->(b)-[:TYPE]->(c)`
  - label match such as `(n:Label)`
  - simple property equality in `WHERE`
  - `RETURN`
  - `LIMIT`
  - basic `CREATE`
  - basic `DELETE`
  - basic `SET` where already stable
  - `EXPLAIN` for supported plans
- CLI support for debugging, import smoke, and local query/write workflows.

## 3. Frozen Before 0.1

These areas remain in the repository as historical or experimental work, but
they do not define product success before 0.1:

- Full openCypher compatibility.
- Procedures, subqueries, pattern comprehension, and complex clause interaction.
- Full openCypher TCK pass rate as a blocking product goal.
- Python, Node.js, or C API stabilization beyond maintenance.
- HNSW/vector search as a default feature.
- Advanced cost-based optimizer work not needed by Mini-Cypher.
- Cross-language parity gates.
- Nightly chaos, soak, fuzz, TCK, perf, stability, and release-window gates as
  default development pressure.

Frozen code can receive build fixes, security fixes, or narrow compatibility
patches. New capability work in these areas requires a decision record first.

## 4. Architecture Boundaries

Core:

- `nervusdb`
- `nervusdb-api`
- `nervusdb-storage`
- the Mini-Cypher path in `nervusdb-query`
- the CLI subset needed for local smoke/debug/import workflows

Experimental:

- `nervusdb-pyo3`
- `nervusdb-node`
- `nervusdb-capi`
- `examples-test/`
- HNSW/vector search
- optimizer work outside the Mini-Cypher core path
- backup/vacuum APIs not needed for the embedded 0.1 loop
- TCK harness, fuzz targets, perf/chaos/soak scripts

Graveyard / frozen:

- full openCypher semantics
- procedures
- subqueries
- pattern comprehension
- temporal/duration breadth
- complex aggregation
- cross-binding parity release gates
- release/stability windows

## 5. Storage Compatibility And Error Model

- File format changes must be explicit and versioned.
- `storage_format_epoch` mismatch must fail fast with `StorageFormatMismatch`.
- WAL/recovery changes require tests that prove committed data survives reopen
  and uncommitted data does not leak after failure.
- Error categories remain: `Syntax`, `Execution`, `Storage`, `Compatibility`.

## 6. Default Validation

The default local and CI-equivalent gate is:

```bash
bash scripts/check.sh
```

That means:

1. `cargo fmt --all -- --check`
2. core-crate clippy for `nervusdb-api`, `nervusdb-storage`,
   `nervusdb-query`, `nervusdb`, and `nervusdb-cli`
3. `bash scripts/workspace_quick_test.sh`

Area-specific scripts for TCK, bindings, perf, fuzz, chaos, soak, and stability
remain available manually, but they are not the default definition of progress.

`scripts/workspace_quick_test.sh` must remain small. Full historical integration
fan-out belongs in `scripts/workspace_full_test.sh` and is manual unless the
change is broad enough to justify the cost.

## 7. 0.1 Acceptance Criteria

0.1 is ready when all of these are true:

- A Rust program can create and reopen a local graph database.
- Nodes, relationships, labels, and properties persist through restart.
- Kill/reopen recovery tests prove committed data is kept and partial writes are
  not exposed.
- One-hop and two-hop query examples are documented and tested.
- Query results for Mini-Cypher are deterministic.
- A 1,000,000 node / 5,000,000 edge smoke or benchmark run completes without
  corruption or crash on documented hardware.
- One-hop and two-hop benchmarks report reproducible P50/P95/P99 numbers.
- Rust API documentation is clear enough for an embedded user to start without a
  server or SDK.
- Ten real examples run end to end.

Anything else is secondary.
