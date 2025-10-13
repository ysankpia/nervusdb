import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

describe('节点标签系统 (B.1)', () => {
  let db: NervusDB;
  let workspace: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-labels-'));
    db = await NervusDB.open(join(workspace, 'db.synapsedb'));
  });

  afterEach(async () => {
    await db.close();
    await rm(workspace, { recursive: true, force: true });
  });

  it('findByLabel 与 whereLabel 基本用法', async () => {
    // 写入节点与标签
    db.addFact(
      { subject: 'alice', predicate: 'KNOWS', object: 'bob' },
      { subjectProperties: { labels: ['Person', 'Developer'] } },
    );
    db.addFact(
      { subject: 'bob', predicate: 'LIKES', object: 'coffee' },
      { subjectProperties: { labels: ['Person'] } },
    );
    await db.flush();

    // 通过标签查找作为主语的节点并沿边联想
    const knowsFromPerson = db
      .findByLabel('Person', { anchor: 'subject' })
      .whereLabel('Person', { on: 'subject' })
      .follow('KNOWS')
      .all();
    expect(knowsFromPerson).toHaveLength(1);
    expect(knowsFromPerson[0].subject).toBe('alice');
    expect(knowsFromPerson[0].object).toBe('bob');

    // 直接在查询结果上按标签过滤
    const personLikes = db
      .find({ predicate: 'LIKES' })
      .whereLabel('Person', { on: 'subject' })
      .all();
    expect(personLikes).toHaveLength(1);
    expect(personLikes[0].subject).toBe('bob');
  });
});
