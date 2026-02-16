# NervusDB

**Rust-native embedded property graph database — SQLite for graphs.**

Store nodes, relationships, and properties in a single local file. Query with Cypher.
No server, no setup, no dependencies.

[![CI](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)

## Highlights

- **Embedded** — open a path, get a graph database. No daemon, no network.
- **Cypher** — openCypher TCK 100% pass rate (3 897 / 3 897 scenarios).
- **Crash-safe** — WAL-based storage with single-writer + snapshot-reader transactions.
- **Multi-platform bindings** — Rust, Python (PyO3), Node.js (N-API), CLI.
- **Vector search** — built-in HNSW index for hybrid graph + vector queries.

## Quick Start

### Rust

```rust
use nervusdb::Db;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::open("/tmp/demo")?;
    db.execute("CREATE (n:Person {name: 'Alice'})", None)?;
    let rows = db.query("MATCH (n:Person) RETURN n.name", None)?;
    println!("{} row(s)", rows.len());
    Ok(())
}
```

### Python

```bash
pip install maturin
maturin develop -m nervusdb-pyo3/Cargo.toml
```

```python
import nervusdb

db = nervusdb.open("/tmp/demo-py")
db.execute_write("CREATE (n:Person {name: 'Alice'})")
for row in db.query_stream("MATCH (n:Person) RETURN n.name"):
    print(row)
db.close()
```

### Node.js

```bash
cargo build --manifest-path nervusdb-node/Cargo.toml --release
```

```typescript
const { Db } = require("./nervusdb-node");

const db = Db.open("/tmp/demo-node");
db.executeWrite("CREATE (n:Person {name: 'Alice'})");
const rows = db.query("MATCH (n:Person) RETURN n.name");
console.log(rows);
db.close();
```

### CLI

```bash
cargo run -p nervusdb-cli -- v2 write \
  --db /tmp/demo \
  --cypher "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})"

cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/demo \
  --cypher "MATCH (a)-[:KNOWS]->(b) RETURN a.name, b.name"
```

> Write statements (`CREATE`, `MERGE`, `DELETE`, `SET`) must use `execute_write` /
> `executeWrite` or a write transaction. Calling `query()` with a write statement
> raises an error.

## Architecture

```
nervusdb          — public API crate (Db::open / query / execute)
nervusdb-query    — Cypher parser, planner, executor
nervusdb-storage  — WAL, page store, segments, compaction
nervusdb-api      — GraphStore / GraphSnapshot traits
nervusdb-cli      — command-line interface
nervusdb-pyo3     — Python binding (PyO3)
nervusdb-node     — Node.js binding (N-API)
```

Storage layout: `<path>.ndb` (page store) + `<path>.wal` (redo log).
Transaction model: single writer + concurrent snapshot readers.

## Test Status

| Suite | Tests | Status |
|-------|-------|--------|
| openCypher TCK | 3 897 / 3 897 | 100% |
| Rust unit + integration | 153 | all green |
| Python (PyO3) | 138 | all green |
| Node.js (N-API) | 109 | all green |

## Documentation

- [User Guide](docs/user-guide.md) — API reference for all platforms
- [Architecture](docs/architecture.md) — storage, query pipeline, crate structure
- [Cypher Support](docs/cypher-support.md) — full compliance matrix
- [Roadmap](docs/ROADMAP.md) — current and planned phases
- [CLI Reference](docs/cli.md) — command-line usage
- [Binding Parity](docs/binding-parity.md) — cross-platform API coverage

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -W warnings
bash scripts/workspace_quick_test.sh
bash scripts/binding_smoke.sh
```

## License

[AGPL-3.0](LICENSE)
