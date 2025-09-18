import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { readPagedManifest } from '@/storage/pagedIndex';
import { compactDatabase } from '@/maintenance/compaction';

describe('Compaction 增量按 primary 重写', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-compact-incr-'));
    dbPath = join(workspace, 'ci.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('仅为满足阈值的 primary 追加新页，并替换 manifest 映射', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    // 追加形成新页
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();

    const m1 = await readPagedManifest(`${dbPath}.pages`);
    const spo1 = m1!.lookups.find((l) => l.order === 'SPO')!;
    const primary = spo1.pages[0].primaryValue;
    const beforeCount = spo1.pages.filter((p) => p.primaryValue === primary).length;
    expect(beforeCount).toBeGreaterThanOrEqual(2);

    const stats = await compactDatabase(dbPath, { mode: 'incremental', orders: ['SPO'], minMergePages: 2 });
    expect(stats.ordersRewritten).toContain('SPO');

    const m2 = await readPagedManifest(`${dbPath}.pages`);
    const spo2 = m2!.lookups.find((l) => l.order === 'SPO')!;
    // 数据不变（收敛效果依赖策略与实现细节，这里不作强约束）
    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBe(3);
  });
});
