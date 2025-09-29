import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';

describe('Lazy.explain · limit/skip 传播估算', () => {
  it('limit/skip 对 estimatedOutput 生效', async () => {
    const dir = await makeWorkspace('unit-lazy-explain-ls');
    const db = await SynapseDB.open(within(dir, 'db.synapsedb'));
    for (let i = 0; i < 10; i++) db.addFact({ subject: 'S', predicate: 'R', object: `O${i}` });
    await db.flush();

    const q = db.find({ subject: 'S' }).skip(3).limit(2);
    const e = (q as any).explain();
    expect(e?.estimate?.estimatedOutput).toBe(2);

    await db.close();
    await cleanupWorkspace(dir);
  });
});
