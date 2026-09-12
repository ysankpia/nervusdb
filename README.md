# GraphLite-RS

[![CI](https://github.com/ysankpia/graphlite/actions/workflows/ci.yml/badge.svg)](https://github.com/ysankpia/graphlite/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.98%2B-orange.svg)](https://www.rust-lang.org)

An embedded, single-file **property graph database**: the SQLite model applied to
graphs. Two files on disk, no server, no daemon, and resident memory bounded by a
configurable buffer pool rather than by dataset size.

```rust
use graphlite::{GraphLite, GraphError};

fn main() -> Result<(), GraphError> {
    let db = GraphLite::open("mydb.db")?;

    db.execute(
        "CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob'})",
    )?;

    let rows = db.query_cypher(
        "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 20 RETURN a.name, b.name",
    )?;
    for row in rows.rows {
        println!("{:?}", row.values);
    }

    db.checkpoint()?;
    Ok(())
}
```

## Status

**`v1.1.0` — stable.** The engine, Cypher surface, analytics and safety guarantees
are implemented and covered by 187 tests. The on-disk format is frozen at version 4
and unchanged by 1.1.0; see `FORMAT.md`. Several known gaps remain — read
[Known limitations](ROADMAP.md#next-planned) before considering production use.

## Features

- **Embedded and single-file.** `{path}` plus a page-level WAL `{path}.wal`. No
  sidecar files, no external services.
- **Bounded memory.** A 4KB page buffer pool with O(1) LRU eviction. Resident
  memory is set by the pool, not by the dataset: verified on a 4.34 GB graph
  (68.9 M edges) inside a 1 GiB pool, and bounded by construction past that.
- **Disk-native adjacency.** Fixed 32-byte node and 64-byte edge records with
  O(1) physical addressing and double-cyclic edge chains. Finding neighbours
  never scans.
- **Compact property storage.** Slotted pages pack several payloads per page;
  40,000 entities with ~150-byte payloads occupy 7.9MB.
- **ACID.** Explicit transactions, single-fsync group commit, STEAL spilling for
  transactions larger than the pool, crash recovery, exact rollback with a
  byte-identical main file. A failed write statement leaves nothing behind, and the
  transaction action queue is bounded (≈502 bytes/node action, 128 bytes/edge action,
  capped at 4M actions) so memory stays set by configuration rather than by input.
- **Cypher.** `CREATE`, `MATCH` (multi-pattern), `MERGE` (idempotent write),
  `UNWIND` (batch ingestion in one statement), `WHERE`, `SET`,
  `DELETE` / `DETACH DELETE`, `ORDER BY`, `SKIP`, `LIMIT`, aggregates,
  variable-length paths, and `EXPLAIN`.

  ```cypher
  UNWIND [1, 2, 3] AS i CREATE (n:Num {v: i})   -- three nodes, one statement
  MERGE (u:User {name: 'alice'})                -- creates, then reuses
  ```

- **Self-consistent reads.** `GraphLite::read_snapshot()` holds one state across a
  multi-step traversal, so reading a node and then its edges cannot observe a
  concurrent delete in between. Snapshots block writers while they live; more
  concurrency needs versioned page visibility ([ROADMAP](ROADMAP.md) item 1).
- **Analytics.** BFS, Dijkstra, cycle detection, PageRank, weakly connected
  components, K-hop subgraphs — all over disk cursors.
- **Production safety.** An exclusive open lock, a structural integrity check, and
  error-preserving read accessors. See
  [Architecture §11](docs/architecture.md#11-production-safety).
- **SDKs.** Python (PyO3) and Node.js (NAPI-RS), with transactions, batch writes
  and logical dump. The library is the interface: there is no separate CLI or GUI
  to keep in sync.

## Documentation

| Document                                     | Contents                                                        |
| -------------------------------------------- | --------------------------------------------------------------- |
| [docs/architecture.md](docs/architecture.md) | How it works: paging, WAL, slotted pages, weave, Cypher, safety |
| [docs/benchmarks.md](docs/benchmarks.md)     | Measured throughput, with conditions and a correction notice    |
| [docs/testing.md](docs/testing.md)           | The suite, and the adversarial style it follows                 |
| [CHANGELOG.md](CHANGELOG.md)                 | Version history — read before upgrading                         |
| [ROADMAP.md](ROADMAP.md)                     | Done, planned, and explicitly out of scope                      |
| [AGENTS.md](AGENTS.md)                       | Architecture invariants; read before changing code              |
| [CONTRIBUTING.md](CONTRIBUTING.md)           | Contribution policy: **issues only, no external code**          |
| [SECURITY.md](SECURITY.md)                   | Private vulnerability reporting                                 |
| [LICENSING.md](LICENSING.md)                 | Dual licensing (AGPL-3.0 + commercial)                          |

## Install

```toml
[dependencies]
graphlite-rs = "1.0.0-rc.3"
```

The Python and Node.js SDKs are **not published to PyPI or npm yet**. Build them
from source (see [bindings/](bindings/)).

## Quick start

### Python

```python
import graphlite

db = graphlite.GraphLite.open("novel.db")
with db.begin_transaction() as tx:
    lin = tx.add_node(["Character"], {"name": "林渊"})
    su = tx.add_node(["Character"], {"name": "苏晴"})
    tx.add_edge(lin, su, "KNOWS", {"since": 2020}, 1.0)

rows = db.query_cypher(
    "MATCH (a:Character)-[:KNOWS]->(b) RETURN a.name AS a, b.name AS b"
)
print(rows)

db.dump_cypher("backup.cypher")   # logical export; replayable into a fresh file
db.backup("snapshot.db")          # consistent online copy
```

### Node.js

```javascript
import { GraphLite } from "graphlite-node";

const db = GraphLite.open("novel.db");
const tx = db.beginTransaction();
const lin = tx.addNode(["Character"], { name: "林渊" });
const su = tx.addNode(["Character"], { name: "苏晴" });
tx.addEdge(lin, su, "KNOWS", { since: 2020 }, 1.0);
tx.commit();
```

### Choosing a memory budget

```rust
use graphlite::{GraphLite, SMALL_POOL_FRAMES, DEFAULT_BUFFER_POOL_FRAMES, LARGE_POOL_FRAMES};

let db = GraphLite::open_with_pool_mb("mydb.db", 16)?;            // 16 MB
let db = GraphLite::open_with_pool_size("mydb.db", SMALL_POOL_FRAMES)?;   // 1 MB
let db = GraphLite::open("mydb.db")?;  // 4 MB default (DEFAULT_BUFFER_POOL_FRAMES)
```

### Bulk writes

Wrap bulk ingestion in one transaction. The whole batch then costs a single
`fsync` instead of one per write, and the write path can weave edge chains in a
batch instead of page-hopping per edge:

```rust
db.with_transaction(|tx| {
    for i in 0..100_000 {
        tx.add_node(
            HashSet::from(["Bulk".to_string()]),
            HashMap::from([("idx".to_string(), Value::from(i))]),
        )?;
    }
    Ok(())
})?;
```

**Chunk very large batches.** A single transaction holds its whole action list in
memory; a 4M-edge transaction reaches ~1.1GB RSS, which a larger buffer pool then
competes with. See [docs/benchmarks.md](docs/benchmarks.md) for the measurement.

### Integrity and error handling

```rust
let db = GraphLite::open("mydb.db")?;   // exclusive lock; a second handle gets DatabaseLocked
db.verify()?;                           // structural self-check (read-only, never repairs)

// get_node folds storage errors into None (documented as lossy);
// try_get_node preserves them and reserves Ok(None) for genuine absence.
let node = db.try_get_node(42)?;
```

## Project layout

```text
src/
  lib.rs            Public facade: GraphLite, Transaction, ACID coordination
  page.rs           4KB pages, NodeRecord, EdgeRecord, SlottedPropPage, PropCodec
  buffer.rs         DiskManager and the LRU buffer pool (page latches, STEAL spill)
  disk_graph.rs     Direct addressing, disk adjacency, freelists, page iterators
  storage.rs        Page-level WAL, CRC verification, checkpoint, recovery
  index.rs          Label and property secondary indexes
  integrity.rs      Read-only structural integrity check
  lock.rs           Process-level exclusive open lock
  sync_ext.rs       Poison-recovering lock accessors
  algo.rs           BFS, Dijkstra, cycles, PageRank, WCC, K-Hop
  cypher/           Lexer, recursive-descent parser, executor
  query.rs          Chainable typed query DSL
  graph.rs          Domain models: Node, Edge, Value, Direction, GraphError
bindings/
  python/           PyO3 SDK          nodejs/   NAPI-RS SDK
tests/              13 suites, 123 cases
benches/            Reproducible throughput, pool-size and memory probes
```

## Development

`main` is protected: changes go through a pull request, including the
maintainer's. Run the full gate before opening one:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

External code contributions are **not accepted** — a licensing decision, not a
judgement on quality; see [CONTRIBUTING.md](CONTRIBUTING.md). Issues, reproducible
bug reports and corrections to the documented numbers are welcome.

## License

Dual-licensed, your choice of:

1. **AGPL-3.0-only** — free. See [LICENSE](LICENSE).
2. **Commercial licence** — for closed-source embedding or a modified network
   service whose changes you do not want to publish. Contact
   **luhuizhx@gmail.com**.

AGPL restricts _closed distribution_, not commercial use: internal use, running it
as-is as a service, and selling support are all fine for free. See
[LICENSING.md](LICENSING.md) for the full breakdown.
