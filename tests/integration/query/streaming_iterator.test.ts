import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { mkdtemp, rm } from 'fs/promises';
import { join } from 'path';
import { tmpdir } from 'os';

describe('流式查询迭代器', () => {
  let tempDir: string;
  let db: NervusDB;

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'synapse-streaming-test-'));
    const dbPath = join(tempDir, 'test.synapsedb');
    db = await NervusDB.open(dbPath);
  });

  afterEach(async () => {
    await db.close();
    await rm(tempDir, { recursive: true, force: true });
  });

  it('异步迭代器应该逐个返回记录', async () => {
    // 插入测试数据
    for (let i = 0; i < 5; i++) {
      db.addFact({ subject: `user${i}`, predicate: 'LIKES', object: `item${i}` });
    }
    await db.flush();

    const query = db.find({ predicate: 'LIKES' });
    const results: string[] = [];

    // 使用异步迭代器
    for await (const record of query) {
      results.push(record.subject);
    }

    expect(results).toHaveLength(5);
    expect(results.sort()).toEqual(['user0', 'user1', 'user2', 'user3', 'user4']);
  });

  it('take() 方法应该限制结果数量', async () => {
    // 插入 10 条记录
    for (let i = 0; i < 10; i++) {
      db.addFact({ subject: `user${i}`, predicate: 'FOLLOWS', object: `user${i + 1}` });
    }
    await db.flush();

    const limited = db.find({ predicate: 'FOLLOWS' }).take(3).all();
    expect(limited).toHaveLength(3);
  });

  it('skip() 方法应该跳过指定数量的记录', async () => {
    // 插入有序的测试数据
    const users = ['alice', 'bob', 'charlie', 'david', 'eve'];
    users.forEach((user, i) => {
      db.addFact({ subject: user, predicate: 'INDEX', object: i.toString() });
    });
    await db.flush();

    const skipped = db.find({ predicate: 'INDEX' }).skip(2).all();
    expect(skipped).toHaveLength(3);
  });

  it('skip() 和 take() 应该可以组合使用', async () => {
    // 插入 10 条记录
    for (let i = 0; i < 10; i++) {
      db.addFact({ subject: `item${i}`, predicate: 'RANK', object: i.toString() });
    }
    await db.flush();

    const paginated = db.find({ predicate: 'RANK' }).skip(3).take(4).all();
    expect(paginated).toHaveLength(4);
  });

  it('batch() 迭代器应该按批次返回记录', async () => {
    // 插入 7 条记录
    for (let i = 0; i < 7; i++) {
      db.addFact({ subject: `batch${i}`, predicate: 'BELONGS_TO', object: 'group' });
    }
    await db.flush();

    const batches: number[] = [];
    const batchSize = 3;

    // 使用批量迭代器
    for await (const batch of db.find({ predicate: 'BELONGS_TO' }).batch(batchSize)) {
      batches.push(batch.length);
    }

    // 期望: [3, 3, 1] (7个记录分为3个批次)
    expect(batches).toEqual([3, 3, 1]);
  });

  it('大数据集流式查询应该控制内存使用', async () => {
    const startMemory = process.memoryUsage().heapUsed;

    // 插入大量数据
    for (let i = 0; i < 5000; i++) {
      db.addFact({ subject: `node${i}`, predicate: 'CONNECTS_TO', object: `node${i + 1}` });
    }
    await db.flush();

    let count = 0;
    const query = db.find({ predicate: 'CONNECTS_TO' });

    // 使用异步迭代器遍历，不应该导致内存爆炸
    for await (const record of query) {
      count++;

      // 每1000条检查一次内存使用
      if (count % 1000 === 0) {
        const currentMemory = process.memoryUsage().heapUsed;
        const memoryIncrease = currentMemory - startMemory;

        // 内存增长应该在合理范围内（小于50MB）
        expect(memoryIncrease).toBeLessThan(50 * 1024 * 1024);
      }
    }

    expect(count).toBe(5000);
  });

  it('批量迭代器的批次大小无效时应该抛出错误', async () => {
    db.addFact({ subject: 'test', predicate: 'IS', object: 'valid' });
    await db.flush();

    const query = db.find({ predicate: 'IS' });

    await expect(async () => {
      for await (const batch of query.batch(0)) {
        // 不应该执行到这里
      }
    }).rejects.toThrow('批次大小必须大于 0');

    await expect(async () => {
      for await (const batch of query.batch(-1)) {
        // 不应该执行到这里
      }
    }).rejects.toThrow('批次大小必须大于 0');
  });

  it('空查询结果的异步迭代器应该正常工作', async () => {
    const query = db.find({ predicate: 'NON_EXISTENT' });
    const results: any[] = [];

    for await (const record of query) {
      results.push(record);
    }

    expect(results).toHaveLength(0);
  });

  it('异步迭代器应该与 follow 链式查询兼容', async () => {
    // 构建链式数据
    db.addFact({ subject: 'root', predicate: 'HAS_CHILD', object: 'child1' });
    db.addFact({ subject: 'root', predicate: 'HAS_CHILD', object: 'child2' });
    db.addFact({ subject: 'child1', predicate: 'HAS_VALUE', object: 'value1' });
    db.addFact({ subject: 'child2', predicate: 'HAS_VALUE', object: 'value2' });
    await db.flush();

    const results: string[] = [];

    // 链式查询 + 异步迭代器
    for await (const record of db
      .find({ subject: 'root' })
      .follow('HAS_CHILD')
      .follow('HAS_VALUE')) {
      results.push(record.object);
    }

    expect(results.sort()).toEqual(['value1', 'value2']);
  });
});
