import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('加权最短路径（Dijkstra）', () => {
  let db: SynapseDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-spw-'));
    db = await SynapseDB.open(join(workspace, 'db.synapsedb'));
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('依据边权重选择更短路径', async () => {
    // 两条路径：S->A->T (权重 10+1)，S->B->T (权重 2+2) → 选择经 B 的路径
    db.addFact({ subject: 'S', predicate: 'R', object: 'A' }, { edgeProperties: { weight: 10 } });
    db.addFact({ subject: 'A', predicate: 'R', object: 'T' }, { edgeProperties: { weight: 1 } });
    db.addFact({ subject: 'S', predicate: 'R', object: 'B' }, { edgeProperties: { weight: 2 } });
    db.addFact({ subject: 'B', predicate: 'R', object: 'T' }, { edgeProperties: { weight: 2 } });
    await db.flush();

    const path = db.shortestPathWeighted('S', 'T', { predicate: 'R', weightProperty: 'weight' });
    expect(path).not.toBeNull();
    const pairs = path!.map((e) => `${e.subject}->${e.object}`).join(',');
    expect(pairs).toBe('S->B,B->T');
  });
});
