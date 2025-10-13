import { describe, it, expect } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';

describe('LazyQueryBuilder · collect()', () => {
  it('collect() 与 all() 结果一致，但以异步方式收集', async () => {
    const dir = await makeWorkspace('unit-lazy-collect');
    const db = await NervusDB.open(within(dir, 'db.synapsedb'));
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();

    const q = db.findLazy({ subject: 'S' }).follow('R');
    const all = q.all();
    const collected = await q.collect();
    expect(collected.length).toBe(all.length);
    await db.close();
    await cleanupWorkspace(dir);
  });
});
