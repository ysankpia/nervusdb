import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('WAL 恢复', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-wal-'));
    dbPath = join(workspace, 'wal.synapsedb');
  });
  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('未 flush 的写入可通过 WAL 重放恢复', async () => {
    const db1 = await SynapseDB.open(dbPath);
    db1.addFact({ subject: 'class:User', predicate: 'HAS_METHOD', object: 'method:login' });
    // 模拟崩溃：不调用 flush，直接新开一个实例

    const db2 = await SynapseDB.open(dbPath);
    const facts = db2.find({ subject: 'class:User', predicate: 'HAS_METHOD' }).all();
    expect(facts).toHaveLength(1);
    expect(facts[0].object).toBe('method:login');
    await db2.flush();
  });
});
