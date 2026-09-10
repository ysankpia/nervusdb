# GraphLite Node.js / TypeScript SDK

Official Node.js and TypeScript bindings for **GraphLite-RS**, the SQLite of Graph Databases.

## Features

- **Embedded & Zero-Configuration**: Works on a single `.db` file with zero background daemons.
- **Native Cypher 1.0 Support**: Execute `CREATE`, `MATCH`, `WHERE`, `RETURN`, `SET`, `DELETE`, `DETACH DELETE`, `ORDER BY`, `SKIP`, `LIMIT`, aggregates (`count`, `sum`, `avg`, `min`, `max`), and multi-hop paths (`*1..3`).
- **Enterprise Graph Algorithms**: Built-in `pageRank`, `weaklyConnectedComponents` (WCC), `kHopSubgraph`, `dijkstra`, `bfs`, and `hasCycle`.
- **Fast Node-API (NAPI-RS)**: High-performance C++ ABI boundary with zero-cost serialization.
- **TypeScript First**: Full TypeScript definitions (`.d.ts`) included out-of-the-box.
- **4KB Paged Buffer Pool & STEAL Spill**: Bounded memory footprint with out-of-core support.
- **ACID & WAL**: Crash-resilient with page-level Write-Ahead Logging.
- **Batched Transactions**: Group commits with a single WAL fsync guarantee.
- **Logical Dump**: Export entire graph to replayable Cypher scripts via `dumpCypher()`.

## Installation

```bash
npm install graphlite-node
# or
npm install @graphlite/core
```

## Quick Start

```typescript
import { GraphLite } from "graphlite-node";

// 1. Open or create database (poolSize in frames: 1024 = 4MB)
const db = GraphLite.open("mydb.db", 1024);

// 2. Execute Cypher statements
db.execute(
  "CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob', age: 32});",
);

// 3. Query with Cypher (supports ORDER BY, SKIP, LIMIT, Aggregates)
const results = db.query(
  "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;",
);
console.log(results);
// Output: [ { 'a.name': 'Alice', 'b.name': 'Bob', 'b.age': 32 } ]

// 4. Update and Delete via Cypher
db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 29, a:Engineer;");
db.execute("MATCH (b:Person {name: 'Bob'}) DETACH DELETE b;");

// 5. Graph Analytics & Algorithms
const ranks = db.pageRank(0.85, 100, 1e-6); // [{ node_id, score }, ...]
const components = db.weaklyConnectedComponents(); // [[node_id, ...], ...]
const sub = db.kHopSubgraph(1, 2, "outgoing", "KNOWS"); // { nodes: [...], edges: [...] }
const sp = db.dijkstra(1, 2, "KNOWS");
console.log(`Dijkstra: cost=${sp?.cost}, path=${sp?.path}`);

// 6. Explicit Batched Transactions (Single fsync commit)
const tx = db.beginTransaction();
for (let i = 0; i < 10000; i++) {
  tx.addNode(["Bulk"], { idx: i });
}
tx.commit();

// 7. Schema introspection & Cypher Dump export
console.log("Labels:", db.labels(), "Edge types:", db.edgeTypes());
const cypherScript = db.dumpCypher();

// 8. Buffer Pool stats & Checkpoint
console.log(db.stats());
db.checkpoint();
```

## Development & Build

```bash
cd bindings/nodejs
npm install
npm run build
npm test
```
