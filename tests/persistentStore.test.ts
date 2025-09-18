import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB, FactRecord } from '@/synapseDb';

describe('SynapseDB 持久化', () => {
  let workspace: string;
  let databasePath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-'));
    databasePath = join(workspace, 'brain.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  const toTripleKey = (fact: FactRecord) => ({
    subjectId: fact.subjectId,
    predicateId: fact.predicateId,
    objectId: fact.objectId,
  });

  it('首次写入会初始化文件并持久化三元组与属性', async () => {
    const db = await SynapseDB.open(databasePath);

    const persisted = db.addFact(
      {
        subject: 'file:/src/user.ts',
        predicate: 'DEFINES',
        object: 'class:User',
      },
      {
        subjectProperties: { type: 'File', lines: 120 },
        objectProperties: { type: 'Class', methods: 5 },
        edgeProperties: { confidence: 0.92 },
      },
    );

    await db.flush();

    const reopened = await SynapseDB.open(databasePath);
    const facts = reopened.listFacts();

    expect(facts).toHaveLength(1);
    expect(facts[0].subject).toBe('file:/src/user.ts');
    expect(facts[0].predicate).toBe('DEFINES');
    expect(facts[0].object).toBe('class:User');
    expect(facts[0].subjectProperties).toEqual({ type: 'File', lines: 120 });
    expect(facts[0].objectProperties).toEqual({ type: 'Class', methods: 5 });
    expect(facts[0].edgeProperties).toEqual({ confidence: 0.92 });

    const properties = reopened.getEdgeProperties<{ confidence: number }>(toTripleKey(persisted));
    expect(properties?.confidence).toBeCloseTo(0.92, 2);

    await reopened.flush();
  });

  it('重复写入复用字典 ID，支持增量刷新', async () => {
    const db = await SynapseDB.open(databasePath);

    const factA = db.addFact({
      subject: 'file:/src/index.ts',
      predicate: 'CONTAINS',
      object: 'function:init',
    });

    const factB = db.addFact({
      subject: 'file:/src/index.ts',
      predicate: 'CONTAINS',
      object: 'function:bootstrap',
    });

    expect(factA.subjectId).toBe(factB.subjectId);
    expect(factA.predicateId).toBe(factB.predicateId);
    expect(factA.objectId).not.toBe(factB.objectId);

    await db.flush();

    const reopened = await SynapseDB.open(databasePath);
    const ids = reopened.listFacts().map((fact): number => fact.subjectId);
    const uniqueIds = new Set<number>();
    ids.forEach((id: number) => uniqueIds.add(id));
    expect(uniqueIds.size).toBe(1);
    await reopened.flush();
  });
});
