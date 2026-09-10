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

// 6. BFS / cycle detection
const bfs = db.bfs(n1, n2, "KNOWS");
console.log("BFS result:", bfs);
if (!bfs || bfs.length !== 2) {
  throw new Error(`Invalid BFS result: ${JSON.stringify(bfs)}`);
}
if (db.hasCycle() !== false) {
  throw new Error("Graph must be acyclic");
}

// 7. Advanced Cypher: SET / ORDER BY / SKIP / LIMIT / aggregation
const setRes = db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 29;");
if (setRes.properties_set !== 1) {
  throw new Error(`SET failed: ${JSON.stringify(setRes)}`);
}

const ordered = db.query(
  "MATCH (p:Person) RETURN p.name, p.age ORDER BY p.age DESC SKIP 1 LIMIT 2;",
);
console.log("ORDER BY / SKIP / LIMIT result:", ordered);
if (ordered.length !== 2) {
  throw new Error(`Paging failed: ${JSON.stringify(ordered)}`);
}

const agg = db.query(
  "MATCH (p:Person) RETURN count(p), sum(p.age), avg(p.age);",
);
console.log("Aggregation result:", agg);
if (agg[0]["count(p)"] !== 4) {
  throw new Error(`Aggregation count failed: ${JSON.stringify(agg)}`);
}

// 8. PageRank
const ranks = db.pageRank(0.85, 100, 1e-9);
console.log("PageRank result:", ranks);
if (!Array.isArray(ranks) || ranks.length !== 4) {
  throw new Error(`PageRank failed: ${JSON.stringify(ranks)}`);
}

// 9. Weakly connected components (Alice->Bob and Charlie->David form two islands)
const wcc = db.weaklyConnectedComponents();
console.log("WCC result:", wcc);
if (wcc.length !== 2) {
  throw new Error(`WCC failed: ${JSON.stringify(wcc)}`);
}
if (
  !wcc.some((c) => c.includes(n1) && c.includes(n2)) ||
  !wcc.some((c) => c.includes(3) && c.includes(4))
) {
  throw new Error(`WCC components incorrect: ${JSON.stringify(wcc)}`);
}

// 10. K-Hop subgraph
const sub = db.kHopSubgraph(n1, 2, "outgoing", "KNOWS");
console.log("K-Hop subgraph:", sub);
if (!sub.nodes.includes(n1) || sub.nodes.length < 2) {
  throw new Error(`K-Hop failed: ${JSON.stringify(sub)}`);
}

// 11. Schema introspection & dump
console.log("Labels:", db.labels(), "EdgeTypes:", db.edgeTypes());
const dump = db.dumpCypher();
if (!dump.includes("CREATE") || !dump.includes("Alice")) {
  throw new Error(`Dump looks wrong: ${dump}`);
}

// 12. 显式批量事务：一次 commit 恰好一次 fsync，且吞吐远超逐条自动提交
const fsyncBefore = db.stats().wal_fsync_count;
const nodesBefore =
  db.query("MATCH (t:Batch) RETURN count(t);")[0]["count(t)"] ?? 0;
const BATCH = 20000;

const tx = db.beginTransaction();
console.log(`Transaction #${tx.txId()} opened`);
for (let i = 0; i < BATCH; i++) {
  tx.addNode(["Batch"], { idx: i, name: `bulk-${i}` });
}
tx.commit();

const afterBatch = db.stats();
const batchCount = db.query("MATCH (t:Batch) RETURN count(t);")[0]["count(t)"];
if (batchCount !== nodesBefore + BATCH) {
  throw new Error(`batched insert count mismatch: ${batchCount}`);
}
if (afterBatch.wal_fsync_count - fsyncBefore !== 1) {
  throw new Error(
    `a single batched commit of ${BATCH} writes must fsync exactly once, got ${
      afterBatch.wal_fsync_count - fsyncBefore
    }`,
  );
}
console.log(
  `Batch commit OK: ${BATCH} nodes, fsync delta = ${
    afterBatch.wal_fsync_count - fsyncBefore
  }`,
);

// 13. 事务回滚必须零污染
const beforeRollback =
  db.query("MATCH (r:Taint) RETURN count(r);")[0]["count(r)"] ?? 0;
const tx2 = db.beginTransaction();
for (let i = 0; i < 1000; i++) {
  tx2.addNode(["Taint"], { idx: i });
}
tx2.rollback();
const afterRollback =
  db.query("MATCH (r:Taint) RETURN count(r);")[0]["count(r)"] ?? 0;
if (afterRollback !== beforeRollback) {
  throw new Error(`rollback leaked data: ${afterRollback}`);
}
console.log("Transaction rollback produced zero pollution");

// 14. 批量事务内的混合操作（增/改/删）
const tx3 = db.beginTransaction();
const mixedA = tx3.addNode(["Mixed"], { v: 1 });
const mixedB = tx3.addNode(["Mixed"], { v: 2 });
tx3.addEdge(mixedA, mixedB, "LINK", { w: 9.5 }, 2.0);
tx3.updateNodeProperty(mixedA, "v", 42);
tx3.removeNode(mixedB);
tx3.commit();
const mixedRows = db.query("MATCH (m:Mixed) RETURN m.v;");
if (mixedRows.length !== 1 || mixedRows[0]["m.v"] !== 42) {
  throw new Error(`mixed batch ops wrong: ${JSON.stringify(mixedRows)}`);
}
console.log("Mixed batch operations committed atomically");

// 15. Buffer pool stats
const stats = db.stats();
console.log("Buffer Pool stats:", stats);
if (stats.capacity_frames !== 512) {
  throw new Error(`Invalid capacity frames: ${stats.capacity_frames}`);
}
if (typeof stats.wal_fsync_count !== "number") {
  throw new Error("wal_fsync_count must be exposed in stats");
}

// 16. Checkpoint
db.checkpoint();
console.log("Checkpoint executed successfully!");
console.log("All Node.js / TypeScript SDK tests passed!");
