import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('删除与属性更新', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-delupd-'));
    dbPath = join(workspace, 'db.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('逻辑删除后查询不再返回目标三元组（含分页与暂存合并）', async () => {
    const db = await SynapseDB.open(dbPath);
    db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    await db.flush();

    expect(db.find({ subject: 'A', predicate: 'R' }).all()).toHaveLength(1);

    db.deleteFact({ subject: 'A', predicate: 'R', object: 'B' });
    expect(db.find({ subject: 'A', predicate: 'R' }).all()).toHaveLength(0);

    await db.flush();
    // 重启后 tombstones 从 manifest 恢复
    const db2 = await SynapseDB.open(dbPath);
    expect(db2.find({ subject: 'A', predicate: 'R' }).all()).toHaveLength(0);
  });

  it('节点与边属性更新返回最新值', async () => {
    const db = await SynapseDB.open(dbPath);
    const fact = db.addFact({ subject: 'S', predicate: 'R', object: 'O' });
    db.setNodeProperties(fact.subjectId, { v: 1 });
    db.setEdgeProperties(
      { subjectId: fact.subjectId, predicateId: fact.predicateId, objectId: fact.objectId },
      { e: 'x' },
    );
    await db.flush();

    const db2 = await SynapseDB.open(dbPath);
    const f = db2.find({ subject: 'S', predicate: 'R' }).all()[0];
    expect(f.subjectProperties).toEqual({ v: 1 });
    expect(f.edgeProperties).toEqual({ e: 'x' });

    db2.setNodeProperties(f.subjectId, { v: 2 });
    await db2.flush();
    const db3 = await SynapseDB.open(dbPath);
    const f2 = db3.find({ subject: 'S', predicate: 'R' }).all()[0];
    expect(f2.subjectProperties).toEqual({ v: 2 });
  });
});
