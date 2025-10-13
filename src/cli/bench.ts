#!/usr/bin/env node
import { NervusDB } from '../synapseDb.js';

async function main() {
  const [dbPath, countArg, modeArg] = process.argv.slice(2);
  if (!dbPath) {
    console.log('用法: pnpm bench <db> [count=10000] [mode=default|lsm]');
    process.exit(1);
  }
  const count = Number(countArg ?? '10000');
  const stagingMode = modeArg === 'lsm' ? ('lsm-lite' as any) : undefined;
  const db = await NervusDB.open(dbPath, { pageSize: 1024, stagingMode });
  console.time('insert');
  for (let i = 0; i < count; i += 1) {
    db.addFact({ subject: `S${i % 1000}`, predicate: `R${i % 50}`, object: `O${i}` });
  }
  console.timeEnd('insert');
  const metrics = db.getStagingMetrics?.();
  if (metrics) console.log('staging', metrics);
  console.time('flush');
  await db.flush();
  console.timeEnd('flush');
  console.time('query');
  const res = db.find({ subject: 'S1', predicate: 'R1' }).all();
  console.timeEnd('query');
  console.log('hits', res.length);
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
