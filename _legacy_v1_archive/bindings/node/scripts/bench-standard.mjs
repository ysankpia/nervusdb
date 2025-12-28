#!/usr/bin/env node
// 简易标准基准：生成合成数据，对比 PatternBuilder 与 Cypher 引擎的查询耗时
// 用法：node scripts/bench-standard.mjs <db> [--count=100000] [--limit=1000]

import { NervusDB } from '../dist/nervusDb.js';

function parseArgs(argv) {
  const args = { db: '', count: 100000, limit: 1000 };
  if (!argv[0]) return args;
  args.db = argv[0];
  for (const a of argv.slice(1)) {
    if (a.startsWith('--count=')) args.count = Number(a.slice(8));
    else if (a.startsWith('--limit=')) args.limit = Number(a.slice(8));
  }
  return args;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.db) {
    console.log('用法: node scripts/bench-standard.mjs <db> [--count=100000] [--limit=1000]');
    process.exit(1);
  }

  const db = await NervusDB.open(args.db);

  // 合成数据：S{i%10000} -[R]-> O{i}
  console.log(`生成数据 ${args.count} 条...`);
  console.time('insert');
  for (let i = 0; i < args.count; i++) {
    db.addFact({ subject: `S${i % 10000}`, predicate: 'R', object: `O${i}` });
  }
  console.timeEnd('insert');
  console.time('flush');
  await db.flush();
  console.timeEnd('flush');

  // PatternBuilder 查询
  console.time('pattern.query');
  const pRes = db.find({ predicate: 'R' }).limit(args.limit).all();
  console.timeEnd('pattern.query');
  console.log('pattern.hits', pRes.length);

  // Cypher 查询
  console.time('cypher.query');
  const cRes = await db.cypherRead('MATCH (s)-[:R]->(o) RETURN s,o', {}, { enableOptimization: true });
  console.timeEnd('cypher.query');
  console.log('cypher.hits', cRes.records.length);

  await db.close();
}

// eslint-disable-next-line n/no-unpublished-bin
main().catch((e) => {
  console.error(e);
  process.exit(1);
});

