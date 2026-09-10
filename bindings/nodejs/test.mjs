import pkg from "./index.js";
import os from "os";
import path from "path";
import fs from "fs";

const { GraphLite } = pkg;

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "graphlite-node-"));
const dbPath = path.join(tmpDir, "node_test.db");

console.log(`Testing GraphLite Node.js SDK at ${dbPath}...`);

// 1. Open database
const db = GraphLite.open(dbPath, 512);

// 2. Add nodes & edge directly
const n1 = db.addNode(["Person"], { name: "Alice", age: 28 });
const n2 = db.addNode(["Person"], { name: "Bob", age: 32 });
const edge = db.addEdge(n1, n2, "KNOWS", { since: 2023 }, 1.5);

console.log(`Created nodes: n1=${n1}, n2=${n2}, edge=${edge}`);
if (n1 <= 0 || n2 <= 0 || edge <= 0) {
  throw new Error("Failed to create nodes or edge");
}

// 3. Cypher execute
const execRes = db.execute(
  "CREATE (c:Person {name: 'Charlie', age: 25})-[:KNOWS {weight: 2.0}]->(d:Person {name: 'David', age: 38});",
);
console.log("Cypher execute result:", execRes);
if (execRes.nodes_created !== 2 || execRes.edges_created !== 1) {
  throw new Error(`Unexpected execute result: ${JSON.stringify(execRes)}`);
}

// 4. Cypher query
const rows = db.query(
  "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 30 RETURN a.name, b.name, b.age;",
);
console.log("Cypher query result:", rows);
if (!Array.isArray(rows) || rows.length === 0) {
  throw new Error("Query returned empty result");
}
for (const row of rows) {
  if (row["b.age"] <= 30) {
    throw new Error(`Filter condition violated: ${JSON.stringify(row)}`);
  }
}

// 5. Dijkstra algorithm
const sp = db.dijkstra(n1, n2, "KNOWS");
console.log("Dijkstra result:", sp);
if (!sp || sp.cost !== 1.5 || sp.path.length !== 2) {
  throw new Error(`Invalid shortest path result: ${JSON.stringify(sp)}`);
}

// 6. Buffer pool stats
const stats = db.stats();
console.log("Buffer Pool stats:", stats);
if (stats.capacity_frames !== 512) {
  throw new Error(`Invalid capacity frames: ${stats.capacity_frames}`);
}

// 7. Checkpoint
db.checkpoint();
console.log("Checkpoint executed successfully!");
console.log("All Node.js / TypeScript SDK tests passed!");
