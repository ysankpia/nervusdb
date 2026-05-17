# NervusDB

**Rust-first embedded property graph database — SQLite for graphs.**

Open a local path, write graph data, query nearby relationships, survive a
crash, and reopen. No server. No network service. No platform ceremony.

[![CI](https://github.com/ysankpia/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/ysankpia/nervusdb/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)

> [中文](README_CN.md)

## Current Focus

NervusDB is being cut back to a finishable 0.1 line:

- Rust embedded API
- local file storage
- WAL-backed crash recovery
- node / edge / label / property persistence
- label scans and neighbor traversal
- a small Mini-Cypher surface
- CLI support for local debug, import smoke, query, and write workflows

Full Cypher compatibility, multi-language SDK stabilization, HNSW/vector search,
cross-binding parity gates, and industrial nightly gates are historical or
experimental. They are not the 0.1 success criteria.

## Quick Start

### Rust

```rust
use nervusdb::Db;
use nervusdb_query::{prepare, query_collect, Params};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::open("/tmp/nervusdb-demo")?;

    let snapshot = db.snapshot();
    let create = prepare("CREATE (n:Person {name: 'Alice'})")?;
    let mut txn = db.begin_write();
    create.execute_write(&snapshot, &mut txn, &Params::new())?;
    txn.commit()?;

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) RETURN n.name LIMIT 10",
        &Params::new(),
    )?;
    println!("{rows:?}");
    Ok(())
}
```

### CLI

```bash
cargo run -p nervusdb-cli -- v2 write \
  --db /tmp/nervusdb-demo \
  --cypher "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})"

cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/nervusdb-demo \
  --cypher "MATCH (a)-[:KNOWS]->(b) RETURN a.name, b.name LIMIT 10"
```

Write statements must use `prepare(...).execute_write(...)` or the CLI write
path. Read queries should stay within the documented Mini-Cypher surface for
0.1.

## Architecture

```text
nervusdb          public Rust facade
nervusdb-api      storage/query boundary traits
nervusdb-storage  page store, WAL, snapshots, recovery, indexes
nervusdb-query    Mini-Cypher parser/planner/executor path plus frozen history
nervusdb-cli      local debug/import/query/write tool
```

Experimental or historical areas remain in the repository but are not the
default product path: Python, Node.js, C bindings, full openCypher TCK, vector
search, parity gates, perf/chaos/soak/fuzz matrices, and release windows.

## Development

Default local check:

```bash
bash scripts/check.sh
```

This runs formatting, core-crate clippy, and the Mini-Cypher core quick test.
Full historical tests live behind `bash scripts/workspace_full_test.sh`.
Area-specific scripts for TCK, bindings, perf, fuzz, chaos, soak, and stability
are manual signals only.

## Documentation

- [Documentation Index](docs/index.md)
- [Product Vision](docs/product/vision.md)
- [0.1 Scope](docs/product/scope-0.1.md)
- [Architecture Overview](docs/architecture/overview.md)
- [Testing Strategy](docs/engineering/testing-strategy.md)
- [Mini-Cypher Reference](docs/reference/mini-cypher.md)
- [Local Validation](docs/runbooks/local-validation.md)

## License

[AGPL-3.0](LICENSE)
