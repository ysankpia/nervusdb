import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';
import { readPagedManifest } from '@/storage/pagedIndex';

describe('Auto-Compact 决策与执行', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-auto-'));
    dbPath = join(workspace, 'ac.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('在 primary 拥有多页时自动选择并执行增量合并', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();

    const decision = await autoCompact(dbPath, { mode: 'incremental', orders: ['SPO'], minMergePages: 2 });
    expect(decision.selectedOrders).toContain('SPO');

    // 数据保持一致
    const facts = db.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBe(3);
  });
});
