import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rm } from 'node:fs/promises';

describe('属性索引 · 集成流 (写入→查询→更新)', () => {
  const dbPath = join(tmpdir(), `prop-index-int-${Date.now()}.synapsedb`);
  let db: SynapseDB;

  beforeAll(async () => {
    db = await SynapseDB.open(dbPath, { rebuildIndexes: true });
  });

  afterAll(async () => {
    await db.close();
    await rm(dbPath, { force: true });
    await rm(`${dbPath}.pages`, { force: true, recursive: true });
    await rm(`${dbPath}.wal`, { force: true });
  });

  it('写入后可按属性检索，属性更新后索引生效', async () => {
    // 写入
    const fact = db.addFact(
      { subject: 'emp:1', predicate: 'worksAt', object: 'co:X' },
      { subjectProperties: { age: 28, level: 'P5' } },
    );
    await db.flush();

    // 按属性查询
    const byAge28 = db.findByNodeProperty({ propertyName: 'age', value: 28 }).all();
    expect(byAge28.length).toBeGreaterThan(0);

    // 更新属性，索引应同步更新
    db.setNodeProperties(fact.subjectId, { age: 29, level: 'P6' });
    await db.flush();

    const byAge28After = db.findByNodeProperty({ propertyName: 'age', value: 28 }).all();
    const byAge29After = db.findByNodeProperty({ propertyName: 'age', value: 29 }).all();
    expect(byAge28After.length).toBe(0);
    expect(byAge29After.length).toBeGreaterThan(0);

    // 集成链路：按公司反向联想到员工，再按等级过滤
    const results = db
      .find({ object: 'co:X', predicate: 'worksAt' }, { anchor: 'subject' })
      .whereNodeProperty({ propertyName: 'level', value: 'P6' })
      .all();
    expect(results.length).toBeGreaterThan(0);
  });
});
