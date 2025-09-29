import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';

describe('LazyQueryBuilder · 复杂链路与 length/slice', () => {
  it('follow→whereProperty→followReverse→skip/limit 组合链路', async () => {
    const db = await SynapseDB.open('tmp-lazy-complex.synapsedb');
    // A -KNOWS-> B(age=30), C(age=20); D(age=40) -KNOWS-> B
    db.addFact(
      { subject: 'A', predicate: 'KNOWS', object: 'B' },
      { objectProperties: { age: 30 } },
    );
    db.addFact(
      { subject: 'A', predicate: 'KNOWS', object: 'C' },
      { objectProperties: { age: 20 } },
    );
    db.addFact(
      { subject: 'D', predicate: 'KNOWS', object: 'B' },
      { subjectProperties: { age: 40 } },
    );
    await db.flush();

    const q = db
      .findLazy({ subject: 'A' })
      .follow('KNOWS')
      .whereProperty('age', '>=', 25, 'node') // 命中 B(30)
      .followReverse('KNOWS') // 反向：谁认识这些人 => A 与 D
      .skip(1)
      .limit(1);

    // 异步流
    const got: string[] = [];
    for await (const f of q) {
      got.push(`${f.subjectId}:${f.predicateId}:${f.objectId}`);
    }
    expect(got.length).toBe(1);

    // 同步物化也应一致
    const arr = q.all();
    expect(arr.length).toBe(1);

    await db.close();
  });

  it('length/slice 在 LazyQueryBuilder 上可用（物化一次）', async () => {
    const db = await SynapseDB.open('tmp-lazy-length.synapsedb');
    db.addFact({ subject: 'X', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'X', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'X', predicate: 'R', object: 'O3' });
    await db.flush();

    const q = db.findLazy({ subject: 'X' });
    expect(q.length).toBe(3);
    const s = q.slice(0, 2);
    expect(s.length).toBe(2);

    await db.close();
  });
});
