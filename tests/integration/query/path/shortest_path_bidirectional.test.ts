import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('最短路径（双向 BFS）', () => {
  let db: SynapseDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-bibfs-'));
    db = await SynapseDB.open(join(workspace, 'db.synapsedb'));
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('与单向 BFS 结果一致', async () => {
    // A->B->C->D->E 路径，另加 A->X->E 并行更短路径
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    db.addFact({ subject: 'B', predicate: 'R', object: 'C' });
    db.addFact({ subject: 'C', predicate: 'R', object: 'D' });
    db.addFact({ subject: 'D', predicate: 'R', object: 'E' });
    db.addFact({ subject: 'A', predicate: 'R', object: 'X' });
    db.addFact({ subject: 'X', predicate: 'R', object: 'E' });
    await db.flush();

    const p1 = db.shortestPath('A', 'E', { predicates: ['R'] });
    const p2 = db.shortestPathBidirectional('A', 'E', { predicates: ['R'] });

    expect(p1).not.toBeNull();
    expect(p2).not.toBeNull();
    expect(p1!.length).toBe(2); // A->X->E
    expect(p2!.length).toBe(2);
    const s1 = p1!.map((e) => `${e.subject}->${e.object}`).join(',');
    const s2 = p2!.map((e) => `${e.subject}->${e.object}`).join(',');
    expect(s2).toBe(s1);
  });
});
