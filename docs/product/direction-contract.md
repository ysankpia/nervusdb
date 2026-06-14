# Direction Contract

## Product Definition

NervusDB is SQLite for property graphs: a Rust-first embedded graph database with
local file storage, WAL-backed crash recovery, persistent graph data, and a
small query surface.

## Primary User

The 0.1 user is a Rust application developer who needs embedded graph
persistence for local-first tools, dependency analysis, knowledge graphs,
ownership graphs, module graphs, or small relationship-heavy features.

## North Star Workflow

```
open(path) -> write graph data -> query one-hop/two-hop relationships -> crash/reopen -> trust results
```

## In Scope (0.1)

- Rust embedded API.
- Local `.ndb` and `.wal` files.
- WAL-backed crash recovery.
- Node, edge, label, and property persistence.
- One writer and snapshot reads.
- Label scan.
- Neighbor traversal by relationship type.
- One-hop and two-hop traversal examples.
- Mini-Cypher for the supported 0.1 query surface.
- CLI smoke, debug, query, write, and import-style workflows.

## Out Of Scope Before 0.1

- Full openCypher compatibility.
- openCypher TCK pass rate as product success.
- Procedures, subqueries, and pattern comprehension.
- Stable Python, Node.js, or C APIs beyond maintenance.
- Vector/HNSW as a default product path.
- Advanced optimizer work outside Mini-Cypher.
- Nightly chaos, soak, fuzz, perf, TCK, release, or stability windows as default gates.

## Acceptance Criteria

0.1 is credible when:

- A Rust program can create and reopen a local graph database.
- Nodes, edges, labels, and properties persist across restart.
- Committed data survives kill/reopen recovery tests.
- One-hop and two-hop queries are documented and tested.
- Mini-Cypher results are deterministic for the supported surface.
- Ten realistic examples are documented and runnable.
- Rust API docs are clear enough to start without a server or non-Rust SDK.
- A manual large smoke can create 1,000,000 nodes and 5,000,000 edges without corruption on documented hardware.

## Product Bias

- Correctness before language breadth.
- Rust API before SDK expansion.
- WAL/recovery proof before feature count.
- Mini-Cypher before full Cypher.
- Fast focused validation before historical gate matrices.
