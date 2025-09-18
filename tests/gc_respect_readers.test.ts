import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { compactDatabase } from '@/maintenance/compaction';
import { garbageCollectPages } from '@/maintenance/gc';
import { addReader } from '@/storage/readerRegistry';

describe('GC 尊重有效读者', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-gc-rdr-'));
    dbPath = join(workspace, 'gcr.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('存在读者注册时，开启 respect-readers 的 GC 将跳过', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();
    await compactDatabase(dbPath, { mode: 'incremental', orders: ['SPO'], minMergePages: 2 });

    // 模拟读者注册
    await addReader(`${dbPath}.pages`, { pid: 12345, epoch: 0, ts: Date.now() });
    const stats = await garbageCollectPages(dbPath, { respectReaders: true });
    expect(stats.skipped).toBe(true);
  });
});

