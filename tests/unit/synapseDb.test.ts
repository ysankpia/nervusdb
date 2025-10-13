import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { NervusDB } from '@/synapseDb';
import type { FactOptions } from '@/synapseDb';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { unlink } from 'node:fs/promises';
import { existsSync } from 'node:fs';

describe('NervusDB 核心API', () => {
  let testDbPath: string;
  let db: NervusDB;

  beforeEach(async () => {
    // 创建临时数据库文件路径
    testDbPath = join(
      tmpdir(),
      `test-synapsedb-${Date.now()}-${Math.random().toString(36).slice(2)}.synapsedb`,
    );
  });

  afterEach(async () => {
    // 清理数据库资源
    if (db) {
      try {
        await db.close();
      } catch {
        // ignore cleanup errors
      }
    }

    // 清理测试文件
    try {
      if (existsSync(testDbPath)) {
        await unlink(testDbPath);
      }

      // 清理可能的索引目录
      const indexDir = `${testDbPath}.pages`;
      if (existsSync(indexDir)) {
        const { rm } = await import('node:fs/promises');
        await rm(indexDir, { recursive: true });
      }
    } catch {
      // ignore cleanup errors
    }
  });

  describe('数据库生命周期管理', () => {
    it('应该成功创建和打开数据库', async () => {
      db = await NervusDB.open(testDbPath);
      expect(db).toBeDefined();
      expect(db).toBeInstanceOf(NervusDB);
    });

    it('应该支持自定义配置选项', async () => {
      db = await NervusDB.open(testDbPath, {
        pageSize: 1000,
        enableLock: false,
        registerReader: false,
      });
      expect(db).toBeDefined();
    });

    it('应该能够正确关闭数据库', async () => {
      db = await NervusDB.open(testDbPath);
      await expect(db.close()).resolves.not.toThrow();
    });

    it('重复打开同一文件路径应该成功', async () => {
      // 首先创建数据库
      db = await NervusDB.open(testDbPath);
      await db.close();

      // 重新打开应该成功
      db = await NervusDB.open(testDbPath);
      expect(db).toBeDefined();
    });
  });

  describe('事实数据操作', () => {
    beforeEach(async () => {
      db = await NervusDB.open(testDbPath);
    });

    it('应该能够添加简单三元组事实', () => {
      const fact = { subject: 'Alice', predicate: 'knows', object: 'Bob' };
      const result = db.addFact(fact);

      expect(result).toBeDefined();
      expect(result.subject).toBe('Alice');
      expect(result.predicate).toBe('knows');
      expect(result.object).toBe('Bob');
      expect(result.subjectId).toBeGreaterThanOrEqual(0);
      expect(result.predicateId).toBeGreaterThanOrEqual(0);
      expect(result.objectId).toBeGreaterThanOrEqual(0);
    });

    it('应该支持带属性的事实添加', () => {
      const fact = { subject: 'Alice', predicate: 'worksAt', object: 'Company' };
      const options: FactOptions = {
        subjectProperties: { age: 30, city: 'Shanghai' },
        objectProperties: { industry: 'Tech', size: 'Large' },
        edgeProperties: { startDate: '2023-01-01', position: 'Engineer' },
      };

      const result = db.addFact(fact, options);

      expect(result).toBeDefined();
      expect(result.subject).toBe('Alice');
      expect(result.predicate).toBe('worksAt');
      expect(result.object).toBe('Company');
    });

    it('应该能够删除事实', () => {
      const fact = { subject: 'Test', predicate: 'delete', object: 'Me' };
      const added = db.addFact(fact);

      // deleteFact 返回 void，不会抛错即为成功
      expect(() => db.deleteFact(added)).not.toThrow();
    });

    it('删除不存在的事实应该返回undefined', () => {
      const nonExistent = {
        subject: 'NonExistent',
        predicate: 'test',
        object: 'data',
        subjectId: 99999,
        predicateId: 99999,
        objectId: 99999,
      };

      const result = db.deleteFact(nonExistent);
      expect(result).toBeUndefined();
    });

    it('应该能够列出所有事实', () => {
      db.addFact({ subject: 'A', predicate: 'relates', object: 'B' });
      db.addFact({ subject: 'B', predicate: 'relates', object: 'C' });

      const facts = db.listFacts();
      expect(facts).toHaveLength(2);
      expect(facts.every((f) => typeof f.subjectId === 'number')).toBe(true);
    });

    it('空数据库列出事实应该返回空数组', () => {
      const facts = db.listFacts();
      expect(facts).toEqual([]);
    });
  });

  describe('节点和属性操作', () => {
    beforeEach(async () => {
      db = await NervusDB.open(testDbPath);
    });

    it('应该能够根据值获取节点ID', () => {
      // 先添加一个事实确保节点存在
      db.addFact({ subject: 'TestNode', predicate: 'test', object: 'value' });

      const nodeId = db.getNodeId('TestNode');
      expect(nodeId).toBeGreaterThanOrEqual(0);
    });

    it('不存在的节点应该返回undefined', () => {
      const nodeId = db.getNodeId('NonExistentNode');
      expect(nodeId).toBeUndefined();
    });

    it('应该能够根据ID获取节点值', () => {
      const fact = db.addFact({ subject: 'ValueTest', predicate: 'has', object: 'data' });

      const value = db.getNodeValue(fact.subjectId);
      expect(value).toBe('ValueTest');
    });

    it('不存在的节点ID应该返回undefined', () => {
      const value = db.getNodeValue(99999);
      expect(value).toBeUndefined();
    });

    it('应该能够获取和设置节点属性', () => {
      const fact = db.addFact({ subject: 'NodeWithProps', predicate: 'test', object: 'data' });
      const properties = { name: 'Test', value: 42 };

      db.setNodeProperties(fact.subjectId, properties);
      const retrieved = db.getNodeProperties(fact.subjectId);

      expect(retrieved).toEqual(properties);
    });

    it('应该能够获取和设置边属性', () => {
      const fact = db.addFact({ subject: 'A', predicate: 'connects', object: 'B' });
      const edgeKey = {
        subjectId: fact.subjectId,
        predicateId: fact.predicateId,
        objectId: fact.objectId,
      };
      const properties = { weight: 0.5, type: 'strong' };

      db.setEdgeProperties(edgeKey, properties);
      const retrieved = db.getEdgeProperties(edgeKey);

      expect(retrieved).toEqual(properties);
    });

    it('不存在的节点属性应该返回null', () => {
      const properties = db.getNodeProperties(99999);
      expect(properties).toBeNull();
    });

    it('不存在的边属性应该返回null', () => {
      const edgeKey = { subjectId: 99999, predicateId: 99999, objectId: 99999 };
      const properties = db.getEdgeProperties(edgeKey);
      expect(properties).toBeNull();
    });
  });

  describe('批量操作和事务', () => {
    beforeEach(async () => {
      db = await NervusDB.open(testDbPath);
    });

    it('应该支持批量开始和提交', () => {
      // beginBatch 返回 void，不会抛错即为成功
      expect(() => db.beginBatch()).not.toThrow();

      db.addFact({ subject: 'Batch', predicate: 'test', object: 'data1' });
      db.addFact({ subject: 'Batch', predicate: 'test', object: 'data2' });

      expect(() => db.commitBatch()).not.toThrow();
    });

    it('应该支持批量中止', () => {
      db.beginBatch();

      db.addFact({ subject: 'Abort', predicate: 'test', object: 'data' });

      expect(() => db.abortBatch()).not.toThrow();
    });

    it('应该支持数据持久化flush', async () => {
      db.addFact({ subject: 'Flush', predicate: 'test', object: 'data' });

      await expect(db.flush()).resolves.not.toThrow();
    });
  });

  describe('查询构建器', () => {
    beforeEach(async () => {
      db = await NervusDB.open(testDbPath);

      // 准备测试数据
      db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
      db.addFact({ subject: 'Bob', predicate: 'knows', object: 'Charlie' });
      db.addFact({ subject: 'Alice', predicate: 'worksAt', object: 'Company' });
    });

    it('应该返回查询构建器实例', () => {
      const query = db.find({ predicate: 'knows' });
      expect(query).toBeDefined();
      expect(typeof query.all).toBe('function');
      expect(typeof query.follow).toBe('function');
    });

    it('应该能够执行基本查询', () => {
      const results = db.find({ predicate: 'knows' }).all();
      expect(results).toHaveLength(2);
    });

    it('应该支持链式查询', () => {
      const results = db.find({ subject: 'Alice' }).follow('knows').all();
      expect(results).toHaveLength(1);
      // 查询结果可能返回字符串或对象，取决于具体实现
      expect(typeof results[0] === 'string' || typeof results[0] === 'object').toBe(true);
    });

    it('应该支持空查询结果', () => {
      const results = db.find({ predicate: 'nonexistent' }).all();
      expect(results).toEqual([]);
    });

    it('应该支持流式查询', () => {
      const stream = db.findStream({ predicate: 'knows' });
      expect(stream).toBeDefined();
      // 流式查询返回的对象类型可能不同，只要不为 null 即可
      expect(stream).not.toBeNull();
    });
  });

  describe('错误处理', () => {
    it('打开无效路径应该抛出错误', async () => {
      const invalidPath = '/invalid/nonexistent/path/database.synapsedb';
      await expect(NervusDB.open(invalidPath)).rejects.toThrow();
    });

    it('空字符串主体不会抛出错误', async () => {
      db = await NervusDB.open(testDbPath);

      // 空字符串应该被正常处理
      expect(() => {
        db.addFact({ subject: '', predicate: 'test', object: 'data' });
      }).not.toThrow();
    });

    it('重复关闭数据库应该不报错', async () => {
      db = await NervusDB.open(testDbPath);

      await db.close();
      await expect(db.close()).resolves.not.toThrow();
    });
  });

  describe('数据类型处理', () => {
    beforeEach(async () => {
      db = await NervusDB.open(testDbPath);
    });

    it('应该正确处理字符串类型的节点值', () => {
      const fact = db.addFact({ subject: 'Alice', predicate: 'equals', object: 'Bob' });
      expect(fact.subject).toBe('Alice');
      expect(fact.object).toBe('Bob');
    });

    it('应该能够处理长字符串', () => {
      const longString = 'a'.repeat(1000);
      const fact = db.addFact({ subject: longString, predicate: 'is', object: 'long' });
      expect(fact.subject).toBe(longString);
      expect(fact.object).toBe('long');
    });
  });
});
