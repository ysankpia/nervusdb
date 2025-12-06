#!/usr/bin/env node
/**
 * 最小基准脚本（实验性）
 * 用法：node benchmarks/basic.mjs
 */
import { NervusDB } from '../dist/index.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mkdtemp, rm } from 'node:fs/promises';

const time = async (name, fn) => {
  const t0 = performance.now();
  const res = await fn();
  const t1 = performance.now();
  console.log(`BENCH ${name}: ${(t1 - t0).toFixed(1)}ms`);
  return res;
};

async function main() {
  const ws = await mkdtemp(join(tmpdir(), 'synapsedb-bench-'));
  const db = await NervusDB.open(join(ws, 'bench.synapsedb'));
  const N = 5000;
  await time('insert', async () => {
    for (let i = 0; i < N; i++) {
      db.addFact({ subject: `u${i}`, predicate: 'KNOWS', object: `v${i}` });
    }
    await db.flush();
  });
  await time('full-scan', () => Promise.resolve(db.find({ predicate: 'KNOWS' }).toArray()));
  await db.close();
  await rm(ws, { recursive: true, force: true });
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

