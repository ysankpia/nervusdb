import { describe, it, expect } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';

describe('Lazy.explain · FOLLOW 基数传播（粗略）', () => {
  it('explain() stages 含 FOLLOW，并使 estimatedOutput 不小于初始 upperBound（如有）', async () => {
    const dir = await makeWorkspace('unit-lazy-explain-follow');
    const db = await NervusDB.open(within(dir, 'db.synapsedb'));
    // 构造若干从 S1 出发的边，提高平均度
    for (let i = 0; i < 8; i++) db.addFact({ subject: 'S1', predicate: 'R', object: `O${i}` });
    // 再加一些与其他主体有关的边，避免过小样本
    for (let i = 0; i < 4; i++) db.addFact({ subject: `SX${i}`, predicate: 'R', object: `OY${i}` });
    await db.flush();

    const q = db.find({ subject: 'S1' }).follow('R');
    const e = (q as any).explain();
    expect(e?.estimate?.order).toBeTypeOf('string');

    const stages = e?.estimate?.stages as
      | Array<{ type: string; factor?: number; output?: number }>
      | undefined;
    expect(Array.isArray(stages)).toBe(true);
    const f = stages?.find((s) => s.type === 'FOLLOW');
    expect(f).toBeDefined();
    if (f) expect(typeof f.factor === 'number' && f.factor > 0).toBe(true);

    const ub = e?.estimate?.upperBound as number | undefined;
    const out = e?.estimate?.estimatedOutput as number | undefined;
    if (typeof ub === 'number' && typeof out === 'number') expect(out).toBeGreaterThanOrEqual(ub);

    await db.close();
    await cleanupWorkspace(dir);
  });
});
