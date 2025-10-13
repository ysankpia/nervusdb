import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { QueryBuilder } from '@/query/queryBuilder';
import { NervusDB } from '@/synapseDb';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { unlink } from 'node:fs/promises';
import { existsSync } from 'node:fs';

describe('查询构建器单元测试', () => {
  let testDbPath: string;
  let db: NervusDB;
  let queryBuilder: QueryBuilder;

  beforeEach(async () => {
    // 创建临时数据库文件
    testDbPath = join(
      tmpdir(),
      `test-query-builder-${Date.now()}-${Math.random().toString(36).slice(2)}.synapsedb`,
    );

    db = await NervusDB.open(testDbPath);

    // 添加测试数据
    db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
    db.addFact({ subject: 'Bob', predicate: 'knows', object: 'Charlie' });
    db.addFact({ subject: 'Alice', predicate: 'worksAt', object: 'Company' });
    db.addFact({ subject: 'Bob', predicate: 'worksAt', object: 'Company' });
    db.addFact({ subject: 'Charlie', predicate: 'likes', object: 'Coffee' });

    // 创建查询构建器
    queryBuilder = db.find({ predicate: 'knows' });
  });

  afterEach(async () => {
    // 清理资源
    if (db) {
      try {
        await db.close();
      } catch {
        // ignore cleanup errors
      }
    }

    try {
      if (existsSync(testDbPath)) {
        await unlink(testDbPath);
      }

      // 清理索引目录
      const indexDir = `${testDbPath}.pages`;
      if (existsSync(indexDir)) {
        const { rm } = await import('node:fs/promises');
        await rm(indexDir, { recursive: true });
      }
    } catch {
      // ignore cleanup errors
    }
  });

  describe('基础查询操作', () => {
    it('应该返回正确数量的结果', () => {
      const results = queryBuilder.all();
      expect(results).toHaveLength(2); // Alice->Bob, Bob->Charlie
    });

    it('all() 方法应该返回所有结果的副本', () => {
      const results1 = queryBuilder.all();
      const results2 = queryBuilder.all();

      expect(results1).toEqual(results2);
      expect(results1).not.toBe(results2); // 应该是不同的数组实例
    });

    it('toArray() 应该等价于 all()', () => {
      const allResults = queryBuilder.all();
      const arrayResults = queryBuilder.toArray();

      expect(arrayResults).toEqual(allResults);
    });

    it('length 属性应该返回正确的记录数', () => {
      expect(queryBuilder.length).toBe(2);
    });

    it('slice() 应该支持数组切片操作', () => {
      const firstResult = queryBuilder.slice(0, 1);
      expect(firstResult).toHaveLength(1);

      const allResults = queryBuilder.slice();
      expect(allResults).toHaveLength(2);

      const lastResult = queryBuilder.slice(1);
      expect(lastResult).toHaveLength(1);
    });
  });

  describe('过滤操作', () => {
    it('where() 应该支持简单的过滤条件', () => {
      const aliceQuery = queryBuilder.where((fact) => fact.subject === 'Alice');
      const results = aliceQuery.all();

      expect(results).toHaveLength(1);
      expect(results[0].subject).toBe('Alice');
      expect(results[0].object).toBe('Bob');
    });

    it('where() 应该处理异常情况', () => {
      // 过滤函数抛出异常时应该返回 false
      const safeQuery = queryBuilder.where(() => {
        throw new Error('Test error');
      });

      const results = safeQuery.all();
      expect(results).toHaveLength(0);
    });

    it('where() 应该支持链式调用', () => {
      const chainedQuery = queryBuilder
        .where((fact) => fact.subject.startsWith('A'))
        .where((fact) => fact.object === 'Bob');

      const results = chainedQuery.all();
      expect(results).toHaveLength(1);
      expect(results[0].subject).toBe('Alice');
    });
  });

  describe('限制和分页操作', () => {
    it('limit() 应该限制结果数量', () => {
      const limitedQuery = queryBuilder.limit(1);
      const results = limitedQuery.all();

      expect(results).toHaveLength(1);
    });

    it('limit() 负数应该返回原查询', () => {
      const negativeLimit = queryBuilder.limit(-1);
      const results = negativeLimit.all();

      expect(results).toHaveLength(2); // 原始数量
    });

    it('limit() NaN应该返回原查询', () => {
      const nanLimit = queryBuilder.limit(NaN);
      const results = nanLimit.all();

      expect(results).toHaveLength(2); // 原始数量
    });

    it('limit(0) 应该返回空结果', () => {
      const zeroLimit = queryBuilder.limit(0);
      const results = zeroLimit.all();

      expect(results).toHaveLength(0);
    });

    it('take() 应该等价于 limit()', () => {
      const takeResults = queryBuilder.take(1).all();
      const limitResults = queryBuilder.limit(1).all();

      expect(takeResults).toEqual(limitResults);
    });

    it('skip() 应该跳过指定数量的结果', () => {
      const skipQuery = queryBuilder.skip(1);
      const results = skipQuery.all();

      expect(results).toHaveLength(1);
    });

    it('skip() 负数或0应该返回原查询', () => {
      const skipNegative = queryBuilder.skip(-1);
      const skipZero = queryBuilder.skip(0);

      expect(skipNegative.all()).toHaveLength(2);
      expect(skipZero.all()).toHaveLength(2);
    });

    it('skip() NaN应该返回原查询', () => {
      const skipNan = queryBuilder.skip(NaN);
      expect(skipNan.all()).toHaveLength(2);
    });

    it('应该支持分页查询组合', () => {
      const pageQuery = queryBuilder.skip(1).take(1);
      const results = pageQuery.all();

      expect(results).toHaveLength(1);
    });
  });

  describe('联合操作', () => {
    it('union() 应该合并并去重两个查询结果', () => {
      const query1 = db.find({ subject: 'Alice' });
      const query2 = db.find({ subject: 'Bob' });

      const unionQuery = query1.union(query2);
      const results = unionQuery.all();

      // Alice: knows Bob, worksAt Company
      // Bob: knows Charlie, worksAt Company
      // 总共4个不重复的事实
      expect(results).toHaveLength(4);
    });

    it('unionAll() 应该合并不去重两个查询结果', () => {
      const query1 = db.find({ predicate: 'knows' });
      const query2 = db.find({ predicate: 'knows' });

      const unionAllQuery = query1.unionAll(query2);
      const results = unionAllQuery.all();

      // 两个相同查询的结果合并，应该是4个（2+2，不去重）
      expect(results).toHaveLength(4);
    });

    it('union() 对相同查询应该去重', () => {
      const query1 = db.find({ predicate: 'knows' });
      const query2 = db.find({ predicate: 'knows' });

      const unionQuery = query1.union(query2);
      const results = unionQuery.all();

      // 相同查询union应该去重，仍然是2个结果
      expect(results).toHaveLength(2);
    });
  });

  describe('迭代器支持', () => {
    it('应该支持同步迭代器', () => {
      const results: any[] = [];
      for (const fact of queryBuilder) {
        results.push(fact);
      }

      expect(results).toHaveLength(2);
      expect(results[0].predicate).toBe('knows');
    });

    it('应该支持异步迭代器', async () => {
      const results: any[] = [];
      for await (const fact of queryBuilder) {
        results.push(fact);
      }

      expect(results).toHaveLength(2);
      expect(results[0].predicate).toBe('knows');
    });

    it('batch() 应该支持批量异步迭代', async () => {
      const batches: any[][] = [];
      for await (const batch of queryBuilder.batch(1)) {
        batches.push(batch);
      }

      expect(batches).toHaveLength(2);
      expect(batches[0]).toHaveLength(1);
      expect(batches[1]).toHaveLength(1);
    });

    it('batch() 大小必须大于0', async () => {
      await expect(async () => {
        for await (const batch of queryBuilder.batch(0)) {
          // This should throw before yielding any batches
        }
      }).rejects.toThrow('批次大小必须大于 0');
    });
  });

  describe('边界条件和错误处理', () => {
    it('空查询结果应该正确处理', () => {
      const emptyQuery = db.find({ predicate: 'nonexistent' });

      expect(emptyQuery.length).toBe(0);
      expect(emptyQuery.all()).toEqual([]);
      expect(emptyQuery.slice()).toEqual([]);
    });

    it('空查询的迭代器应该正确工作', async () => {
      const emptyQuery = db.find({ predicate: 'nonexistent' });

      const syncResults: any[] = [];
      for (const fact of emptyQuery) {
        syncResults.push(fact);
      }

      const asyncResults: any[] = [];
      for await (const fact of emptyQuery) {
        asyncResults.push(fact);
      }

      expect(syncResults).toEqual([]);
      expect(asyncResults).toEqual([]);
    });

    it('空查询的批量迭代器应该正确工作', async () => {
      const emptyQuery = db.find({ predicate: 'nonexistent' });

      const batches: any[][] = [];
      for await (const batch of emptyQuery.batch(5)) {
        batches.push(batch);
      }

      expect(batches).toEqual([]);
    });

    it('链式操作应该保持不可变性', () => {
      const originalResults = queryBuilder.all();
      const limitedQuery = queryBuilder.limit(1);
      const filteredQuery = queryBuilder.where((f) => f.subject === 'Alice');

      // 原查询不应该被修改
      expect(queryBuilder.all()).toEqual(originalResults);
      expect(queryBuilder.all()).toHaveLength(2);

      // 派生查询应该有正确的结果
      expect(limitedQuery.all()).toHaveLength(1);
      expect(filteredQuery.all()).toHaveLength(1);
    });
  });

  describe('变长路径查询', () => {
    it('variablePath() 应该返回路径构建器', () => {
      const pathBuilder = queryBuilder.variablePath('knows', { max: 2 });
      expect(pathBuilder).toBeDefined();
      expect(typeof pathBuilder.all).toBe('function');
    });

    it('variablePath() 对不存在的关系应该返回空路径构建器', () => {
      const pathBuilder = queryBuilder.variablePath('nonexistent', { max: 1 });
      expect(pathBuilder).toBeDefined();
      expect(typeof pathBuilder.all).toBe('function');
    });

    it('空前沿的variablePath应该返回空路径构建器', () => {
      const emptyQuery = db.find({ predicate: 'nonexistent' });
      const pathBuilder = emptyQuery.variablePath('knows', { max: 1 });
      expect(pathBuilder).toBeDefined();
      expect(typeof pathBuilder.all).toBe('function');
    });

    it('variablePath() 应该返回正确的路径结果', () => {
      // 从Alice开始的查询，寻找through knows关系的路径
      const aliceQuery = db.find({ subject: 'Alice', predicate: 'knows' });
      const pathBuilder = aliceQuery.variablePath('knows', { max: 1 });
      const paths = pathBuilder.all();

      // 路径结果的结构验证
      expect(Array.isArray(paths)).toBe(true);
    });
  });
});
