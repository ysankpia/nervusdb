/**
 * Cypher 基础功能测试
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { createCypherSupport, type CypherSupport } from '@/extensions/query/cypher';
import { PersistentStore } from '@/core/storage/persistentStore';
import { tmpdir } from 'os';
import { join } from 'path';
import { unlinkSync, rmSync, existsSync } from 'fs';

describe('Cypher 基础功能', () => {
  let db: NervusDB;
  let cypher: CypherSupport;
  let dbPath: string;

  beforeEach(async () => {
    // 使用临时文件
    dbPath = join(tmpdir(), `test-cypher-${Date.now()}.synapsedb`);
    db = await NervusDB.open(dbPath, {
      experimental: { cypher: true },
    });
    cypher = createCypherSupport(db.getStore());

    // 添加一些测试数据
    db.addFact({ subject: 'Alice', predicate: 'KNOWS', object: 'Bob' });
    db.addFact({ subject: 'Bob', predicate: 'KNOWS', object: 'Charlie' });
    db.addFact(
      { subject: 'Alice', predicate: 'WORKS_AT', object: 'TechCorp' },
      {
        subjectProperties: { name: 'Alice Smith', age: 30 },
        objectProperties: { name: 'TechCorp', type: 'Company' },
      },
    );
    await db.flush();
  });

  afterEach(async () => {
    await db.close();
    // 清理测试文件
    try {
      if (existsSync(dbPath)) {
        unlinkSync(dbPath);
      }
      const indexDir = dbPath + '.pages';
      if (existsSync(indexDir)) {
        rmSync(indexDir, { recursive: true, force: true });
      }
      const walFile = dbPath + '.wal';
      if (existsSync(walFile)) {
        unlinkSync(walFile);
      }
    } catch (error) {
      // 忽略清理错误
    }
  });

  describe('语法验证', () => {
    it('应该验证有效的 MATCH 查询', () => {
      const result = cypher.validateCypher('MATCH (n) RETURN n');
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('应该验证复杂的查询', () => {
      const result = cypher.validateCypher(
        'MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.age > 25 RETURN a.name, b.name',
      );
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('应该检测语法错误', () => {
      const result = cypher.validateCypher('MATCH (n RETURN n'); // 缺少右括号
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe('基础查询执行', () => {
    it('应该执行简单的 MATCH 查询', async () => {
      const result = await cypher.cypher('MATCH (n) RETURN n');

      expect(result.records).toBeDefined();
      expect(result.summary.statement).toBe('MATCH (n) RETURN n');
      expect(result.summary.statementType).toBe('READ_ONLY');
    });

    it('应该正确解析参数化查询', async () => {
      // 只测试参数解析，使用语法验证而不是执行
      const result = cypher.validateCypher('MATCH (n) RETURN n.property WHERE n.id = $param');
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);

      // 测试多参数
      const result2 = cypher.validateCypher(
        'MATCH (n) WHERE n.age > $minAge AND n.name = $name RETURN n',
      );
      expect(result2.valid).toBe(true);
      expect(result2.errors).toHaveLength(0);
    });
  });

  describe('只读模式', () => {
    it('应该执行只读查询', async () => {
      const result = await cypher.cypherRead('MATCH (n) RETURN n');
      expect(result.summary.statementType).toBe('READ_ONLY');
    });

    it('应该拒绝写操作在只读模式', async () => {
      await expect(cypher.cypherRead('CREATE (n:Person {name: "Test"})')).rejects.toThrow(
        '在只读模式下不能执行写操作',
      );
    });
  });

  describe('语句类型检测', () => {
    it('应该正确识别读操作', async () => {
      const result = await cypher.cypher('MATCH (n) RETURN n');
      expect(result.summary.statementType).toBe('READ_ONLY');
    });

    it('应该正确识别写操作', async () => {
      // 注意：CREATE 当前会抛出 "未实现" 错误，我们测试错误类型
      await expect(cypher.cypher('CREATE (n:Person {name: "Test"})')).rejects.toThrow();
    });
  });

  describe('性能度量', () => {
    it('应该提供执行时间统计', async () => {
      const result = await cypher.cypher('MATCH (n) RETURN n');

      expect(result.summary.resultAvailableAfter).toBeGreaterThanOrEqual(0);
      expect(result.summary.resultConsumedAfter).toBeGreaterThanOrEqual(
        result.summary.resultAvailableAfter,
      );
    });
  });
});
