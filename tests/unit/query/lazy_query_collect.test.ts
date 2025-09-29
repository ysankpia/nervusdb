import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';

describe('LazyQueryBuilder · collect()', () => {
  it('collect() 与 all() 结果一致，但以异步方式收集', async () => {
    const db = await SynapseDB.open('tmp-lazy-collect.synapsedb');
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();

    const q = db.findLazy({ subject: 'S' }).follow('R');
    const all = q.all();
    const collected = await q.collect();
    expect(collected.length).toBe(all.length);
    await db.close();
  });
});

