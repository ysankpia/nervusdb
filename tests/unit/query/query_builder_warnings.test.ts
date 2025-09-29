import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// 动态 import，确保在设置环境变量后再加载模块

describe('QueryBuilder warnings · 大结果集内存处理提示', () => {
  const originalThreshold = process.env.SYNAPSEDB_QUERY_WARN_THRESHOLD;
  const originalSilence = process.env.SYNAPSEDB_SILENCE_QUERY_WARNINGS;

  beforeEach(() => {
    process.env.SYNAPSEDB_QUERY_WARN_THRESHOLD = '2'; // 低阈值，方便触发
    delete process.env.SYNAPSEDB_SILENCE_QUERY_WARNINGS;
  });

  afterEach(() => {
    if (originalThreshold === undefined) delete process.env.SYNAPSEDB_QUERY_WARN_THRESHOLD;
    else process.env.SYNAPSEDB_QUERY_WARN_THRESHOLD = originalThreshold;
    if (originalSilence === undefined) delete process.env.SYNAPSEDB_SILENCE_QUERY_WARNINGS;
    else process.env.SYNAPSEDB_SILENCE_QUERY_WARNINGS = originalSilence;
    vi.restoreAllMocks();
  });

  it('all(): 大于阈值时输出一次警告', async () => {
    const { SynapseDB } = await import('@/synapseDb');
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => void 0);

    const db = await SynapseDB.open('tmp-warn-all.synapsedb');
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });

    const q = db.find({ subject: 'S' });
    const res = q.all();
    expect(res.length).toBe(3);
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls[0][0]).toContain('SynapseDB: all()');

    await db.close();
  });

  it('where(): 大于阈值时输出一次警告', async () => {
    const { SynapseDB } = await import('@/synapseDb');
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => void 0);

    const db = await SynapseDB.open('tmp-warn-where.synapsedb');
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });

    const q = db.find({ subject: 'S' });
    const q2 = q.where(() => true);
    // 只在 where 调用时发出一次警告
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls[0][0]).toContain('SynapseDB: where()');
    expect(q2.all().length).toBe(3);

    await db.close();
  });
});
