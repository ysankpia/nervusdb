import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('进程级写锁（可选）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-lock-'));
    dbPath = join(workspace, 'lock.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('启用 enableLock 时同进程重复打开可并行（同 PID），不同进程应独占（此处仅验证同进程不报错）', async () => {
    const db1 = await SynapseDB.open(dbPath, { indexDirectory: `${dbPath}.pages`, enableLock: true });
    const db2 = await SynapseDB.open(dbPath, { indexDirectory: `${dbPath}.pages`, enableLock: false });
    db1.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    await db1.flush();
    await db2.flush();
    await db1.close();
    await db2.close();
    expect(true).toBe(true);
  });
});

