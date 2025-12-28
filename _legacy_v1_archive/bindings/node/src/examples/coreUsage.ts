/**
 * 示例：薄绑定（thin binding）
 *
 * 绑定层只做参数转换，所有执行逻辑都在 Rust Core。
 */

import { NervusDB } from '../index.js';

export async function coreExample() {
  const db = await NervusDB.open(':memory:');
  db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
  const facts = db.listFacts();
  const hits = db.getStore().query({ predicate: 'knows' });
  await db.close();
  console.log('Core example:', facts.length, 'facts, query hits', hits.length);
}

export async function algorithmsExample() {
  const db = await NervusDB.open(':memory:');
  db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
  db.addFact({ subject: 'Bob', predicate: 'knows', object: 'Charlie' });

  const store = db.getStore();
  const aliceId = store.intern('Alice');
  const charlieId = store.intern('Charlie');
  const knowsId = store.intern('knows');
  const path = db.algorithms.bfsShortestPath(aliceId, charlieId, knowsId, {
    maxHops: 10,
    bidirectional: true,
  });
  console.log('Shortest path:', path?.path ?? null);

  await db.close();
}

if (import.meta.url === `file://${process.argv[1]}`) {
  (async () => {
    console.log('=== NervusDB Examples ===');
    await coreExample();
    await algorithmsExample();
  })().catch(console.error);
}
