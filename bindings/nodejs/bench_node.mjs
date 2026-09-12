import pkg from "./index.js";
import { performance } from "perf_hooks";
import fs from "fs";

const { NervusDb } = pkg;
// 输出路径取自 DB_PATH，默认当前目录；刻意不硬编码作者本机的绝对路径。
const dbPath = process.env.DB_PATH || "bench_node.db";
const poolFrames = Number(process.env.POOL_FRAMES || 4096);
try { fs.unlinkSync(dbPath); fs.unlinkSync(dbPath + ".wal"); } catch (e) {}

const db = NervusDb.open(dbPath, poolFrames);
const NODES = 50000;
const EDGES = 100000;

// 构建模式必须显式声明并打印：debug 绑定比 release 慢约 6 倍，
// 历史上正是把 debug 数字与 release 核心对比，得出了错误的「SDK 慢 9 倍」结论。
const buildProfile = process.env.BUILD_PROFILE || "unknown";
if (buildProfile !== "release") {
  console.warn(`!! BUILD_PROFILE=${buildProfile}: rebuild with \`cargo build --release -p nervusdb-node\` first,`);
  console.warn("   otherwise these numbers are not comparable with the release figures in the docs.");
}

// 预热：首次运行包含 JIT/页缓存冷启动，实测首次比稳态低 3-4 倍。
const warm = db.beginTransaction();
for (let i = 1; i <= 2000; i++) warm.addNode(["Warmup"], { i });
warm.commit();

console.log(`=== Node.js SDK Benchmark ===`);
console.log(`DB: ${dbPath}  pool: ${poolFrames} frames  build: ${buildProfile}`);
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
