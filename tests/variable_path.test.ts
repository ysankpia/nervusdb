import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('变长路径查询 (B.2)', () => {
  let db: SynapseDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-path-'));
    db = await SynapseDB.open(join(workspace, 'db.synapsedb'));
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('followPath 支持 [min..max] 范围', async () => {
    // A->B->C->D->E 链
    db.addFact({ subject: 'A', predicate: 'LINK', object: 'B' });
    db.addFact({ subject: 'B', predicate: 'LINK', object: 'C' });
    db.addFact({ subject: 'C', predicate: 'LINK', object: 'D' });
    db.addFact({ subject: 'D', predicate: 'LINK', object: 'E' });
    await db.flush();

    const res = db.find({ subject: 'A' }).followPath('LINK', { min: 2, max: 3 }).all();

    // 应包含第二跳(B->C)和第三跳(C->D)两条边
    expect(res).toHaveLength(2);
    const edges = res.map((r) => `${r.subject}->${r.object}`).sort();
    expect(edges).toEqual(['B->C', 'C->D']);
  });
});
