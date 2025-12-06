import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('聚合函数框架 (B.3)', () => {
  let db: NervusDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-agg-'));
    db = await NervusDB.open(join(workspace, 'db.synapsedb'));
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('COUNT / GROUP BY 基础能力', async () => {
    db.addFact({ subject: 'alice', predicate: 'KNOWS', object: 'bob' });
    db.addFact({ subject: 'alice', predicate: 'KNOWS', object: 'carol' });
    db.addFact({ subject: 'bob', predicate: 'KNOWS', object: 'dave' });
    await db.flush();

    const result = db
      .aggregate()
      .match({ predicate: 'KNOWS' })
      .groupBy(['subject'])
      .count('friends')
      .execute();

    // alice 有 2 条 KNOWS，bob 有 1 条
    const bySubject: Record<string, number> = {};
    for (const row of result) {
      if (typeof row.subject === 'string') bySubject[row.subject] = Number(row.friends);
    }
    expect(bySubject['alice']).toBe(2);
    expect(bySubject['bob']).toBe(1);
  });

  it('SUM / AVG 针对属性字段', async () => {
    db.addFact(
      { subject: 'alice', predicate: 'RATED', object: 'item1' },
      { edgeProperties: { score: 4 } },
    );
    db.addFact(
      { subject: 'alice', predicate: 'RATED', object: 'item2' },
      { edgeProperties: { score: 5 } },
    );
    db.addFact(
      { subject: 'bob', predicate: 'RATED', object: 'item3' },
      { edgeProperties: { score: 3 } },
    );
    await db.flush();

    const result = db
      .aggregate()
      .match({ predicate: 'RATED' })
      .groupBy(['subject'])
      .sum('edgeProperties.score', 'total')
      .avg('edgeProperties.score', 'avg')
      .execute();

    const stat: Record<string, { total: number; avg: number }> = {};
    for (const row of result) {
      if (typeof row.subject === 'string')
        stat[row.subject] = { total: Number(row.total), avg: Number(row.avg) };
    }
    expect(stat['alice'].total).toBe(9);
    expect(Math.round(stat['alice'].avg * 10) / 10).toBe(4.5);
    expect(stat['bob'].total).toBe(3);
    expect(stat['bob'].avg).toBe(3);
  });

  it('MIN/MAX + ORDER BY + LIMIT', async () => {
    db.addFact({ subject: 'u1', predicate: 'E', object: 'o' }, { edgeProperties: { score: 10 } });
    db.addFact({ subject: 'u1', predicate: 'E', object: 'o2' }, { edgeProperties: { score: 30 } });
    db.addFact({ subject: 'u2', predicate: 'E', object: 'o3' }, { edgeProperties: { score: 20 } });
    await db.flush();

    const rows = db
      .aggregate()
      .match({ predicate: 'E' })
      .groupBy(['subject'])
      .min('edgeProperties.score', 'min')
      .max('edgeProperties.score', 'max')
      .orderBy('max', 'DESC')
      .limit(1)
      .execute();

    expect(rows).toHaveLength(1);
    // u1: min=10,max=30; u2: min=max=20 → 降序第一应为 u1
    expect(rows[0]['subject']).toBe('u1');
    expect(rows[0]['max']).toBe(30);
  });
});
