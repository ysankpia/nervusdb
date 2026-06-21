# Direction Contract

## Product Definition

NervusDB is SQLite for property graphs: a Rust-first embedded graph database
with local directory storage, Fjall-backed crash-safe persistence, durable graph
data, and a small query surface.

NervusDB owns the graph model and query/API contract. Fjall owns low-level KV
persistence. The product goal is not to reimplement a general storage engine.

## Primary User

The 0.1 user is a Rust application developer who needs embedded graph
persistence for local-first tools, dependency analysis, knowledge graphs,
ownership graphs, module graphs, or small relationship-heavy features.

## North Star Workflow

```text
open(directory) -> write graph data -> query one-hop/two-hop relationships -> crash/reopen -> trust results
```

## In Scope (0.1)

- **Rust embedded API** — `Db::open`, `WriteTxn`, `DbSnapshot`, `ReadTxn`, and
  Mini-Cypher.
- **Local database directory** — `Db::open(path)` opens a directory managed by
  the Fjall-backed storage layer.
- **Crash-safe committed persistence** — committed data remains readable after
  process failure and reopen.
- **Node / edge / label / relationship type / property persistence**.
- **One writer and snapshot readers** — write transactions are serialized;
  snapshots are immutable read views.
- **Label scans** — `MATCH (n:Label)` uses label storage, not only full graph
  filtering.
- **Neighbor traversal by relationship type** — directed one-hop and documented
  two-hop patterns.
- **Mini-Cypher core** — documented `MATCH`, `RETURN`, `CREATE`, basic `SET`,
  basic `DELETE`, `WHERE` equality, `LIMIT`, and `EXPLAIN`.
- **CLI** — local smoke/debug/query/write/import-style workflows.
- **Runnable 0.1 examples** — realistic local graph examples that stay inside
  the supported core.

## Current Storage Contract

0.1 uses a Fjall-backed logical keyspace model. The public contract is a local
database directory, not a `.ndb + .wal` file pair.

The current model has no independent edge ID and no parallel edges. Edge
identity is `(src_iid, rel_type_id, dst_iid)`. Labels and relationship types are
separate namespaces. Property keys are original strings with length framing, not
hashes.

## Explicitly Deleted Or Frozen Before 0.1

The following are not current 0.1 product work:

- self-built Pager/WAL/B+Tree/CSR storage direction
- `.ndb/.wal` as a public file-format promise
- HNSW/vector search
- Python, Node.js, and C bindings
- full openCypher
- procedures, subqueries, pattern comprehension
- `OPTIONAL MATCH`, broad aggregation, `ORDER BY/SKIP`, `WITH`, `UNION`,
  `UNWIND`, and variable-length paths as core gates
- property range indexes
- openCypher TCK pass rate as a product success metric
- fuzz, chaos, soak, perf, TCK, or release windows as default development gates

Deleted platform-era material is evidence only through git history. Promotion
back into core requires a new ADR and updates to product, architecture,
validation, and active plan docs.

## Acceptance Criteria

0.1 is credible when:

- A Rust program can create and reopen a local graph database directory.
- Nodes, edges, labels, relationship types, and properties persist across
  restart.
- Committed data survives process-level crash/reopen smoke.
- Label scans and one-hop/two-hop traversal are documented and tested.
- Mini-Cypher results are deterministic for the supported surface.
- CLI smoke/write/query workflows work against a local directory.
- Rust API docs are clear enough to start without a server or non-Rust SDK.
- A manual large smoke can create 1,000,000 nodes and 5,000,000 edges without
  corruption on documented hardware.

## Product Bias

- Correctness before language breadth.
- Rust API before SDK expansion.
- Reopen/crash proof before feature count.
- Mini-Cypher before full Cypher.
- Logical graph storage before custom storage-engine work.
- Fast focused validation before historical gate matrices.
