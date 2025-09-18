import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

async function createDatabase(): Promise<{ db: SynapseDB; path: string; workspace: string }> {
  const workspace = await mkdtemp(join(tmpdir(), 'synapsedb-query-'));
  const path = join(workspace, 'query.synapsedb');
  const db = await SynapseDB.open(path);
  return { db, path, workspace };
}

describe('QueryBuilder 联想查询', () => {
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

  it('找不到节点时返回空查询集', () => {
    const result = db.find({ subject: 'unknown:node' }).all();
    expect(result).toHaveLength(0);
  });

  it('支持按主语与谓语定位事实', () => {
    db.addFact({
      subject: 'class:User',
      predicate: 'HAS_METHOD',
      object: 'method:login',
    });

    const matches = db.find({ subject: 'class:User', predicate: 'HAS_METHOD' }).all();
    expect(matches).toHaveLength(1);
    expect(matches[0].object).toBe('method:login');
  });

  it('支持多跳 follow 与 followReverse 联想', () => {
    db.addFact({
      subject: 'file:/src/user.ts',
      predicate: 'DEFINES',
      object: 'class:User',
    });
    db.addFact({
      subject: 'class:User',
      predicate: 'HAS_METHOD',
      object: 'method:login',
    });
    db.addFact({
      subject: 'commit:abc123',
      predicate: 'MODIFIES',
      object: 'file:/src/user.ts',
    });
    db.addFact({
      subject: 'commit:abc123',
      predicate: 'AUTHOR_OF',
      object: 'person:alice',
    });

    const authors = db
      .find({ object: 'method:login' })
      .followReverse('HAS_METHOD')
      .followReverse('DEFINES')
      .followReverse('MODIFIES')
      .follow('AUTHOR_OF')
      .all();

    expect(authors).toHaveLength(1);
    expect(authors[0].object).toBe('person:alice');
  });

  it('支持 anchor 配置聚焦主语集合', () => {
    db.addFact({
      subject: 'file:/src/index.ts',
      predicate: 'CONTAINS',
      object: 'function:init',
    });
    db.addFact({
      subject: 'file:/src/index.ts',
      predicate: 'CONTAINS',
      object: 'function:bootstrap',
    });

    const results = db.find({ subject: 'file:/src/index.ts' }, { anchor: 'subject' }).all();
    expect(results).toHaveLength(2);
  });
});
