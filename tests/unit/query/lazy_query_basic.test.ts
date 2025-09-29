import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';

describe('LazyQueryBuilder · 基础流式链路', () => {
  it('findLazy + follow + limit 流式产出与 all() 结果一致', async () => {
    const db = await SynapseDB.open('tmp-lazy-basic.synapsedb');
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    db.addFact({ subject: 'A', predicate: 'R', object: 'C' });
    db.addFact({ subject: 'B', predicate: 'R', object: 'D' });
    await db.flush();

    const q = db.findLazy({ subject: 'A' }).follow('R').limit(2);

    const arr = q.all();
    expect(arr.length).toBe(2);
    // 异步迭代与 all() 一致
    const iter: string[] = [];
    for await (const f of q) {
      iter.push(`${f.subjectId}:${f.predicateId}:${f.objectId}`);
    }
    expect(iter.length).toBe(2);

    await db.close();
  });

  it('whereProperty/whereLabel 在流上过滤', async () => {
    const db = await SynapseDB.open('tmp-lazy-filter.synapsedb');
    db.addFact(
      { subject: 'X', predicate: 'IS', object: 'Person' },
      { subjectProperties: { age: 30 } },
    );
    db.addFact(
      { subject: 'Y', predicate: 'IS', object: 'Person' },
      { subjectProperties: { age: 20 } },
    );
    await db.flush();

    const res = db
      .findLazy({ predicate: 'IS', object: 'Person' })
      .whereProperty('age', '>=', 25, 'node')
      .all();
    expect(res.length).toBe(1);

    await db.close();
  });
});
