import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('查询增强：UNION/最短路径/Cypher', () => {
  let db: SynapseDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-qe-'));
    db = await SynapseDB.open(join(workspace, 'db.synapsedb'), {
      experimental: { cypher: true },
    });
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('UNION/UNION ALL', async () => {
    db.addFact({ subject: 'A', predicate: 'R', object: '1' });
    db.addFact({ subject: 'B', predicate: 'R', object: '2' });
    db.addFact({ subject: 'C', predicate: 'S', object: '3' });
    await db.flush();

    const q1 = db.find({ predicate: 'R' });
    const q2 = db.find({ predicate: 'S' });

    const unionRes = q1.union(q2).all();
    expect(unionRes).toHaveLength(3);

    const unionAllRes = q1.unionAll(q1).all();
    expect(unionAllRes).toHaveLength(4); // R 两条，UNION ALL 叠加为4
  });

  it('最短路径 BFS', async () => {
    db.addFact({ subject: 'A', predicate: 'LINK', object: 'B' });
    db.addFact({ subject: 'B', predicate: 'LINK', object: 'C' });
    db.addFact({ subject: 'C', predicate: 'LINK', object: 'D' });
    await db.flush();

    const path = db.shortestPath('A', 'D', { predicates: ['LINK'] });
    expect(path).not.toBeNull();
    expect(path!.length).toBe(3);
  });

  it('Cypher 最小子集', async () => {
    db.addFact({ subject: 'alice', predicate: 'KNOWS', object: 'bob' });
    await db.flush();

    const result = await db.cypher('MATCH (a)-[:KNOWS]->(b) RETURN a,b');
    expect(result.records.length).toBe(1);
    expect(result.records[0]['a']).toBe('alice');
    expect(result.records[0]['b']).toBe('bob');
  });
});
