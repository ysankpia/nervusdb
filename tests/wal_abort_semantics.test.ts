import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('WAL ABORT 语义测试', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-abort-'));
    dbPath = join(workspace, 'abort.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('单一批次 ABORT 后重启时数据不应生效', async () => {
    const db = await SynapseDB.open(dbPath);

    // 先提交一些基础数据
    db.beginBatch({ txId: 'base-data' });
    db.addFact({ subject: 'BaseS', predicate: 'BaseR', object: 'BaseO' });
    db.commitBatch();

    // 开始需要中止的批次
    db.beginBatch({ txId: 'to-abort' });
    db.addFact({ subject: 'AbortS1', predicate: 'AbortR', object: 'AbortO1' });
    db.addFact({ subject: 'AbortS2', predicate: 'AbortR', object: 'AbortO2' });
    db.abortBatch(); // 中止批次

    await db.close();

    // 重新打开数据库，验证 ABORT 的数据在 WAL 重放时不生效
    const db2 = await SynapseDB.open(dbPath);

    const baseResults = db2.find({ predicate: 'BaseR' }).all();
    expect(baseResults).toHaveLength(1);
    expect(baseResults[0].subject).toBe('BaseS');

    // ABORT 的数据在重启后不应存在
    const abortResults = db2.find({ predicate: 'AbortR' }).all();
    expect(abortResults).toHaveLength(0);

    await db2.close();
  });

  it('嵌套批次部分 ABORT（重启验证）', async () => {
    const db = await SynapseDB.open(dbPath);

    // 外层批次开始
    db.beginBatch({ txId: 'outer-1' });
    db.addFact({ subject: 'Outer1', predicate: 'OuterR', object: 'OuterO1' });

    // 内层批次1（将被提交）
    db.beginBatch({ txId: 'inner-1' });
    db.addFact({ subject: 'Inner1', predicate: 'InnerR', object: 'InnerO1' });
    db.commitBatch();

    // 内层批次2（将被中止）
    db.beginBatch({ txId: 'inner-2' });
    db.addFact({ subject: 'Inner2', predicate: 'InnerR', object: 'InnerO2' });
    db.abortBatch(); // 中止内层批次2

    // 外层批次提交
    db.addFact({ subject: 'Outer2', predicate: 'OuterR', object: 'OuterO2' });
    db.commitBatch();

    await db.close();

    // 重启验证：内层批次2被ABORT，其数据不应在重放时恢复
    const db2 = await SynapseDB.open(dbPath);

    const outerResults = db2.find({ predicate: 'OuterR' }).all();
    expect(outerResults).toHaveLength(2);

    // 只有内层1应该存在，内层2被ABORT不应恢复
    const innerResults = db2.find({ predicate: 'InnerR' }).all();
    expect(innerResults).toHaveLength(1);
    expect(innerResults[0].subject).toBe('Inner1');

    await db2.close();
  });

  it('ABORT 后重启恢复验证', async () => {
    // 第一阶段：提交部分数据，中止部分数据
    {
      const db = await SynapseDB.open(dbPath);

      // 提交的数据
      db.beginBatch({ txId: 'committed-data' });
      db.addFact({ subject: 'Committed', predicate: 'R', object: 'O1' });
      db.commitBatch();

      // 中止的数据
      db.beginBatch({ txId: 'aborted-data' });
      db.addFact({ subject: 'Aborted1', predicate: 'R', object: 'O2' });
      db.addFact({ subject: 'Aborted2', predicate: 'R', object: 'O3' });
      db.abortBatch();

      // 再次提交数据
      db.beginBatch({ txId: 'committed-data-2' });
      db.addFact({ subject: 'Committed2', predicate: 'R', object: 'O4' });
      db.commitBatch();

      await db.close();
    }

    // 第二阶段：重启验证，中止的数据不应恢复
    {
      const db2 = await SynapseDB.open(dbPath);

      const results = db2.find({ predicate: 'R' }).all();
      expect(results).toHaveLength(2);

      const subjects = results.map((r) => r.subject);
      expect(subjects).toContain('Committed');
      expect(subjects).toContain('Committed2');
      expect(subjects).not.toContain('Aborted1');
      expect(subjects).not.toContain('Aborted2');

      await db2.close();
    }
  });

  it('ABORT 对属性操作的影响', async () => {
    const db = await SynapseDB.open(dbPath);

    // 先添加一个事实以获取节点ID
    db.addFact({ subject: 'TestNode', predicate: 'type', object: 'Node' });
    await db.flush();

    const facts = db.find({ subject: 'TestNode' }).all();
    expect(facts).toHaveLength(1);
    const nodeId = facts[0].subjectId;

    // 开始批次并设置属性
    db.beginBatch({ txId: 'prop-batch' });
    db.setNodeProperties(nodeId, { name: 'TestName', value: 42 });

    // 验证批次内属性可见
    const propsInBatch = db.getNodeProperties(nodeId);
    expect(propsInBatch).toEqual({ name: 'TestName', value: 42 });

    // 中止批次
    db.abortBatch();

    // 验证属性已回滚
    const propsAfterAbort = db.getNodeProperties(nodeId);
    expect(propsAfterAbort).toBeNull();

    await db.close();
  });

  it('混合操作 ABORT 测试', async () => {
    const db = await SynapseDB.open(dbPath);

    // 先建立一些基础数据
    db.addFact({ subject: 'S1', predicate: 'R1', object: 'O1' });
    await db.flush();
    const nodeId = db.find({ subject: 'S1' }).all()[0].subjectId;

    // 开始复合操作批次
    db.beginBatch({ txId: 'mixed-ops' });

    // 添加事实
    db.addFact({ subject: 'S2', predicate: 'R2', object: 'O2' });
    db.addFact({ subject: 'S3', predicate: 'R3', object: 'O3' });

    // 删除事实
    db.deleteFact({ subject: 'S1', predicate: 'R1', object: 'O1' });

    // 设置属性
    db.setNodeProperties(nodeId, { deleted: true });

    // 验证批次内的状态
    const allFactsInBatch = db.find({}).all();
    const nodePropsInBatch = db.getNodeProperties(nodeId);

    // 中止整个批次
    db.abortBatch();

    // 验证所有操作都被回滚
    const factsAfterAbort = db.find({}).all();
    expect(factsAfterAbort).toHaveLength(1); // 只有原始的 S1-R1-O1
    expect(factsAfterAbort[0].subject).toBe('S1');

    const nodePropsAfterAbort = db.getNodeProperties(nodeId);
    expect(nodePropsAfterAbort).toBeNull();

    await db.close();
  });

  it('大量数据 ABORT 性能测试', async () => {
    const db = await SynapseDB.open(dbPath);

    const startTime = Date.now();

    db.beginBatch({ txId: 'large-abort' });

    // 添加大量数据
    for (let i = 0; i < 1000; i++) {
      db.addFact({ subject: `S${i}`, predicate: 'bulk', object: `O${i}` });
    }

    // 中止大批次
    db.abortBatch();

    const endTime = Date.now();
    const duration = endTime - startTime;

    // ABORT 应该很快完成（阈值进一步放宽以适配不同机器/FS）
    expect(duration).toBeLessThan(5000); // 不应超过5秒

    // 验证没有数据被提交
    const results = db.find({ predicate: 'bulk' }).all();
    expect(results).toHaveLength(0);

    await db.close();
  });
});
