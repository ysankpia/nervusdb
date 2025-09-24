import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';

describe('WAL 嵌套事务问题重现', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-nested-bug-'));
    dbPath = join(workspace, 'bug.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('重现问题：内层COMMIT后外层ABORT不应该影响已提交的内层事务', async () => {
    const db = await SynapseDB.open(dbPath);

    // 外层事务
    db.beginBatch({ txId: 'outer' });

    // 内层事务1：提交
    db.beginBatch({ txId: 'inner-commit' });
    db.addFact({ subject: 'InnerCommitted', predicate: 'test', object: 'should-survive' });
    db.commitBatch(); // 这个应该永久生效

    // 内层事务2：中止
    db.beginBatch({ txId: 'inner-abort' });
    db.addFact({ subject: 'InnerAborted', predicate: 'test', object: 'should-die' });
    db.abortBatch(); // 这个应该被丢弃

    // 外层事务中止
    db.abortBatch(); // 这应该只影响外层，不应该影响已提交的内层事务

    await db.close();

    // 重启验证
    const db2 = await SynapseDB.open(dbPath);
    // 合并“predicate=test”的结果与“object=test”的基线记录，确保同时验证两类数据
    const results = db2
      .find({ predicate: 'test' })
      .union(db2.find({ object: 'test' }))
      .all();
    const subjects = results.map((r) => r.subject);

    console.log('实际结果:', subjects);

    // 期望：InnerCommitted 应该存在，InnerAborted 不应该存在
    expect(subjects).toContain('InnerCommitted');
    expect(subjects).not.toContain('InnerAborted');

    await db2.close();
  });

  it('验证WAL重放是否正确处理嵌套事务', async () => {
    const db = await SynapseDB.open(dbPath);

    // 先创建一些基础数据
    db.addFact({ subject: 'Base', predicate: 'type', object: 'test' });
    await db.flush();

    // 开始嵌套事务但不flush，依赖WAL重放
    db.beginBatch({ txId: 'outer' });

    db.addFact({ subject: 'OuterAdd', predicate: 'test', object: 'outer' });

    db.beginBatch({ txId: 'inner-commit' });
    db.addFact({ subject: 'InnerCommitted', predicate: 'test', object: 'inner' });
    db.commitBatch();

    db.beginBatch({ txId: 'inner-abort' });
    db.addFact({ subject: 'InnerAborted', predicate: 'test', object: 'abort' });
    db.abortBatch();

    db.addFact({ subject: 'OuterFinal', predicate: 'test', object: 'final' });
    db.commitBatch();

    // 不flush，直接关闭
    await db.close();

    // 重启，依赖WAL重放
    const db2 = await SynapseDB.open(dbPath);
    const results = db2
      .find({ predicate: 'test' })
      .union(db2.find({ object: 'test' }))
      .all();
    const subjects = results.map((r) => r.subject);

    console.log('WAL重放结果:', subjects);

    // 期望：Base, InnerCommitted, OuterAdd, OuterFinal
    expect(subjects).toContain('Base');
    expect(subjects).toContain('InnerCommitted');
    expect(subjects).toContain('OuterAdd');
    expect(subjects).toContain('OuterFinal');
    expect(subjects).not.toContain('InnerAborted');

    await db2.close();
  });
});
