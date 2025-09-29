import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';

describe('LazyQueryBuilder · followPath 真流式', () => {
  it('与 EAGER followPath 结果一致（小图）', async () => {
    const dir = await makeWorkspace('unit-lazy-followpath');
    const db = await SynapseDB.open(within(dir, 'db.synapsedb'));
    // 形成一个简单的层级：A->B1,B2 ; B1->C1 ; B2->C2
    db.addFact({ subject: 'A', predicate: 'R', object: 'B1' });
    db.addFact({ subject: 'A', predicate: 'R', object: 'B2' });
    db.addFact({ subject: 'B1', predicate: 'R', object: 'C1' });
    db.addFact({ subject: 'B2', predicate: 'R', object: 'C2' });
    await db.flush();

    // EAGER 参考
    const eager = db.find({ subject: 'A' });
    const eagerR = eager.followPath('R', { min: 1, max: 2 }).all();
    const eagerKeys = new Set(eagerR.map((f) => `${f.subjectId}:${f.predicateId}:${f.objectId}`));

    // LAZY 流式
    const lazy = db.find({ subject: 'A' }).followPath('R', { min: 1, max: 2 });
    const lazyArr = await (lazy as any).collect();
    const lazyKeys = new Set(
      lazyArr.map((f: any) => `${f.subjectId}:${f.predicateId}:${f.objectId}`),
    );

    expect(lazyKeys.size).toBe(eagerKeys.size);
    for (const k of eagerKeys) expect(lazyKeys.has(k)).toBe(true);

    await db.close();
    await cleanupWorkspace(dir);
  });
});
