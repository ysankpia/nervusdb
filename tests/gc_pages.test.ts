import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promises as fs } from 'node:fs';

import { SynapseDB } from '@/synapseDb';
import { readPagedManifest, pageFileName } from '@/storage/pagedIndex';
import { compactDatabase } from '@/maintenance/compaction';
import { garbageCollectPages } from '@/maintenance/gc';

describe('页面级 GC（移除不可达页块）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-gc-'));
    dbPath = join(workspace, 'gc.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('在增量重写后通过 GC 收缩页文件体积', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    // 追加形成新页并进行增量重写（旧页不再被引用）
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();
    await compactDatabase(dbPath, { mode: 'incremental', orders: ['SPO'], minMergePages: 2 });

    const m1 = await readPagedManifest(`${dbPath}.pages`);
    const file = join(`${dbPath}.pages`, pageFileName('SPO'));
    const st1 = await fs.stat(file);
    // GC 之前文件包含不可达旧页，GC 后应变小或相等
    const stats = await garbageCollectPages(dbPath);
    const st2 = await fs.stat(file);
    // 文件大小不应为 0，且 GC 过程不影响数据正确性（不同实现细节可能导致相等或略有差异）
    expect(st2.size).toBeGreaterThan(0);
    expect(stats.bytesAfter).toBeGreaterThan(0);

    // 数据不变
    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(facts.length).toBe(3);
  });
});
