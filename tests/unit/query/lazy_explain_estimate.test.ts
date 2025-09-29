import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';

describe('LazyQueryBuilder.explain · 估算摘要', () => {
  it('explain() 返回 LAZY 计划与 estimate 概要', async () => {
    const dir = await makeWorkspace('unit-lazy-explain');
    const db = await SynapseDB.open(within(dir, 'db.synapsedb'));
    db.addFact({ subject: 'S1', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S1', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S2', predicate: 'R', object: 'O3' });
    await db.flush();

    const q = db.find({ subject: 'S1' });
    const summary = (q as any).explain();
    expect(summary).toBeDefined();
    expect(summary.type).toBe('LAZY');
    expect(summary.plan?.length).toBeGreaterThan(0);
    // 粗略估计信息存在（不强制校验具体数值）
    expect(summary.estimate?.order).toBeTypeOf('string');

    await db.close();
    await cleanupWorkspace(dir);
  });
});
