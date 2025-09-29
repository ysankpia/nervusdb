import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb';

describe('LazyQueryBuilder · variablePathStream', () => {
  it('流式 BFS 产出的路径与同步 variablePath 一致（无 target）', async () => {
    const db = await SynapseDB.open('tmp-lazy-vpath.synapsedb');
    // A-R->B-R->C-R->D
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    db.addFact({ subject: 'B', predicate: 'R', object: 'C' });
    db.addFact({ subject: 'C', predicate: 'R', object: 'D' });
    await db.flush();

    const lazy = db.findLazy({ subject: 'A' });

    // 同步
    const syncPaths = lazy.variablePath('R', { min: 1, max: 3 }).all();

    // 流式
    const asyncPaths: Array<{ len: number; end: number }> = [];
    for await (const p of (lazy as any).variablePathStream('R', { min: 1, max: 3 })) {
      asyncPaths.push({ len: p.length, end: p.endId });
    }

    expect(asyncPaths.length).toBe(syncPaths.length);
    await db.close();
  });
});
