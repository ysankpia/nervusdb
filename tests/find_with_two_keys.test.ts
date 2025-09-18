import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('find 支持双键（s+o / p+o）命中', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-find2-'));
    dbPath = join(workspace, 'db.synapsedb');
  });
  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('s+o 查询可命中结果（SOP 顺序）', async () => {
    const db = await SynapseDB.open(dbPath);
    db.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    db.addFact({ subject: 'S', predicate: 'R2', object: 'O2' });
    await db.flush();

    const res = db.find({ subject: 'S', object: 'O' }).all();
    expect(res).toHaveLength(1);
    expect(res[0].predicate).toBe('R');
  });

  it('p+o 查询可命中结果（POS 顺序）', async () => {
    const db = await SynapseDB.open(dbPath);
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    db.addFact({ subject: 'A2', predicate: 'R', object: 'C' });
    await db.flush();

    const res = db.find({ predicate: 'R', object: 'C' }).all();
    expect(res).toHaveLength(1);
    expect(res[0].subject).toBe('A2');
  });
});
