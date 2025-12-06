import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PersistentStore } from '../../../src/core/storage/persistentStore.js';

describe('PropertyDataStore 集成测试 - Issue #7', () => {
  let testDir: string;
  let dbPath: string;

  beforeEach(async () => {
    testDir = await mkdtemp(join(tmpdir(), 'property-integration-'));
    dbPath = join(testDir, 'test.synapsedb');
  });

  afterEach(async () => {
    try {
      await rm(testDir, { recursive: true, force: true });
    } catch {}
  });

  it('should migrate properties from main file to PropertyDataStore on first flush', async () => {
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });

    // 插入事实
    const fact1 = store1.addFact({ subject: 'user1', predicate: 'IS_PERSON', object: 'true' });

    // 设置属性（PersistentStore.addFact不支持options参数）
    store1.setNodeProperties(fact1.subjectId, {
      name: 'Alice',
      age: 25,
      tags: ['dev', 'typescript'],
    });

    // flush前验证（使用nodeId而不是字符串）
    const props1 = store1.getNodeProperties(fact1.subjectId);
    expect(props1).toEqual({ name: 'Alice', age: 25, tags: ['dev', 'typescript'] });

    // flush：将数据持久化到 PropertyDataStore
    await store1.flush();
    await store1.close();

    // 重启后验证数据是否从PropertyDataStore加载
    const store2 = await PersistentStore.open(dbPath, { enableLock: true });
    const props2 = store2.getNodeProperties(fact1.subjectId);
    expect(props2).toEqual({ name: 'Alice', age: 25, tags: ['dev', 'typescript'] });

    await store2.close();
  });

  it('should persist property updates correctly', async () => {
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });

    // 初始数据
    const fact = store1.addFact({ subject: 'user1', predicate: 'IS_PERSON', object: 'true' });
    store1.setNodeProperties(fact.subjectId, { status: 'active' });

    await store1.flush();

    // 更新属性（使用nodeId）
    store1.setNodeProperties(fact.subjectId, { status: 'inactive', level: 'senior' });

    await store1.flush();
    await store1.close();

    // 重启验证
    const store2 = await PersistentStore.open(dbPath, { enableLock: true });
    const props = store2.getNodeProperties(fact.subjectId);
    expect(props).toEqual({ status: 'inactive', level: 'senior' });

    await store2.close();
  });
});
