# GraphLite Python SDK

Official Python bindings for **GraphLite-RS**, the SQLite of Graph Databases.

## Features

- **Embedded & Zero-Configuration**: Operates on a single `.db` file with zero background servers.
- **Native Cypher 1.0 Support**: Execute `CREATE`, `MATCH`, `WHERE`, `RETURN`, `SET`, `DELETE`, `DETACH DELETE`, `ORDER BY`, `SKIP`, `LIMIT`, aggregates (`count`, `sum`, `avg`, `min`, `max`), and multi-hop paths (`*1..3`).
- **Enterprise Graph Algorithms**: Built-in `pagerank`, `weakly_connected_components`, `k_hop_subgraph`, `dijkstra`, `bfs`, and `has_cycle`.
- **4KB Paged Buffer Pool & STEAL Spilling**: Ultra-low memory footprint, bounded RAM usage, out-of-core traversal.
- **ACID & WAL**: Crash-resilient with page-level Write-Ahead Logging and checksum validation.
- **Explicit Batched Transactions**: Context manager `begin_transaction()` with single fsync commit guarantee.
- **Logical Dump**: Export entire database into replayable Cypher scripts via `dump_cypher()`.

## Installation

**Not published to PyPI yet** — build from source:

```bash
cd bindings/python
maturin develop
```

Requires [maturin](https://www.maturin.rs/) and the Rust toolchain.

## Quick Start

```python
import graphlite

# 1. Open or create database (pool_size in frames, default 1024 = 4MB)
db = graphlite.GraphLite.open("mydb.db", pool_size=1024)

# 2. Execute Cypher CREATE statements
db.execute("CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob', age: 32});")

# 3. Query with Cypher, returns list of dicts (supports ORDER BY, SKIP, LIMIT, Aggregates)
results = db.query("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;")
for row in results:
    print(row)
    # {'a.name': 'Alice', 'b.name': 'Bob', 'b.age': 32}

# 4. Cypher Updates and Deletions
db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 29, a:Engineer;")
db.execute("MATCH (b:Person {name: 'Bob'}) DETACH DELETE b;")

# 5. Graph Analytics & Algorithms
ranks = db.pagerank(0.85, 100, 1e-6)          # [(node_id, score), ...]
components = db.weakly_connected_components() # [[node_id, ...], ...]
sub = db.k_hop_subgraph(1, 2, "outgoing", "KNOWS")
res = db.dijkstra(1, 2, "KNOWS")
if res:
    cost, path = res
    print(f"Dijkstra Cost: {cost}, Path: {path}")

# 6. Batched Transactions (Single fsync commit)
with db.begin_transaction() as tx:
    for i in range(10000):
        tx.add_node(["Bulk"], {"idx": i, "name": f"bulk-{i}"})

# 7. Schema introspection & Cypher Dump export
print("Labels:", db.labels(), "Edge types:", db.edge_types())
with open("backup.cypher", "w") as f:
    f.write(db.dump_cypher())

# 8. Buffer pool stats & Checkpoint
print(db.stats())
db.checkpoint()
```
