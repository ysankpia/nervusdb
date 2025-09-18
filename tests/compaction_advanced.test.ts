import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { compactDatabase } from '@/maintenance/compaction';
import { readPagedManifest } from '@/storage/pagedIndex';

describe('Compaction 高级选项', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-compact-adv-'));
    dbPath = join(workspace, 'c.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('dry-run 仅输出统计，不修改 manifest', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    const m1 = await readPagedManifest(`${dbPath}.pages`);
    const pagesBefore = m1!.lookups.find((l) => l.order === 'SPO')!.pages.length;

    const stats = await compactDatabase(dbPath, { dryRun: true, minMergePages: 1, orders: ['SPO'] });
    expect(stats.ordersRewritten).toContain('SPO');

    const m2 = await readPagedManifest(`${dbPath}.pages`);
    const pagesAfter = m2!.lookups.find((l) => l.order === 'SPO')!.pages.length;
    expect(pagesAfter).toBe(pagesBefore); // 未变更
  });

  it('orders 过滤仅重写指定顺序', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R1', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R2', object: 'O2' });
    await db.flush();
    const stats = await compactDatabase(dbPath, { orders: ['SPO'], minMergePages: 1 });
    expect(stats.ordersRewritten).toEqual(['SPO']);
  });
});

