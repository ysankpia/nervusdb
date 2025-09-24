import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('变长路径高级能力（唯一性与最短路径）', () => {
  let db: SynapseDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-vpath-'));
    db = await SynapseDB.open(join(workspace, 'db.synapsedb'));
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('NODE 唯一性：不重复节点', async () => {
    // A->B->C->A 构成环
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    db.addFact({ subject: 'B', predicate: 'R', object: 'C' });
    db.addFact({ subject: 'C', predicate: 'R', object: 'A' });
    await db.flush();

    const builder = db
      .find({ subject: 'A' })
      .variablePath('R', { min: 1, max: 3, uniqueness: 'NODE' });
    const paths = builder.all();
    expect(paths.length).toBeGreaterThan(0);
    // 检查每条路径不重复节点
    for (const p of paths) {
      const nodes: number[] = [];
      let cur = p.startId;
      for (const e of p.edges) {
        const next = e.record.objectId;
        nodes.push(cur);
        cur = next;
      }
      nodes.push(cur);
      const set = new Set(nodes);
      expect(set.size).toBe(nodes.length);
    }
  });

  it('NONE 唯一性：允许环路', async () => {
    db.addFact({ subject: 'X', predicate: 'R', object: 'Y' });
    db.addFact({ subject: 'Y', predicate: 'R', object: 'X' });
    await db.flush();

    const paths = db
      .find({ subject: 'X' })
      .variablePath('R', { min: 2, max: 3, uniqueness: 'NONE' })
      .all();
    // 允许 X->Y->X 的环路存在
    expect(paths.some((p) => p.length >= 2)).toBe(true);
  });

  it('最短路径（基于 variablePath.shortest）', async () => {
    db.addFact({ subject: 'S', predicate: 'R', object: 'A' });
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    db.addFact({ subject: 'B', predicate: 'R', object: 'T' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'T' }); // 直接边，最短为1
    await db.flush();

    const store = (db as any).store;
    const tId = store.getNodeIdByValue('T');
    const shortest = db.find({ subject: 'S' }).variablePath('R', { max: 4 }).shortest(tId);
    expect(shortest).not.toBeNull();
    expect(shortest!.length).toBe(1);
  });
});
