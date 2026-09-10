// Reproducible throughput benchmark for the Node.js SDK.
//
// Run with:
//   cd bindings/nodejs && node benches/throughput.mjs
//
// Scale is controlled by GL_SCALE=small for a quick smoke run.
// Every result is printed next to its configuration so a number can never be
// quoted without its measurement conditions.

import { GraphLite } from "../index.js";
import fs from "fs";
import os from "os";
import path from "path";

const SCALE = process.env.GL_SCALE || "full";
const QUICK = SCALE === "small";

function rm(p) {
  for (const s of ["", ".wal"]) {
    if (fs.existsSync(p + s)) fs.rmSync(p + s);
  }
}

function benchNodes(db, count, label) {
  const t0 = performance.now();
  const tx = db.beginTransaction();
  for (let i = 1; i <= count; i++) {
    tx.addNode(["Entity"], {
      idx: i,
      name: `entity-${String(i).padStart(8, "0")}`,
    });
  }
  tx.commit();
  const elapsed = (performance.now() - t0) / 1000;
  console.log(
    `${label.padEnd(14)}: ${String(Math.round(count / elapsed)).padStart(12)} ops/s   (${elapsed.toFixed(3)}s for ${count} nodes)`,
  );
}

function benchEdges(db, nodes, edges, label) {
  const t0 = performance.now();
  const tx = db.beginTransaction();
  let written = 0;
  for (let i = 0; i < edges; i++) {
    const src = (i % nodes) + 1;
    const dst = ((i * 7919 + 13) % nodes) + 1;
    if (src !== dst) {
      tx.addEdge(src, dst, "REL", {}, 1.0);
      written++;
    }
  }
  tx.commit();
  const elapsed = (performance.now() - t0) / 1000;
  console.log(
    `${label.padEnd(14)}: ${String(Math.round(written / elapsed)).padStart(12)} ops/s   (${elapsed.toFixed(3)}s for ${written} edges)`,
  );
}

function scenario(label, nodes, edges, poolFrames, dbPath) {
  rm(dbPath);
  console.log(`\n=== ${label} ===`);
  console.log(
    `config        : ${nodes} nodes, ${edges} edges, pool ${poolFrames} frames (${(poolFrames * 4) / 1024} MB)`,
  );

  const db = GraphLite.open(dbPath, poolFrames);
  benchNodes(db, nodes, "nodes");
  if (edges > 0) benchEdges(db, nodes, edges, "edges");

  const t0 = performance.now();
  db.checkpoint();
  console.log(
    `${"checkpoint".padEnd(14)}: ${((performance.now() - t0) / 1000).toFixed(3)}s`,
  );

  const size = fs.existsSync(dbPath) ? fs.statSync(dbPath).size : 0;
  console.log(`file size     : ${(size / 1048576).toFixed(2)} MB`);
  console.log(`buffer stats  : ${JSON.stringify(db.stats())}`);

  rm(dbPath);
}

function main() {
  console.log(`GraphLite Node.js SDK throughput benchmark (scale = ${SCALE})`);
  if (QUICK) console.log("NOTE: smoke scale, not the documented figures.");

  const dbPath = path.join(os.tmpdir(), "graphlite-node-bench.db");
  const n = QUICK ? 100000 : 1000000;
  const e = QUICK ? 400000 : 4000000;
  scenario("1M nodes + 4M edges, file-backed, 64MB pool", n, e, 16384, dbPath);
  scenario("1M nodes + 4M edges, file-backed, 1MB pool", n, e, 256, dbPath);
}

main();
