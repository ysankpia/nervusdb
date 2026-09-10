# GraphLite Node.js / TypeScript SDK

Official Node.js and TypeScript bindings for **GraphLite-RS**, the SQLite of Graph Databases.

## Features

- **Embedded & Zero-Configuration**: Works on a single `.db` file with zero background daemons.
- **Native Cypher Support**: Execute `CREATE`, `MATCH`, `WHERE`, `RETURN`, `LIMIT` directly.
- **Fast Node-API (NAPI-RS)**: High-performance C++ ABI boundary with zero-cost serialization.
- **TypeScript First**: Full TypeScript definitions (`.d.ts`) included out-of-the-box.
- **4KB Paged Buffer Pool**: Low bounded memory footprint.
- **ACID & WAL**: Safe against abrupt crashes and power outages.

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

// 3. Query with Cypher
const results = db.query(
  "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;",
);
console.log(results);
// Output: [ { 'a.name': 'Alice', 'b.name': 'Bob', 'b.age': 32 } ]

// 4. Graph Algorithms (Dijkstra)
const sp = db.dijkstra(1, 2, "KNOWS");
if (sp) {
  console.log(`Cost: ${sp.cost}, Path: ${sp.path}`);
}

// 5. Buffer Pool stats
console.log(db.stats());

// 6. Checkpoint
db.checkpoint();
```

## Development & Build

```bash
cd bindings/nodejs
npm install
npm run build
npm test
```
