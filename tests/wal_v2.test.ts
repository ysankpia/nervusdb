import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('WAL v2 批次提交语义', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-walv2-'));
    dbPath = join(workspace, 'walv2.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('未提交的批次不会在重启后生效', async () => {
    const db1 = await SynapseDB.open(dbPath);
    db1.beginBatch();
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    // 未调用 commitBatch，模拟崩溃：不 flush，直接重开

    const db2 = await SynapseDB.open(dbPath);
    const results = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(results).toHaveLength(0);
    await db2.flush();
  });

  it('提交后的批次在重启后可恢复', async () => {
    const db1 = await SynapseDB.open(dbPath);
    db1.beginBatch();
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    db1.commitBatch();
    // 不调用 flush，模拟崩溃重启

    const db2 = await SynapseDB.open(dbPath);
    const results = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(results).toHaveLength(1);
    await db2.flush();
  });
});

