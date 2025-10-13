import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('模式匹配（多段 + 标签 + 属性）', () => {
  let db: NervusDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-pattern-'));
    db = await NervusDB.open(join(workspace, 'db.synapsedb'));
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('node-edge-node-edge-node 链式匹配，结合标签与属性过滤', async () => {
    // 数据：p(Person, age) -WORKS_AT-> c(Company) -LOCATED_IN-> city
    db.addFact(
      { subject: 'alice', predicate: 'WORKS_AT', object: 'acme' },
      { subjectProperties: { labels: ['Person'], age: 30 } },
    );
    db.addFact(
      { subject: 'bob', predicate: 'WORKS_AT', object: 'acme' },
      { subjectProperties: { labels: ['Person'], age: 22 } },
    );
    db.addFact(
      { subject: 'acme', predicate: 'LOCATED_IN', object: 'beijing' },
      { subjectProperties: { labels: ['Company'] } },
    );
    await db.flush();

    const rows = await db
      .match()
      .node('p', ['Person'])
      .edge('->', 'WORKS_AT')
      .node('c', ['Company'])
      .edge('->', 'LOCATED_IN')
      .node('city')
      .whereNodeProperty('p', 'age', '>=', 25)
      .return(['p', 'c'])
      .execute();

    expect(rows.length).toBe(1);
    expect(rows[0]['p']).toBe('alice');
    expect(rows[0]['c']).toBe('acme');
  });
});
