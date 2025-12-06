/**
 * Cypher 查询优化器测试
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { NervusDB } from '@/synapseDb';
import { createCypherSupport } from '@/extensions/query/cypher';
import { CypherQueryPlanner, CypherQueryExecutor } from '@/extensions/query/pattern';
import { tmpdir } from 'os';
import { join } from 'path';
import { unlinkSync, rmSync, existsSync } from 'fs';

describe('Cypher 查询优化器', () => {
  let db: NervusDB;
  let cypher: ReturnType<typeof createCypherSupport>;
  let dbPath: string;

  beforeEach(async () => {
    dbPath = join(tmpdir(), `test-optimization-${Date.now()}.synapsedb`);
    db = await NervusDB.open(dbPath, {
      experimental: { cypher: true },
    });
    cypher = createCypherSupport(db.getStore());

    // 添加测试数据
    for (let i = 1; i <= 100; i++) {
      db.addFact({ subject: `Person${i}`, predicate: 'HAS_NAME', object: `Name${i}` });
      db.addFact({ subject: `Person${i}`, predicate: 'HAS_AGE', object: `${20 + (i % 50)}` });

      if (i <= 50) {
        db.addFact({ subject: `Person${i}`, predicate: 'WORKS_AT', object: 'Company1' });
      } else {
        db.addFact({ subject: `Person${i}`, predicate: 'WORKS_AT', object: 'Company2' });
      }

      if (i % 10 === 0) {
        db.addFact({ subject: `Person${i}`, predicate: 'IS_MANAGER', object: 'true' });
      }
    }

    // 添加关系网络
    for (let i = 1; i < 100; i++) {
      if (i % 5 === 0) {
        db.addFact({ subject: `Person${i}`, predicate: 'KNOWS', object: `Person${i + 1}` });
      }
    }

    await db.flush();
  });

  afterEach(async () => {
    await db.close();
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

  describe('查询计划器', () => {
    it('应该生成基础查询计划', async () => {
      const planner = new CypherQueryPlanner(db.getStore());

      const query = {
        type: 'CypherQuery' as const,
        clauses: [
          {
            type: 'MatchClause' as const,
            optional: false,
            pattern: {
              type: 'Pattern' as const,
              elements: [
                {
                  type: 'NodePattern' as const,
                  variable: 'n',
                  labels: [],
                  properties: undefined,
                },
              ],
            },
          },
        ],
      };

      const plan = await planner.generatePlan(query);
      expect(plan).toBeDefined();
      expect(plan.type).toBe('IndexScan');
      expect(plan.cost).toBeGreaterThan(0);
      expect(plan.cardinality).toBeGreaterThan(0);
    });

    it('应该缓存查询计划', async () => {
      const planner = new CypherQueryPlanner(db.getStore());

      const query = {
        type: 'CypherQuery' as const,
        clauses: [
          {
            type: 'MatchClause' as const,
            optional: false,
            pattern: {
              type: 'Pattern' as const,
              elements: [
                {
                  type: 'NodePattern' as const,
                  variable: 'n',
                  labels: [],
                  properties: undefined,
                },
              ],
            },
          },
        ],
      };

      // 第一次生成计划
      const plan1 = await planner.generatePlan(query);

      // 第二次应该从缓存获取
      const plan2 = await planner.generatePlan(query);

      expect(plan1).toEqual(plan2);

      const stats = planner.getCacheStats();
      expect(stats.size).toBeGreaterThan(0);
    });

    it('应该清理计划缓存', async () => {
      const planner = new CypherQueryPlanner(db.getStore());

      const query = {
        type: 'CypherQuery' as const,
        clauses: [
          {
            type: 'MatchClause' as const,
            optional: false,
            pattern: {
              type: 'Pattern' as const,
              elements: [
                {
                  type: 'NodePattern' as const,
                  variable: 'n',
                  labels: [],
                  properties: undefined,
                },
              ],
            },
          },
        ],
      };

      await planner.generatePlan(query);
      expect(planner.getCacheStats().size).toBeGreaterThan(0);

      planner.clearCache();
      expect(planner.getCacheStats().size).toBe(0);
    });
  });

  describe('查询执行器', () => {
    it('应该执行索引扫描计划', async () => {
      const planner = new CypherQueryPlanner(db.getStore());
      const executor = new CypherQueryExecutor(db.getStore());

      const query = {
        type: 'CypherQuery' as const,
        clauses: [
          {
            type: 'MatchClause' as const,
            optional: false,
            pattern: {
              type: 'Pattern' as const,
              elements: [
                {
                  type: 'NodePattern' as const,
                  variable: 'n',
                  labels: [],
                  properties: undefined,
                },
              ],
            },
          },
        ],
      };

      const plan = await planner.generatePlan(query);
      const results = await executor.execute(plan);

      expect(Array.isArray(results)).toBe(true);
      expect(results.length).toBeGreaterThan(0);
    });

    it('应该处理参数化查询计划', async () => {
      const planner = new CypherQueryPlanner(db.getStore());
      const executor = new CypherQueryExecutor(db.getStore());

      const query = {
        type: 'CypherQuery' as const,
        clauses: [
          {
            type: 'MatchClause' as const,
            optional: false,
            pattern: {
              type: 'Pattern' as const,
              elements: [
                {
                  type: 'NodePattern' as const,
                  variable: 'n',
                  labels: [],
                  properties: undefined,
                },
              ],
            },
          },
        ],
      };

      const parameters = { name: 'Person1' };
      const plan = await planner.generatePlan(query);
      const results = await executor.execute(plan, parameters);

      expect(Array.isArray(results)).toBe(true);
    });
  });

  describe('端到端优化测试', () => {
    it('应该在优化模式下执行查询', async () => {
      const result = await cypher.cypher(
        'MATCH (n) RETURN n LIMIT 5',
        {},
        {
          enableOptimization: true,
          optimizationLevel: 'basic',
        },
      );

      expect(result.records).toBeDefined();
      expect(result.records.length).toBeGreaterThan(0);
      expect(result.records.length).toBeLessThanOrEqual(5);
      expect(result.summary.statementType).toBe('READ_ONLY');
    });

    it('应该在优化失败时回退到传统执行', async () => {
      // 这个查询可能会触发优化失败的情况
      const result = await cypher.cypher(
        'MATCH (n) WHERE n.nonexistent = "value" RETURN n',
        {},
        {
          enableOptimization: true,
        },
      );

      expect(result.records).toBeDefined();
      expect(Array.isArray(result.records)).toBe(true);
      expect(result.summary.statementType).toBe('READ_ONLY');
    });

    it('应该支持积极优化模式', async () => {
      const result = await cypher.cypher(
        'MATCH (n) RETURN n LIMIT 3',
        {},
        {
          enableOptimization: true,
          optimizationLevel: 'aggressive',
        },
      );

      expect(result.records).toBeDefined();
      expect(result.records.length).toBeGreaterThan(0);
      expect(result.records.length).toBeLessThanOrEqual(3);
    });
  });

  describe('性能对比测试', () => {
    it('应该测量优化前后的性能差异', async () => {
      // 使用没有LIMIT的查询进行性能对比，避免传统路径不支持LIMIT的问题
      const query = 'MATCH (n) RETURN n';

      // 传统执行
      const start1 = Date.now();
      const result1 = await cypher.cypher(query, {}, { enableOptimization: false });
      const time1 = Date.now() - start1;

      // 优化执行
      const start2 = Date.now();
      const result2 = await cypher.cypher(query, {}, { enableOptimization: true });
      const time2 = Date.now() - start2;

      expect(result1.records.length).toBe(result2.records.length);
      expect(result1.records.length).toBeGreaterThan(0);

      // 优化版本不应该显著变慢（允许一定开销）
      // 说明：在开启覆盖率收集时（V8 instrumentation）整体执行有固定倍数的开销，
      // 为避免将覆盖率工具的噪声误判为性能回退，这里在覆盖率模式下放宽容忍系数。
      const allowFactor = process.env.VITEST_COVERAGE ? 3.25 : 3;
      expect(time2).toBeLessThanOrEqual(time1 * allowFactor);
    });
  });

  describe('优化器管理', () => {
    it('应该提供优化器统计信息', () => {
      const stats = cypher.getOptimizerStats();

      expect(stats).toBeDefined();
      expect(stats.planner).toBeDefined();
      expect(typeof stats.planner.size).toBe('number');
    });

    it('应该支持清理优化器缓存', () => {
      cypher.clearOptimizationCache();

      const stats = cypher.getOptimizerStats();
      expect(stats.planner.size).toBe(0);
    });

    it('应该支持预热优化器', async () => {
      await cypher.warmUpOptimizer();

      // 预热后应该有统计信息
      const stats = cypher.getOptimizerStats();
      expect(stats.planner).toBeDefined();
    });
  });

  describe('错误处理', () => {
    it('应该处理无效查询的优化请求', async () => {
      await expect(
        cypher.cypher('INVALID QUERY SYNTAX', {}, { enableOptimization: true }),
      ).rejects.toThrow();
    });

    it('应该处理复杂查询的优化回退', async () => {
      // 复杂查询可能触发优化失败，但应该成功回退
      const result = await cypher.cypher(
        'MATCH (a)-[r]->(b) WHERE a.complex = $param RETURN a, r, b',
        { param: 'test' },
        { enableOptimization: true },
      );

      expect(result.records).toBeDefined();
      expect(Array.isArray(result.records)).toBe(true);
    });
  });
});
