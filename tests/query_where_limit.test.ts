import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

async function createDatabase(): Promise<{ db: SynapseDB; path: string; workspace: string }> {
  const workspace = await mkdtemp(join(tmpdir(), 'synapsedb-where-'));
  const path = join(workspace, 'where.synapsedb');
  const db = await SynapseDB.open(path);
  return { db, path, workspace };
}

describe('QueryBuilder where/limit', () => {
  let workspace: string;
  let db: SynapseDB;

  beforeEach(async () => {
    const env = await createDatabase();
    workspace = env.workspace;
    db = env.db;
  });

  afterEach(async () => {
    await db.flush();
    await rm(workspace, { recursive: true, force: true });
  });

  it('where 过滤边属性', () => {
    const a = db.addFact(
      { subject: 'S', predicate: 'R', object: 'O1' },
      { edgeProperties: { conf: 0.8 } },
    );
    const b = db.addFact(
      { subject: 'S', predicate: 'R', object: 'O2' },
      { edgeProperties: { conf: 0.2 } },
    );
    expect(a.object).toBe('O1');
    expect(b.object).toBe('O2');

    const results = db
      .find({ subject: 'S', predicate: 'R' })
      .where((f) => (f.edgeProperties as { conf?: number } | undefined)?.conf! >= 0.5)
      .all();
    expect(results).toHaveLength(1);
    expect(results[0].object).toBe('O1');
  });

  it('limit 限制结果集并影响后续联想的前沿', () => {
    db.addFact({ subject: 'A', predicate: 'LINK', object: 'B1' });
    db.addFact({ subject: 'A', predicate: 'LINK', object: 'B2' });
    db.addFact({ subject: 'B1', predicate: 'LINK', object: 'C1' });
    db.addFact({ subject: 'B2', predicate: 'LINK', object: 'C2' });

    const limited = db
      .find({ subject: 'A', predicate: 'LINK' })
      .limit(1)
      // 重新锚定到对象侧，使后续正向扩展从 B* 出发
      .anchor('object')
      .follow('LINK')
      .all();

    expect(limited).toHaveLength(1);
    const target = limited[0].object;
    expect(['C1', 'C2']).toContain(target);
  });
});
