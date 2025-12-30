# NervusDB v2

**Rust-embedded, crash-safe property graph database with Cypher subset – like SQLite for graphs.**

> A production-ready embedded graph database succeeding KùzuDB. Features: single-writer + snapshot readers, CSR storage, crash-safe WAL, and a curated Cypher subset for real-world workloads.

[![CI](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Quick Start

### Rust

```rust
use nervusdb_v2::Db;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open or create database
    let db = Db::open_paths(["/tmp/demo.ndb"]).unwrap();

    // Write data
    db.execute(
        "CREATE (a {name: 'Alice'})-[:1 {weight: 0.5}]->(b {name: 'Bob'})",
        None,
    ).unwrap();

    // Query data
    let results = db.query(
        "MATCH (a)-[:1]->(b) WHERE a.name = 'Alice' RETURN a, b LIMIT 10",
        None,
    ).unwrap();

    for record in results {
        println!("{:?}", record);
    }

    Ok(())
}
```

### CLI

```bash
# Create database and write data
nervusdb-cli v2 write --db /tmp/demo --cypher "CREATE (a {name: 'Alice'})-[:1]->(b {name: 'Bob'})"

# Query data (NDJSON output)
nervusdb-cli v2 query --db /tmp/demo --cypher "MATCH (a)-[:1]->(b) RETURN a, b"
```

## Supported Cypher Subset

| Category | Features |
|----------|----------|
| **Read** | `MATCH`, `RETURN`, `WHERE`, `ORDER BY`, `SKIP`, `LIMIT`, `DISTINCT`, `EXPLAIN`, Variable-length paths (`*1..5`) |
| **Write** | `CREATE`, `MERGE`, `DELETE`, `DETACH DELETE`, `SET` |
| **Patterns** | Single node, single-hop relationships, labels |
| **Aggregations** | `COLLECT`, `MIN`, `MAX`, `COUNT`, `SUM` |
| **Other** | `OPTIONAL MATCH`, `MERGE` |

See [docs/reference/cypher_support.md](docs/reference/cypher_support.md) for full details.

## Architecture

- **Storage**: Two-file format (`.ndb` page store + `.wal` redo log)
- **Transaction**: Single Writer + Snapshot Readers
- **Layout**: MemTable (delta) + Immutable CSR segments + Compaction
- **API**: `nervusdb-v2` crate with `GraphStore` trait

## Why NervusDB?

| Feature | NervusDB | KùzuDB | Neo4j |
|---------|----------|--------|-------|
| Embedded | Yes | Yes | No |
| Rust-native | Yes | Yes (Rust) | No |
| Crash-safe | Yes | Yes | Yes |
| Cypher subset | Yes | Yes | Full |
| Binary releases | Coming | No | No |
| Python bindings | Coming | Yes | Yes |

NervusDB is positioned as the spiritual successor to KùzuDB for the Rust ecosystem, with a focus on production-hardened storage and minimal dependencies.

## Installation

### From Source

```bash
cargo install nervusdb-cli
```

### From Crates.io

```bash
cargo add nervusdb-v2
```

### From GitHub Releases

Download prebuilt binaries from [Releases](https://github.com/LuQing-Studio/nervusdb/releases).

## Development

```bash
# Format check
cargo fmt --all -- --check

# Lint
cargo clippy --workspace --all-targets -- -W warnings

# Tests
cargo test --workspace

# Benchmark
./scripts/v2_bench.sh
```

## Resources

- [User Guide](docs/user-guide.md)
- [CLI Reference](docs/cli.md)
- [API Documentation](https://docs.rs/nervusdb-v2)
- [Design Documents](docs/design/)

## License

[Apache-2.0](LICENSE)
