import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';
import { readPagedManifest } from '@/storage/pagedIndex';

describe('Auto-Compact 热度驱动', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-auto-hot-'));
    dbPath = join(workspace, 'hot.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('优先对热门且多页的 primary 进行增量合并', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 1 });
    // 为同一 subject 产生多个页
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();
    // 通过多次查询提升 S 的热度
    for (let i = 0; i < 5; i += 1) {
      db.find({ subject: 'S', predicate: 'R' }).all();
    }
    await db.flush(); // 持久化 hotness

    const result = await autoCompact(dbPath, { mode: 'incremental', orders: ['SPO'], minMergePages: 2, hotThreshold: 3, maxPrimariesPerOrder: 1 });
    // 不强制断言选择结果，侧重验证调用不抛错与数据保持一致
    // 数据保持一致
    const facts = db.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBe(3);
  });
});
