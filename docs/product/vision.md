# Product Vision

NervusDB is SQLite for property graphs: a Rust-native embedded graph database
that opens a local path, stores graph data durably, and answers small graph
queries without running a server.

## North Star

Make the common local graph workflow boring:

```text
open(path) -> write graph data -> query nearby relationships -> survive crash -> reopen
```

The product wins when a Rust user can embed it the way they embed SQLite: no
daemon, no network dependency, predictable files, reliable recovery, and results
that are easy to validate.

## Primary User

The 0.1 user is a Rust application developer who needs local graph persistence:
developer tools, knowledge graphs, dependency graphs, lightweight graph analysis,
and local-first applications.

## Product Bias

- Prefer correctness and recovery over language breadth.
- Prefer Rust API stability over early SDK expansion.
- Prefer a small explainable query surface over full Cypher compatibility.
- Prefer deterministic local checks over large scheduled gate matrices.
- Prefer a finishable embedded 0.1 over proving every historical subsystem still
  deserves first-class product status.

## Non-North-Stars

NervusDB 0.1 is not a Neo4j replacement, a full Cypher standard implementation,
a vector database, a distributed service, or a multi-language SDK platform.
