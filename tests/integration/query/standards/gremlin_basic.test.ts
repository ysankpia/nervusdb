/**
 * Gremlin 基础功能测试
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SynapseDB } from '@/synapseDb';
import { gremlin, P } from '@/query/gremlin';
import type { GraphTraversalSource } from '@/query/gremlin/source';
import { tmpdir } from 'os';
import { join } from 'path';
import { unlinkSync, rmSync, existsSync } from 'fs';

describe('Gremlin 基础功能', () => {
  let db: SynapseDB;
  let g: GraphTraversalSource;
  let dbPath: string;

  beforeEach(async () => {
    dbPath = join(tmpdir(), `test-gremlin-${Date.now()}.synapsedb`);
    db = await SynapseDB.open(dbPath);
    g = gremlin((db as any).store);

    // 添加测试数据
    // 人物节点
    db.addFact({ subject: 'person:张三', predicate: 'HAS_NAME', object: '张三' });
    db.addFact({ subject: 'person:张三', predicate: 'HAS_AGE', object: '25' });
    db.addFact({ subject: 'person:张三', predicate: 'HAS_CITY', object: '北京' });

    db.addFact({ subject: 'person:李四', predicate: 'HAS_NAME', object: '李四' });
    db.addFact({ subject: 'person:李四', predicate: 'HAS_AGE', object: '30' });
    db.addFact({ subject: 'person:李四', predicate: 'HAS_CITY', object: '上海' });

    db.addFact({ subject: 'person:王五', predicate: 'HAS_NAME', object: '王五' });
    db.addFact({ subject: 'person:王五', predicate: 'HAS_AGE', object: '28' });
    db.addFact({ subject: 'person:王五', predicate: 'HAS_CITY', object: '北京' });

    // 关系
    db.addFact({ subject: 'person:张三', predicate: 'KNOWS', object: 'person:李四' });
    db.addFact({ subject: 'person:李四', predicate: 'KNOWS', object: 'person:王五' });
    db.addFact({ subject: 'person:王五', predicate: 'KNOWS', object: 'person:张三' });

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

  describe('基本遍历', () => {
    it('应该能获取所有顶点', async () => {
      const vertices = await g.V().toList();
      expect(vertices.length).toBeGreaterThan(0);

      // 验证顶点结构
      const vertex = vertices[0];
      expect(vertex).toHaveProperty('type', 'vertex');
      expect(vertex).toHaveProperty('id');
      expect(vertex).toHaveProperty('label');
      expect(vertex).toHaveProperty('properties');
    });

    it('应该能获取所有边', async () => {
      const edges = await g.E().toList();
      expect(edges.length).toBeGreaterThan(0);

      // 验证边结构
      const edge = edges[0];
      expect(edge).toHaveProperty('type', 'edge');
      expect(edge).toHaveProperty('id');
      expect(edge).toHaveProperty('label');
      expect(edge).toHaveProperty('inVertex');
      expect(edge).toHaveProperty('outVertex');
      expect(edge).toHaveProperty('properties');
    });

    it('应该能按 ID 获取指定顶点', async () => {
      // 首先获取一个顶点 ID
      const allVertices = await g.V().toList();
      expect(allVertices.length).toBeGreaterThan(0);

      const vertexId = allVertices[0].id;
      const vertices = await g.V(vertexId).toList();

      expect(vertices.length).toBe(1);
      expect(vertices[0].id).toBe(vertexId);
    });

    it('应该支持 limit() 限制结果数量', async () => {
      const vertices = await g.V().limit(2).toList();
      expect(vertices.length).toBeLessThanOrEqual(2);
    });
  });

  describe('遍历导航', () => {
    it('应该能使用 out() 遍历出边', async () => {
      const results = await g.V().out('KNOWS').toList();
      expect(results.length).toBeGreaterThan(0);

      // 每个结果都应该是顶点
      results.forEach((result) => {
        expect(result).toHaveProperty('type', 'vertex');
      });
    });

    it('应该能使用 in() 遍历入边', async () => {
      const results = await g.V().in('KNOWS').toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        expect(result).toHaveProperty('type', 'vertex');
      });
    });

    it('应该能使用 both() 遍历双向边', async () => {
      const results = await g.V().both('KNOWS').toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        expect(result).toHaveProperty('type', 'vertex');
      });
    });

    it('应该能使用 outE() 获取出边', async () => {
      const results = await g.V().outE('KNOWS').toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        expect(result).toHaveProperty('type', 'edge');
        expect(result.label).toBe('KNOWS');
      });
    });

    it('应该能使用 inV() 从边到入顶点', async () => {
      const results = await g.E().inV().toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        expect(result).toHaveProperty('type', 'vertex');
      });
    });

    it('应该能使用 outV() 从边到出顶点', async () => {
      const results = await g.E().outV().toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        expect(result).toHaveProperty('type', 'vertex');
      });
    });
  });

  describe('过滤功能', () => {
    it('应该支持 has() 属性过滤', async () => {
      const results = await g.V().has('HAS_NAME').toList();
      expect(results.length).toBeGreaterThan(0);

      // 每个结果都应该有名字属性
      results.forEach((result) => {
        expect(result.properties).toHaveProperty('HAS_NAME');
      });
    });

    it('应该支持 has() 属性值过滤', async () => {
      const results = await g.V().has('HAS_NAME', '张三').toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        expect(result.properties.HAS_NAME).toBe('张三');
      });
    });

    it('应该支持 hasLabel() 标签过滤', async () => {
      const results = await g.E().hasLabel('KNOWS').toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        expect(result.label).toBe('KNOWS');
      });
    });

    it('应该支持谓词过滤', async () => {
      const results = await g.V().has('HAS_AGE', P.gt('26')).toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        const age = parseInt(result.properties.HAS_AGE as string);
        expect(age).toBeGreaterThan(26);
      });
    });
  });

  describe('投影和聚合', () => {
    it('应该支持 values() 获取属性值', async () => {
      const results = await g.V().values('HAS_NAME').toList();
      expect(results.length).toBeGreaterThan(0);

      results.forEach((result) => {
        expect(typeof result.properties.value).toBe('string');
      });
    });

    it('应该支持 count() 计数', async () => {
      const results = await g.V().count().toList();
      expect(results.length).toBe(1);
      expect(results[0].properties.value).toBeGreaterThan(0);
    });

    it('应该支持 dedup() 去重', async () => {
      const allResults = await g.V().both().toList();
      const dedupResults = await g.V().both().dedup().toList();

      expect(dedupResults.length).toBeLessThanOrEqual(allResults.length);
    });
  });

  describe('链式查询', () => {
    it('应该支持复杂链式查询', async () => {
      // 查找张三认识的人认识的人
      const results = await g.V().has('HAS_NAME', '张三').out('KNOWS').out('KNOWS').toList();

      expect(Array.isArray(results)).toBe(true);
    });

    it('应该支持带过滤的链式查询', async () => {
      // 查找年龄大于25的人认识的人
      const results = await g
        .V()
        .has('HAS_AGE', P.gt('25'))
        .out('KNOWS')
        .values('HAS_NAME')
        .toList();

      expect(Array.isArray(results)).toBe(true);
    });
  });

  describe('终端操作', () => {
    it('应该支持 hasNext() 检查', async () => {
      const hasNext = await g.V().hasNext();
      expect(typeof hasNext).toBe('boolean');
    });

    it('应该支持 next() 获取单个结果', async () => {
      const result = await g.V().next();
      expect(result).toHaveProperty('type', 'vertex');
    });

    it('应该支持 tryNext() 安全获取', async () => {
      const result = await g.V().tryNext();
      if (result) {
        expect(result).toHaveProperty('type', 'vertex');
      }
    });

    it('应该支持 iterate() 仅执行', async () => {
      await expect(g.V().iterate()).resolves.toBeUndefined();
    });
  });

  describe('错误处理', () => {
    it('应该正确处理不存在的 ID', async () => {
      const results = await g.V('nonexistent-id').toList();
      expect(results.length).toBe(0);
    });

    it('应该正确处理空查询结果', async () => {
      const results = await g.V().has('NONEXISTENT_PROPERTY').toList();
      expect(results.length).toBe(0);
    });

    it('应该在没有更多结果时抛出错误', async () => {
      const emptyTraversal = g.V().has('NONEXISTENT_PROPERTY');
      await expect(emptyTraversal.next()).rejects.toThrow('No more elements');
    });
  });

  describe('遍历源管理', () => {
    it('应该提供统计信息', () => {
      const stats = g.getStats();
      expect(stats).toHaveProperty('totalTraversals');
      expect(stats).toHaveProperty('avgExecutionTime');
      expect(stats).toHaveProperty('cacheHitRate');
      expect(stats).toHaveProperty('activeTraversals');
    });

    it('应该支持缓存清理', () => {
      g.clearCache();
      const stats = g.getStats();
      expect(stats.cacheHitRate).toBe(0);
    });

    it('应该支持预热', async () => {
      await expect(g.warmUp()).resolves.toBeUndefined();
    });
  });
});
