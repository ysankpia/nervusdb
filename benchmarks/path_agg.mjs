#!/usr/bin/env node
/**
 * 路径遍历与聚合流式基准
 */
import { NervusDB } from '../dist/index.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mkdtemp, rm } from 'node:fs/promises';

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

  // 构造 R 星型 + 链式，用于路径基准
  for (let i = 0; i < 2000; i++) {
    db.addFact({ subject: 'root', predicate: 'R', object: `n${i}` });
  }
  for (let i = 0; i < 1000; i++) {
    db.addFact({ subject: `c${i}`, predicate: 'R', object: `c${i+1}` });
  }
  await db.flush();

  await t('variablePath [min..max]', () =>
    Promise.resolve(db.find({ subject: 'c0' }).followPath('R', { min: 3, max: 5 }).toArray()),
  );

  // 聚合流式：构造多用户评分
  for (let u = 0; u < 1000; u++) {
    for (let k = 0; k < 5; k++) {
      db.addFact({ subject: `user${u}`, predicate: 'RATED', object: `item${u}-${k}` }, { edgeProperties: { score: (k % 5) + 1 } });
    }
  }
  await db.flush();

  await t('aggregation streaming', async () => {
    const rows = await db
      .aggregate()
      .groupBy(['subject'])
      .avg('edgeProperties.score', 'avg')
      .matchStream({ predicate: 'RATED' }, { batchSize: 1000 })
      .executeStreaming();
    return rows.length;
  });

  await db.close();
  await rm(ws, { recursive: true, force: true });
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

