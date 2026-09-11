import pkg from "./index.js";
import { performance } from "perf_hooks";
import fs from "fs";

const { GraphLite } = pkg;
// 输出路径取自 DB_PATH，默认当前目录；刻意不硬编码作者本机的绝对路径。
const dbPath = process.env.DB_PATH || "bench_node.db";
const poolFrames = Number(process.env.POOL_FRAMES || 4096);
try { fs.unlinkSync(dbPath); fs.unlinkSync(dbPath + ".wal"); } catch (e) {}

const db = GraphLite.open(dbPath, poolFrames);
const NODES = 50000;
const EDGES = 100000;

console.log(`=== Node.js SDK Benchmark ===`);
console.log(`DB: ${dbPath}  pool: ${poolFrames} frames`);
// 1. Nodes
const t0 = performance.now();
const tx1 = db.beginTransaction();
for (let i = 1; i <= NODES; i++) {
  tx1.addNode(["Person"], { idx: i, name: `node-${i}` });
}
tx1.commit();
const nodeDur = (performance.now() - t0) / 1000;
console.log(`Nodes: ${NODES} in ${nodeDur.toFixed(3)}s (${(NODES / nodeDur).toFixed(0)} ops/s)`);

// 2. Edges
const t1 = performance.now();
const tx2 = db.beginTransaction();
for (let i = 0; i < EDGES; i++) {
  const src = (i % NODES) + 1;
  const dst = ((i * 7919 + 13) % NODES) + 1;
  if (src !== dst) {
    tx2.addEdge(src, dst, "KNOWS", { weight: 1.5 }, 1.5);
  }
}
tx2.commit();
const edgeDur = (performance.now() - t1) / 1000;
console.log(`Edges: ${EDGES} in ${edgeDur.toFixed(3)}s (${(EDGES / edgeDur).toFixed(0)} ops/s)`);

db.checkpoint();
try { fs.unlinkSync(dbPath); fs.unlinkSync(dbPath + ".wal"); } catch (e) {}
