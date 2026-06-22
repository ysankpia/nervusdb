# Embedded Graph Benchmark Research Plan

## Status

Research benchmark specification. This is not a NervusDB feature roadmap and is
not a 0.0.6 plan.

NervusDB `v0.0.5` remains the current stability-freeze line. This document exists
to answer a separate question: if we compare NervusDB with SQLite used as a graph
store and with practical embedded graph databases, what should be measured, and
what would constitute honest evidence?

## Decision

The first real comparison set should be:

| System | Role | Include In First Harness | Reason |
|---|---:|---:|---|
| NervusDB 0.0.5 | Subject under test | yes | Current released embedded property graph crate |
| SQLite graph schema | Relational baseline | yes | Strong single-file embedded baseline and the right comparison for "SQLite for graph" |
| SQLite graph schema with materialized graph indexes | Optimized relational baseline | yes | Tests whether a carefully designed SQLite schema already solves enough of the problem |
| Kuzu | Embedded property graph baseline | yes | Closest graph-native embedded competitor |
| DuckDB | Analytical relational baseline | later/manual | Useful for bulk analytical joins, not a graph database |
| SurrealDB embedded | Multi-model baseline | later/manual | Embedded-capable, but not property-graph-first |
| Neo4j embedded / ArcadeDB embedded | JVM graph baselines | later/manual | Real graph systems, but JVM weight changes the deployment class |
| Oxigraph / Jena TDB | RDF/SPARQL baselines | later/manual | Graph databases, but RDF triples are a different model from LPG |

Do not start by comparing every database we can name. The first benchmark should
compare the systems a user would realistically choose for a local embedded
property graph:

1. NervusDB.
2. SQLite with a graph schema.
3. SQLite with graph-native materialized indexes.
4. Kuzu.

If NervusDB loses to SQLite on the core local graph workloads, that is useful
evidence. If it beats a lazy SQLite schema but loses to a careful SQLite schema,
that is also useful evidence. If it only wins against a deliberately crippled
SQLite schema, the benchmark is worthless.

## External Facts Used

- SQLite stores the complete main database state in a single main database file,
  while rollback journal or WAL files can exist during transaction processing:
  <https://www.sqlite.org/fileformat.html>
- SQLite `WITHOUT ROWID` tables use the declared primary key as the B-tree key
  rather than adding the usual hidden rowid table shape:
  <https://sqlite.org/withoutrowid.html>
- SQLite generated columns can participate in indexes:
  <https://sqlite.org/gencol.html>
- Kuzu describes itself as an embedded graph database built for query speed and
  scalability:
  <https://kuzudb.github.io/docs>
- Kuzu recommends `COPY FROM` for large CSV imports:
  <https://kuzudb.github.io/docs/import/csv/>
- DuckDB is an embedded/in-process analytical SQL database, but it is
  table-oriented rather than graph-native:
  <https://duckdb.org/why_duckdb.html>
- SurrealDB can run embedded inside Rust applications:
  <https://surrealdb.com/docs/build/embedding/by-language/rust>
- Neo4j has an embedded Java mode, which makes it a different deployment class
  from Rust/C/C++ embedded libraries:
  <https://neo4j.com/docs/java-reference/current/java-embedded/>

## Real Goal

The benchmark is not "prove NervusDB is fast." The goal is to decide whether
NervusDB has a real reason to exist for embedded local property graph use.

The benchmark must answer these questions:

1. Is NervusDB competitive with a serious SQLite graph schema?
2. Does NervusDB provide simpler graph operations than SQLite without losing
   unacceptable performance?
3. How far is NervusDB from a graph-native embedded engine such as Kuzu?
4. Which weakness matters first: write throughput, point lookup, one-hop
   traversal, two-hop traversal, delete cleanup, crash safety, or operational
   footprint?
5. Would a future C single-file graph engine actually improve the product, or
   would it just spend months rebuilding SQLite/Fjall mechanics?

## Non-Goals

- Do not benchmark full Cypher.
- Do not benchmark variable-length paths.
- Do not benchmark shortest paths or graph algorithms.
- Do not benchmark vector search.
- Do not benchmark network servers.
- Do not compare unsafe modes against durable modes.
- Do not use benchmark work to reopen NervusDB feature development unless a
  downstream project hits a concrete blocker.

## Comparison Classes

### Class A: Same Deployment Class

These are the fair first-order comparisons.

| System | Storage Shape | Query Shape | Fairness Notes |
|---|---|---|---|
| NervusDB | Local directory, Fjall-backed LSM | Rust API + Mini-Cypher | Subject under test |
| SQLite simple graph | Single SQLite file plus transient WAL/journal | SQL joins | Strong general embedded baseline |
| SQLite materialized graph | Single SQLite file plus transient WAL/journal | SQL with graph-specific indexes | Tests how far SQLite can be pushed before writing a graph engine |
| Kuzu | Local embedded database directory/files | Cypher | Closest embedded graph-native baseline |

### Class B: Useful But Not Same Shape

These may be measured, but not placed in the primary leaderboard.

| System | Why It Is Different |
|---|---|
| DuckDB | Excellent embedded analytical SQL engine, but graph traversal is not its native target |
| SurrealDB embedded | Multi-model database; graph semantics and local deployment differ from NervusDB |
| Neo4j embedded | JVM deployment class and much broader feature surface |
| ArcadeDB embedded | JVM multi-model graph/document/vector engine |
| Oxigraph / Jena TDB | RDF/SPARQL model, not labeled property graph |

## Durability Profiles

Every system must be run in named durability profiles. Never mix them in one
chart.

### Safe Profile

This is the default comparison.

| System | Required Setting |
|---|---|
| NervusDB | Default commit path; no unsafe/buffered mode |
| SQLite | `PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;` or explicitly documented rollback journal FULL |
| Kuzu | Default durable local database mode |

### Bulk Profile

This profile measures how fast each system loads data when used correctly.

| System | Required Setting |
|---|---|
| NervusDB | One large write transaction |
| SQLite | One explicit transaction; prepared statements; indexes created before or after load must be reported |
| Kuzu | `COPY FROM` for large CSV load, because Kuzu documentation recommends it |

### Unsafe Profile

Unsafe or relaxed modes are optional and must be reported separately.

Examples:

- SQLite `synchronous=NORMAL` or `OFF`.
- Any database mode that admits recent data loss after power failure.

Unsafe numbers are not release evidence. They are only engineering context.

## Data Sets

Use deterministic generation. Each generated data set must have a manifest:

```json
{
  "seed": 1,
  "node_count": 100000,
  "edge_count": 500000,
  "labels": ["BenchNode"],
  "relationships": ["LINK"],
  "properties": ["name", "kind", "status", "chapter"],
  "shape": "uniform_degree"
}
```

### S: Local Smoke

```text
nodes=10,000
degree=5
edges=50,000
lookup_iters=1,000
traversal_iters=1,000
```

Purpose: fast local correctness and harness sanity.

### M: Real Baseline

```text
nodes=100,000
degree=5
edges=500,000
lookup_iters=10,000
traversal_iters=10,000
```

Purpose: default comparison line.

### L: Manual Stress

```text
nodes=1,000,000
degree=5
edges=5,000,000
lookup_iters=20,000
traversal_iters=20,000
```

Purpose: manual evidence only. Do not make this part of normal CI.

## Graph Shapes

### Uniform Degree

Each node has `degree` outgoing edges:

```text
src = i
dst = (i * 1315423911 + j * 2654435761 + seed) % node_count
rel = LINK
```

This avoids a pure ring and gives enough scatter to test adjacency locality.

### Hub Skew

One percent of nodes receive a large fraction of incoming edges.

Purpose: test high-degree nodes, hot traversal, and index fanout behavior.

### Agent Memory Shape

Use the realistic local-agent model:

```text
Character
Event
Fact
KNOWS
APPEARS_IN
CAUSES
MENTIONS
```

Purpose: prove the database helps the intended downstream use case, not just a
synthetic graph.

## Common Logical Operations

Every system must implement these operations with its idiomatic API/query
language.

| Operation | Meaning |
|---|---|
| load_nodes | Insert nodes with label and scalar properties |
| load_edges | Insert directed relationships |
| reopen_verify | Close, reopen, verify counts and sample checksums |
| lookup_by_property | Find `BenchNode` by `name = 'node_N'` |
| one_hop_hot | Repeated outgoing traversal from one fixed node |
| one_hop_cold | Outgoing traversal across deterministic sampled nodes |
| incoming_cold | Incoming traversal across deterministic sampled nodes |
| two_hop | Bounded two-hop traversal from sampled nodes |
| update_property | Change `status` on sampled nodes and re-query |
| detach_delete | Delete sampled nodes and verify incident edges disappear |
| disk_footprint | Total bytes and file count after checkpoint/close |

## Metric Schema

Each run must emit one JSON object as the last line.

```json
{
  "benchmark_version": 1,
  "system": "nervusdb",
  "system_version": "0.0.5",
  "profile": "safe",
  "dataset": "M",
  "shape": "uniform_degree",
  "seed": 1,
  "nodes": 100000,
  "edges": 500000,
  "hardware": "Apple M-series / OS / filesystem",
  "git_commit": "optional",
  "load_nodes_ms": 0.0,
  "load_edges_ms": 0.0,
  "commit_ms": 0.0,
  "reopen_verify_ms": 0.0,
  "lookup_p50_us": 0.0,
  "lookup_p95_us": 0.0,
  "lookup_p99_us": 0.0,
  "one_hop_hot_edges_per_sec": 0.0,
  "one_hop_cold_edges_per_sec": 0.0,
  "incoming_cold_edges_per_sec": 0.0,
  "two_hop_paths_per_sec": 0.0,
  "update_p99_us": 0.0,
  "detach_delete_p99_us": 0.0,
  "db_bytes": 0,
  "db_file_count": 0,
  "correctness_hash": "hex",
  "notes": []
}
```

The benchmark is invalid if it does not emit correctness evidence.

## Correctness Hash

Performance without correctness is noise. Each system must produce a stable hash
from observable query results:

```text
node_count
edge_count
lookup(node_0)
lookup(node_last)
out_neighbors(sample_0)
out_neighbors(sample_mid)
in_neighbors(sample_last)
two_hop_count(sample_0)
post_update_lookup(status='updated')
post_delete_absence_check
```

The hash input must be sorted where result order is not part of the query
contract.

## SQLite Baseline Schema

There should be two SQLite baselines.

### SQLite Simple

This is what a competent user would write without building a full graph layer.

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;

CREATE TABLE nodes (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT,
  status TEXT,
  chapter INTEGER
);

CREATE INDEX nodes_label_name_idx ON nodes(label, name);
CREATE INDEX nodes_label_status_idx ON nodes(label, status);

CREATE TABLE edges (
  src INTEGER NOT NULL,
  rel TEXT NOT NULL,
  dst INTEGER NOT NULL,
  PRIMARY KEY (src, rel, dst),
  FOREIGN KEY (src) REFERENCES nodes(id) ON DELETE CASCADE,
  FOREIGN KEY (dst) REFERENCES nodes(id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE INDEX edges_dst_rel_src_idx ON edges(dst, rel, src);
```

Representative queries:

```sql
SELECT id FROM nodes WHERE label = 'BenchNode' AND name = ? LIMIT 1;
SELECT dst FROM edges WHERE src = ? AND rel = 'LINK';
SELECT src FROM edges WHERE dst = ? AND rel = 'LINK';
SELECT e2.dst
FROM edges e1
JOIN edges e2 ON e2.src = e1.dst AND e2.rel = 'LINK'
WHERE e1.src = ? AND e1.rel = 'LINK';
```

### SQLite Materialized Graph

This is SQLite used as the substrate for a small graph engine.

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;

CREATE TABLE nodes (
  id INTEGER PRIMARY KEY,
  flags INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE labels (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE reltypes (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE node_labels (
  node_id INTEGER NOT NULL,
  label_id INTEGER NOT NULL,
  PRIMARY KEY (node_id, label_id)
) WITHOUT ROWID;

CREATE INDEX node_labels_by_label_idx ON node_labels(label_id, node_id);

CREATE TABLE node_props (
  node_id INTEGER NOT NULL,
  key TEXT NOT NULL,
  value_type INTEGER NOT NULL,
  value_text TEXT,
  value_int INTEGER,
  PRIMARY KEY (node_id, key)
) WITHOUT ROWID;

CREATE TABLE edges (
  src INTEGER NOT NULL,
  rel INTEGER NOT NULL,
  dst INTEGER NOT NULL,
  PRIMARY KEY (src, rel, dst)
) WITHOUT ROWID;

CREATE INDEX edges_in_idx ON edges(dst, rel, src);

CREATE TABLE idx_node_prop (
  label_id INTEGER NOT NULL,
  key TEXT NOT NULL,
  value_type INTEGER NOT NULL,
  value_text TEXT,
  value_int INTEGER,
  node_id INTEGER NOT NULL,
  PRIMARY KEY (label_id, key, value_type, value_text, value_int, node_id)
) WITHOUT ROWID;
```

This baseline is intentionally strong. It asks whether a graph-specific storage
layout on top of SQLite already beats NervusDB for the important local use cases.

## Kuzu Baseline Schema

Use a schema that maps directly to the benchmark graph.

```cypher
CREATE NODE TABLE BenchNode(
  id INT64,
  name STRING,
  kind STRING,
  status STRING,
  chapter INT64,
  PRIMARY KEY(id)
);

CREATE REL TABLE Link(
  FROM BenchNode TO BenchNode
);
```

Representative queries:

```cypher
MATCH (n:BenchNode {name: $name}) RETURN n.id LIMIT 1;
MATCH (n:BenchNode)-[:Link]->(m:BenchNode) WHERE n.id = $id RETURN m.id;
MATCH (m:BenchNode)-[:Link]->(n:BenchNode) WHERE n.id = $id RETURN m.id;
MATCH (a:BenchNode)-[:Link]->(b:BenchNode)-[:Link]->(c:BenchNode)
WHERE a.id = $id
RETURN c.id;
```

Bulk load should use `COPY FROM` when the run is measuring bulk ingestion, and
ordinary statements when the run is measuring application-style writes. Do not
mix those two into the same number.

## NervusDB Baseline

Use the released public `nervusdb` crate and public APIs only. Do not benchmark
private storage functions.

Representative Mini-Cypher queries:

```cypher
MATCH (n:BenchNode) WHERE n.name = 'node_99999' RETURN n LIMIT 1
MATCH (n:BenchNode)-[:LINK]->(m:BenchNode) WHERE n.name = 'node_0' RETURN m
```

Representative Rust API operations:

```text
Db::open(path)
begin_write()
create_node()
create_edge()
set_node_property()
snapshot()
nodes_with_label_and_property()
neighbors()
incoming_neighbors()
```

NervusDB numbers must include:

- current `scripts/core_bench.sh` output
- same workload through the cross-db harness
- `nervusdb v2 fsck --db <path> --json` result for post-run correctness

## Reporting

The final report should have three tables, not one giant misleading table.

### Table 1: Product Fit

| System | Embedded | Single Main File | Property Graph | Query Language | Rust-Friendly | Operational Footprint |
|---|---:|---:|---:|---|---:|---|
| NervusDB | yes | no, directory | yes | Rust API + Mini-Cypher | yes | low |
| SQLite simple | yes | yes | manual schema | SQL | yes through bindings | very low |
| SQLite materialized | yes | yes | manual graph layer | SQL | yes through bindings | low |
| Kuzu | yes | database directory/files | yes | Cypher | yes if binding is acceptable | medium |

### Table 2: Correctness And Operations

| System | Reopen Safe | Crash Test | Fsck/Repair | Detach Delete | Edge Integrity | Notes |
|---|---:|---:|---:|---:|---:|---|

### Table 3: Performance

| System | Load Edges/s | Lookup P99 | One-Hop Cold Edges/s | Two-Hop Paths/s | Update P99 | Delete P99 | Bytes/Edge |
|---|---:|---:|---:|---:|---:|---:|---:|

## Execution Phases

### Phase 1: NervusDB vs SQLite

Build the first harness with only:

- NervusDB.
- SQLite simple.
- SQLite materialized.

Current harness entry:

```bash
bash scripts/cross_db_bench.sh --small
bash scripts/cross_db_bench.sh --medium
bash scripts/cross_db_bench.sh --system nervusdb --nodes 100000 --degree 5 --iters 10000
```

The script writes per-system JSON and an NDJSON summary under:

```text
artifacts/cross-db-bench/
```

Acceptance:

- Same generated dataset.
- Same correctness hash.
- Same durability profile.
- Same JSON schema.
- M-size run completes locally.

Initial small-run evidence on 2026-06-22:

```text
command: bash scripts/cross_db_bench.sh --small
dataset: nodes=1,000 degree=5 edges=5,000 iters=200 mutation_iters=20
systems: nervusdb, sqlite-simple, sqlite-materialized
correctness_hash: 1309e1269351c870 for all three systems
summary: artifacts/cross-db-bench/cross-db-bench-small-20260622-102358.ndjson
```

Initial medium-run evidence on 2026-06-22:

```text
command: bash scripts/cross_db_bench.sh --medium
dataset: nodes=100,000 degree=5 edges=500,000 iters=10,000 mutation_iters=100
systems: nervusdb, sqlite-simple, sqlite-materialized
correctness_hash: d4b70801ad0bb15b for all three systems
summary: artifacts/cross-db-bench/cross-db-bench-medium-20260622-103209.ndjson
```

| System | Commit ms | Reopen ms | Lookup P99 us | One-hop cold edges/s | Two-hop paths/s | Update P99 us | Detach delete P99 us | Disk bytes | Files |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| NervusDB | 8,018.821 | 2,913.563 | 2.000 | 1,043,008 | 1,107,359 | 85,827.125 | 82,955.750 | 84,595,889 | 31 |
| SQLite simple | 81.255 | 13.711 | 1.500 | 2,537,599 | 7,309,274 | 436.667 | 245.750 | 29,319,168 | 1 |
| SQLite materialized | 100.171 | 10.034 | 1.334 | 2,850,262 | 7,737,163 | 2,713.292 | 7,807.959 | 38,244,352 | 1 |

Interpretation: for this workload, SQLite is not merely a convenient baseline;
it is a serious embedded storage competitor. NervusDB's property lookup is in
the same microsecond class, but commit, reopen, mutation latency, traversal
throughput, and disk footprint are materially worse. The next investigation
should focus on those concrete gaps before considering a C storage rewrite.

This phase answers the most important question: does a custom embedded graph
database beat a serious SQLite design enough to justify its existence?

### Phase 2: Add Kuzu

Add Kuzu after Phase 1 is stable.

Acceptance:

- Kuzu uses its idiomatic node/relationship tables.
- Bulk mode uses `COPY FROM`.
- Application-write mode is separate from bulk mode.
- Kuzu result hash matches the common correctness hash.

This phase answers the second important question: how far is NervusDB from a
graph-native embedded engine?

### Phase 3: Optional Wider Landscape

Add DuckDB, SurrealDB embedded, Neo4j embedded, ArcadeDB, Oxigraph, or Jena only
if a downstream project would realistically choose that system.

These results belong in an appendix. They should not drive NervusDB's core
direction unless the deployment model is actually comparable.

## Invalid Benchmarks

Reject a result if any of these are true:

- SQLite runs unsafe while NervusDB runs durable.
- Kuzu uses bulk `COPY FROM` while other systems use row-by-row application
  writes, and the chart labels them as the same workload.
- The benchmark does not reopen the database.
- There is no correctness hash.
- Query result order is assumed without sorting where order is not guaranteed.
- The SQLite schema is deliberately naive and no strong SQLite baseline is
  included.
- The data set is too small to cross page/cache boundaries.
- The report hides disk footprint or file count.

## Interpretation Rules

If SQLite materialized wins most workloads, the correct conclusion is not
"rewrite NervusDB in C." The correct conclusion is that SQLite may be the right
storage substrate for a future single-file graph engine.

If Kuzu wins by a large margin on traversal and Cypher workloads, the correct
conclusion is that NervusDB should not chase full graph analytics before it has
a real downstream use case.

If NervusDB wins on simple local agent-memory workloads, the correct conclusion
is to use it and stop building database infrastructure until a real blocker
appears.

If NervusDB loses only on bulk import but wins or ties on lookup/traversal for
the target workload, the correct fix is likely a bulk-load path, not a storage
rewrite.

If NervusDB loses on point lookup after 0.0.4's property index, the first bug to
investigate is index routing or benchmark fairness, not a C rewrite.

## C Single-File Graph Engine Implication

A future C project inspired by SQLite should not begin until this benchmark says
SQLite-as-graph is either:

1. strong enough that using SQLite as the storage substrate is better than
   writing a pager/B-tree from scratch, or
2. measurably blocked by graph-specific layout needs that SQLite cannot satisfy
   without too much complexity.

The benchmark decision gate is:

```text
Do not write pager.c until SQLite materialized has been measured.
Do not write btree.c until SQLite materialized has been measured.
Do not write a query language until storage correctness and traversal numbers
exist.
```

That is the non-romantic version of "SQLite for graph." First prove where SQLite
stops being enough.
