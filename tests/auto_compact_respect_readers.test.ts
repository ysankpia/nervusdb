import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';
import { addReader } from '@/storage/readerRegistry';

describe('Auto-Compact 尊重读者', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-auto-respect-'));
    dbPath = join(workspace, 'ar.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('存在读者时，respect-readers 的 auto-compact 返回 skipped', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    await db.flush();
    await addReader(`${dbPath}.pages`, { pid: 99999, epoch: 0, ts: Date.now() });

    const decision = await autoCompact(dbPath, { mode: 'incremental', orders: ['SPO'], minMergePages: 2, respectReaders: true });
    expect(decision.skipped).toBe(true);
  });
});

