# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Because this is a database, two kinds of change are called out explicitly, as a
user has to act on them:

- **Storage format** — the on-disk layout changed. Old files may not open. The
  migration path is a logical dump and re-import (see README).
- **Behavioural** — existing queries or APIs behave differently.

---

## [Unreleased]

### Fixed

- **CI was red on the `v1.0.0-rc.1` tag.** Two checks failed that local
  verification had missed, both because the local toolchain was older than CI's:
  `rustdoc -D warnings` rejected unescaped angle brackets in doc comments
  (`<expr>`, `<NodeId>`), and a newer clippy flagged
  `self.ops.drain(..).collect()` in the commit path (a needless allocation).
  Fixed the code rather than pinning an older compiler.

  _If you build the `v1.0.0-rc.1` tag itself, expect those two lint failures;
  they are doc-comment and lint-level only and do not affect the library. Use
  `main` or a later tag._

### Changed

- **Contribution policy: this repository no longer accepts external code.**
  Pull requests from anyone other than the owner, a member or a collaborator are
  closed automatically. The reason is licensing (dual AGPL + commercial), not
  code quality: merging an outside patch licensed under AGPL alone would make
  that code unusable for the commercial licence and break the model
  project-wide. Issues remain welcome and are acted on. See CONTRIBUTING.md.

  _If you were planning to send a patch, please open an issue with a
  reproduction instead._

- `CLA.md` now applies **only** to changes the maintainer explicitly invites.
  Opening a pull request on your own is no longer, and is not treated as,
  acceptance of it.

- The README no longer describes the project as "production-grade". The release
  notes and roadmap list known production gaps, so the claim contradicted them.
  It now states plainly that this is a release candidate and links to the
  limitations.

### Added

- `CHANGELOG.md` (this file).
- `ROADMAP.md` — planned work, explicit non-goals, and the rule that any
  performance claim must ship with a runnable scenario and its measurement
  conditions.
- `LICENSING.md` — plain-language explanation of the dual-licence model,
  including that AGPL does not prohibit commercial use.
- `CONTRIBUTING.md`, `CLA.md`.
- `SECURITY.md` — how to report a vulnerability privately.
- Reproducible benchmarks: `benches/throughput.rs`, `benches/pool_probe.rs`,
  `benches/mem_probe.rs`, plus `benches/throughput.py` and
  `benches/throughput.mjs` for the SDKs. Each scenario prints its own
  configuration, so a number cannot be quoted without its conditions.
- GitHub Actions CI (format, check, clippy, tests, release-mode throughput
  suites, rustdoc) on Linux and macOS.

---

## [1.0.0-rc.1] — 2026-09-11

First public pre-release. Everything below is new; the engine was built and
verified in this cycle.

### Added — storage kernel

- Single-file storage: `{path}` plus a page-level WAL `{path}.wal`. Two files
  only, no sidecar.
- 4KB physical paging with an LRU buffer pool, page pin/unpin counting, dirty
  tracking and page-level latches.
- Fixed-size records with O(1) physical addressing: `NodeRecord` 32 bytes,
  `EdgeRecord` 64 bytes.
- Disk-native index-free adjacency: double-cyclic edge pointer chains traversed
  through the buffer pool. Neighbour lookup never scans.
- Slotted property pages: several variable-length payloads packed per page, 1KB
  inline threshold, overflow chains above it, and a compact varint codec
  (`PropCodec`) replacing bincode-framed maps.
- Property pointers packed as 24-bit page + 8-bit slot; slot reclamation with
  in-page compaction and a whole-page freelist.
- Freelist slot reuse for both node and edge records.
- Crash recovery by WAL replay, with CRC verification per frame and safe
  truncation of a torn trailing frame.

### Added — ACID and transactions

- Explicit transactions with single-fsync group commit.
- STEAL spilling: when the pool fills mid-transaction, uncommitted pages spill to
  the WAL tracked by a page-location index, so one transaction can mutate far
  more pages than the pool holds.
- Exact rollback: per-page baselines restore content and index positions, leaving
  the main file byte-identical.
- `with_transaction` for single-fsync bulk ingestion.

### Added — Cypher 1.0

- `CREATE`, `MATCH` (including multi-pattern joins), `WHERE` with label
  predicates, `SET` (property and label), `DELETE` / `DETACH DELETE`,
  `ORDER BY`, `SKIP`, `LIMIT`.
- Aggregates: `count`, `sum`, `avg`, `min`, `max`, with grouping.
- Variable-length paths (`*1..3`) and undirected edges.
- Multi-label node patterns; `RETURN *` expansion from actual bindings.

### Added — analytics

- Breadth-first search, Dijkstra shortest path, directed cycle detection.
- PageRank with configurable damping, iteration cap and tolerance, redistributing
  dangling mass and normalising scores to 1.0.
- Weakly connected components (union-find, undirected semantics).
- K-hop subgraph extraction with direction and edge-type filtering.

### Added — production safety

- Exclusive single-writer open lock, taken on the data file itself via
  `std::fs::File::try_lock` (no new dependency, no sidecar file). A contended
  open returns `DatabaseLocked` instead of silently losing a writer's data. The
  lock is acquired **before** WAL replay, which itself writes the main file.
- `integrity_check()` and `verify()`, backed by a degree-conservation oracle:
  chain degree walked from on-disk pointers must equal expected degree from an
  independent scan of the edge id space. Catches chain damage that leaves counts
  self-consistent.
- `try_get_node` / `try_get_edge` preserve storage errors; `get_node` /
  `get_edge` are documented as lossy.
- Poison-recovering lock accessors replace 71 panic sites, so a poisoned lock
  cannot abort the process.

### Added — tooling and bindings

- Interactive CLI: multi-line input, ASCII tables, and
  `.schema` / `.stats` / `.checkpoint` / `.dump` / `.history` / `.help`.
- Logical dump (`dump_cypher`) that replays into a fresh database, and doubles as
  the migration path across storage format changes.
- Python SDK (PyO3) and Node.js SDK (NAPI-RS) with transaction support.

### Added — indexing

- Label inverted index and `(label, property)` index, used for start-node
  selection. Indexes self-heal after property updates, label changes and failed
  transactions, and rebuild on demand after a cold restart.

### Fixed — correctness

Found by the adversarial test suites written in this cycle:

- **Self-loop insertion dropped the outgoing chain head.** The single-edge path
  wrote the source update and then clobbered it with a stale target copy when
  `src == dst`, losing the chain. Both updates are now merged for self-loops.
- **`MATCH (a)-[:R]->(a)` overwrote the first binding of `a`** instead of
  treating the repeated variable as a join constraint. Repeated variables are
  now a constraint, not a silent reassignment.
- **Quadratic incoming-chain planning.** The batch weave looked up positions with
  a linear scan per edge, which went quadratic when many edges shared one target.
  Replaced with position indexes.
- **`SQLITE`-style silent data loss on concurrent open** — two handles each
  reported success and the second write vanished (measured: two writers, one
  surviving node, no error).
- **Silent corruption went unreported** — corrupting one 4KB page produced a
  successful open, no error, and 156 of 200 nodes silently wrong or missing.

### Fixed — performance

Discrete edge writes degraded as the graph grew. Root cause was not cache hit
rate but three structural defects:

- **Per-edge weaving** touched the source page, target page and old head page for
  every edge; with pages far exceeding the pool, the same page was evicted and
  re-read repeatedly per batch, and each miss spilled a full 4KB page to the WAL
  ("false spill"). Replaced by two-phase batch weaving: distinct touched nodes
  are read once in page order, chain pointers are derived in memory, edge records
  are written in id order, and node head pointers are written once per node.
- **`LRUReplacer` was O(pool size) per operation** — a `VecDeque` with linear
  scans in `pin`, `unpin` and victim selection. This was the real throughput
  ceiling: at 16,384 frames (misses ≈ 0) throughput was 47k ops/s, an order of
  magnitude below the 1024-frame case. Replaced with an intrusive doubly-linked
  list, making every operation O(1).
- **Page directory pages were evictable**, so each node/edge address resolution
  could be forced to re-walk the whole directory chain. Page 0 and all directory
  pages are now substitution-exempt.

### Changed — behavioural

- **Storage format `DB_PAGE_VERSION` 1 → 2** (slotted property pages). Files
  written by the earlier one-page-per-entity layout are **not readable**; `open`
  returns an explicit error directing you to `.dump` and re-import. It never
  silently reinterprets an old file.
- `DELETE` on a node that still has relationships is now a hard error advising
  `DETACH DELETE`, rather than an implicit cascade.
- Edge weight validation moved from buffering time to commit-apply time, so an
  invalid edge fails the whole transaction atomically.
- Batched edge writes now order chains per source node rather than in strict
  client order. Outgoing chain order is preserved; **incoming** chain order for a
  target with edges from several sources may differ from per-edge insertion.
  Membership, edge contents and all analytics results are identical.

### Removed

- The pre-`DiskGraph` in-memory `Graph` type (271 lines, zero callers). It
  contradicted the pure-disk invariant.
- `CHALLENGE.md` and `MASTER_ROADMAP.md`, which described the pre-development
  problem statement and were superseded by the code, README and ROADMAP.

---

## Notes on performance figures

Earlier drafts advertised 708,561 ops/s (10M nodes), 1,019,526 ops/s
(`:memory:`) and a 1,439,681 ops/s edge burst. **Those configurations do not
reproduce those numbers** on the hardware used here; the measured values are
roughly 3–4× lower. The `:memory:` node figure _is_ reproducible once the
property payload is removed (~1.43M ops/s), which is likely how it was obtained.

Every figure now ships with its scenario and conditions — see the README
benchmark section and `ROADMAP.md`. A number without its conditions is not
accepted.

[Unreleased]: https://github.com/ysankpia/graphlite/compare/v1.0.0-rc.1...HEAD
[1.0.0-rc.1]: https://github.com/ysankpia/graphlite/releases/tag/v1.0.0-rc.1
