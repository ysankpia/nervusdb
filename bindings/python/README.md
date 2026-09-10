# GraphLite Python SDK

Official Python bindings for **GraphLite-RS**, the SQLite of Graph Databases.

## Features

- **Embedded & Zero-Configuration**: Operates on a single `.db` file.
- **Native Cypher Support**: Execute `CREATE`, `MATCH`, `WHERE`, `RETURN`, `LIMIT` directly.
- **4KB Paged Buffer Pool**: Ultra-low memory footprint, bounded RAM usage.
- **ACID & WAL**: Crash-resilient with Write-Ahead Logging.
- **Fast Graph Algorithms**: Built-in Dijkstra shortest path, BFS, cycle detection.

## Installation

```bash
pip install graphlite
```

Or build from source:

```bash
cd bindings/python
maturin develop
```

## Quick Start

```python
import graphlite

# 1. Open or create database
db = graphlite.GraphLite.open("mydb.db", pool_size=1024)

# 2. Execute Cypher CREATE statements
db.execute("CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob', age: 32});")

# 3. Query with Cypher, returns list of dicts
results = db.query("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;")
for row in results:
    print(row)
    # {'a.name': 'Alice', 'b.name': 'Bob', 'b.age': 32}

# 4. Shortest path with Dijkstra
res = db.dijkstra(1, 2, "KNOWS")
if res:
    cost, path = res
    print(f"Cost: {cost}, Path: {path}")

# 5. Buffer pool stats
print(db.stats())

# 6. Checkpoint
db.checkpoint()
```
