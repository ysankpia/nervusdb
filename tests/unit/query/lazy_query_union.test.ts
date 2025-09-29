import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';

describe('LazyQueryBuilder · union/unionAll', () => {
  it('union: 去重合并两路流', async () => {
    const db = await SynapseDB.open('tmp-lazy-union.synapsedb');
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'T', predicate: 'R', object: 'O2' });
    await db.flush();

    const a = db.findLazy({ subject: 'S' });
    const b = db.findLazy({ object: 'O2' });
    const u = a.union(b);
    const arr = u.all();
    // 期望至少包含三条（S-R-O1, S-R-O2, T-R-O2），且无重复
    expect(arr.length).toBe(3);

    // 异步迭代也能得到三条
    let count = 0;
    for await (const _ of u) count += 1;
    expect(count).toBe(3);

    await db.close();
  });

  it('unionAll: 简单拼接', async () => {
    const db = await SynapseDB.open('tmp-lazy-unionall.synapsedb');
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();

    const a = db.findLazy({ subject: 'S' });
    const u = a.unionAll(a);
    const arr = u.all();
    expect(arr.length).toBe(4); // 2 + 2

    let count = 0;
    for await (const _ of u) count += 1;
    expect(count).toBe(4);

    await db.close();
  });
});

