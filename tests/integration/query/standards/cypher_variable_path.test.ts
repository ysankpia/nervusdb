import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('Cypher 最小子集（变长路径）', () => {
  let db: SynapseDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-cypher-'));
    db = await SynapseDB.open(join(workspace, 'db.synapsedb'), {
      experimental: { cypher: true },
    });
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('MATCH (a)-[:R*2..3]->(b) RETURN a,b', async () => {
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    db.addFact({ subject: 'B', predicate: 'R', object: 'C' });
    db.addFact({ subject: 'C', predicate: 'R', object: 'D' });
    await db.flush();

    const result = await db.cypher('MATCH (x)-[:R*2..3]->(y) RETURN x,y');
    // 应至少包含 (A->C), (B->D)
    const pairs = result.records.map((r) => `${r['x']}->${r['y']}`).sort();
    expect(pairs).toContain('A->C');
    expect(pairs).toContain('B->D');
  });
});
