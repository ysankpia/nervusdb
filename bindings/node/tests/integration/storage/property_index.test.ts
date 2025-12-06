import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rm } from 'node:fs/promises';

describe('属性索引 · 基础能力', () => {
  const dbPath = join(tmpdir(), `prop-index-basic-${Date.now()}.synapsedb`);
  let db: NervusDB;

  beforeAll(async () => {
    db = await NervusDB.open(dbPath, { rebuildIndexes: true });
    // 构造少量带属性的数据
    db.beginBatch();
    db.addFact(
      { subject: 'user:1', predicate: 'worksAt', object: 'company:A' },
      {
        subjectProperties: { name: 'u1', age: 30, dept: 'd1' },
        objectProperties: { name: 'A', industry: 'tech' },
        edgeProperties: { role: 'dev', weight: 0.8 },
      },
    );
    db.addFact(
      { subject: 'user:2', predicate: 'worksAt', object: 'company:A' },
      {
        subjectProperties: { name: 'u2', age: 26, dept: 'd2' },
        objectProperties: { name: 'A', industry: 'tech' },
        edgeProperties: { role: 'qa', weight: 0.5 },
      },
    );
    db.addFact(
      { subject: 'user:3', predicate: 'worksAt', object: 'company:B' },
      {
        subjectProperties: { name: 'u3', age: 34, dept: 'd1' },
        objectProperties: { name: 'B', industry: 'finance' },
        edgeProperties: { role: 'dev', weight: 0.7 },
      },
    );
    db.commitBatch();
    await db.flush();
  });

  afterAll(async () => {
    await db.close();
    await rm(dbPath, { force: true });
    await rm(`${dbPath}.pages`, { force: true, recursive: true });
    await rm(`${dbPath}.wal`, { force: true });
  });

  it('基于节点属性的等值与范围查询', () => {
    const age30 = db.findByNodeProperty({ propertyName: 'age', value: 30 }).all();
    expect(age30.length).toBeGreaterThan(0);

    const ageRange = db
      .findByNodeProperty({
        propertyName: 'age',
        range: { min: 25, max: 32, includeMin: true, includeMax: true },
      })
      .all();
    // user:1, user:2 满足
    expect(ageRange.length).toBeGreaterThanOrEqual(2);
  });

  it('基于边属性的等值查询', () => {
    const devEdges = db.findByEdgeProperty({ propertyName: 'role', value: 'dev' }).all();
    expect(devEdges.length).toBeGreaterThan(0);
  });

  it('链式联想 + 节点属性过滤（whereNodeProperty）', () => {
    // 先找 tech 行业的公司，再反向找员工，并按年龄过滤
    const results = db
      .findByNodeProperty({ propertyName: 'industry', value: 'tech' }, { anchor: 'object' })
      .followReverse('worksAt')
      .whereNodeProperty({ propertyName: 'age', range: { min: 26, max: 32 } })
      .all();
    expect(results.length).toBeGreaterThan(0);
  });
});
