# Product Vision

NervusDB is SQLite for property graphs.

The product should let a Rust application open a local path, write graph data,
query nearby relationships, survive process failure, and reopen without running
a server.

## Primary User

The 0.1 user is a Rust application developer who needs embedded graph
persistence for local-first tools, dependency analysis, knowledge graphs,
ownership graphs, module graphs, or small relationship-heavy features.

## North Star Workflow

```text
open(path) -> write graph data -> query one-hop/two-hop relationships -> crash/reopen -> trust results
```

## Product Bias

- Correctness before language breadth.
- Rust API before SDK expansion.
- WAL/recovery proof before feature count.
- Mini-Cypher before full Cypher.
- Fast focused validation before historical gate matrices.
