import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { rmSync, mkdirSync } from 'node:fs';
import { PersistentStore } from '../../../src/core/storage/persistentStore.js';

describe('Disk-Centric Architecture Performance', () => {
  let testDir: string;
  let dbPath: string;

  beforeEach(() => {
    const unique = `disk-perf-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    testDir = join(tmpdir(), unique);
    mkdirSync(testDir, { recursive: true });
    dbPath = join(testDir, 'test.db');
  });

  afterEach(() => {
    try {
      rmSync(testDir, { recursive: true, force: true });
    } catch {
      // ignore cleanup errors
    }
  });

  it('should open database with O(1) time regardless of data size', async () => {
    // 第一阶段：创建一个包含大量数据的数据库
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });

    // 插入 10,000 条三元组（模拟大型数据库）
    for (let i = 0; i < 10000; i++) {
      store1.addFact({
        subject: `subject_${i}`,
        predicate: `predicate_${i % 100}`,
        object: `object_${i}`,
      });
    }

    await store1.flush();
    await store1.close();

    // 第二阶段：测试打开时间
    const startTime = Date.now();
    const store2 = await PersistentStore.open(dbPath, { enableLock: false });
    const openTime = Date.now() - startTime;

    // 验证：打开时间应该很快（< 500ms），因为不加载全量数据
    expect(openTime).toBeLessThan(500);
    console.log(`✓ Database with 10,000 triples opened in ${openTime}ms`);

    // 验证数据可查询（从磁盘读取）
    const results = store2.query({});
    expect(results.length).toBe(10000);

    await store2.close();
  });

  it('should query from disk without loading all data into memory', async () => {
    // 创建数据库
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });

    for (let i = 0; i < 5000; i++) {
      store1.addFact({
        subject: `user_${i}`,
        predicate: 'knows',
        object: `user_${i + 1}`,
      });
    }

    await store1.flush();
    await store1.close();

    // 重新打开（此时数据在磁盘）
    const store2 = await PersistentStore.open(dbPath, { enableLock: false });

    // 查询应该从磁盘读取，而不需要先加载全部数据
    const startTime = Date.now();
    const results = store2.query({ predicate: 'knows' });
    const queryTime = Date.now() - startTime;

    expect(results.length).toBe(5000);
    expect(queryTime).toBeLessThan(200); // 磁盘查询应该很快
    console.log(`✓ Queried 5,000 triples from disk in ${queryTime}ms`);

    await store2.close();
  });

  it('should merge in-memory deltas with disk data', async () => {
    // 创建基础数据并持久化
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });
    store1.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
    store1.addFact({ subject: 'Bob', predicate: 'knows', object: 'Charlie' });
    await store1.flush();
    await store1.close();

    // 重新打开并添加新数据（未flush，在内存中）
    const store2 = await PersistentStore.open(dbPath, { enableLock: true });
    store2.addFact({ subject: 'Charlie', predicate: 'knows', object: 'Dave' });

    // 查询应该返回磁盘数据 + 内存增量
    const allResults = store2.listFacts();
    const knowsResults = allResults.filter((r) => r.predicate === 'knows');
    expect(knowsResults.length).toBe(3);

    const objects = knowsResults.map((r) => r.object);
    expect(objects).toContain('Bob');
    expect(objects).toContain('Charlie');
    expect(objects).toContain('Dave');

    await store2.close();
  });

  it('should maintain fast startup time even after multiple flush cycles', async () => {
    const store1 = await PersistentStore.open(dbPath, { enableLock: true });

    // 模拟多次写入和 flush 循环
    for (let cycle = 0; cycle < 5; cycle++) {
      for (let i = 0; i < 1000; i++) {
        store1.addFact({
          subject: `cycle${cycle}_subject${i}`,
          predicate: 'type',
          object: `cycle${cycle}_value`,
        });
      }
      await store1.flush();
    }

    await store1.close();

    // 验证：即使经过多次 flush，打开时间仍然很快
    const startTime = Date.now();
    const store2 = await PersistentStore.open(dbPath, { enableLock: false });
    const openTime = Date.now() - startTime;

    expect(openTime).toBeLessThan(500);
    console.log(`✓ Database with 5,000 triples (5 flush cycles) opened in ${openTime}ms`);

    // 验证数据完整性
    const results = store2.query({});
    expect(results.length).toBe(5000);

    await store2.close();
  });
});
