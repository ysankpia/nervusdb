#!/usr/bin/env node
/**
 * 插入/扫描基准（可配置规模）
 * 用法：node benchmarks/insert_scan.mjs [N=20000]
 */
import { NervusDB } from '../dist/index.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mkdtemp, rm } from 'node:fs/promises';

const N = Number(process.argv[2] ?? '20000');

const t = async (name, fn) => {
  const s = performance.now();
  const r = await fn();
  const e = performance.now();
  console.log(`${name}: ${(e - s).toFixed(1)}ms`);
  return r;
};

async function main() {
  const ws = await mkdtemp(join(tmpdir(), 'synapsedb-bench-'));
  const db = await NervusDB.open(join(ws, 'bench.synapsedb'));
  await t('insert', async () => {
    for (let i = 0; i < N; i++) {
      db.addFact({ subject: `user${i%1000}`, predicate: 'KNOWS', object: `friend${i}` });
    }
  });
  await t('flush', () => db.flush());
  await t('scan-all', () => Promise.resolve(db.find({ predicate: 'KNOWS' }).toArray()));
  await t('scan-filter', () => Promise.resolve(db.find({ subject: 'user1', predicate: 'KNOWS' }).toArray()));
  await db.close();
  await rm(ws, { recursive: true, force: true });
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

