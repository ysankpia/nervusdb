/**
 * 双向 BFS 路径查询测试
 *
 * 验证双向 BFS 算法的正确性和性能改进
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { join } from 'node:path';
import { cleanupWorkspace, makeWorkspace } from '../../../helpers/tempfs';

import { NervusDB } from '@/synapseDb';
import { SimpleBidirectionalPathBuilder } from '@/extensions/query/path/bidirectionalSimple';
import { VariablePathBuilder } from '@/extensions/query/path/variable';

describe('双向 BFS 路径查询', () => {
  let testDir: string;
  let db: NervusDB;
  let store: any;

  beforeEach(async () => {
    testDir = await makeWorkspace('bibfs');
    db = await NervusDB.open(join(testDir, 'test.synapsedb'));
    store = db.getStore();

    // 构建测试图：A -> B -> C -> D -> E
    await setupLinearGraph();
  });

  afterEach(async () => {
    await db.close();
    await cleanupWorkspace(testDir);
  });

  async function setupLinearGraph() {
    // 创建线性图: A -> B -> C -> D -> E
    db.addFact({ subject: 'A', predicate: 'CONNECTS', object: 'B' });
    db.addFact({ subject: 'B', predicate: 'CONNECTS', object: 'C' });
    db.addFact({ subject: 'C', predicate: 'CONNECTS', object: 'D' });
    db.addFact({ subject: 'D', predicate: 'CONNECTS', object: 'E' });

    // 添加更多连接创建复杂图
    db.addFact({ subject: 'A', predicate: 'CONNECTS', object: 'F' });
    db.addFact({ subject: 'F', predicate: 'CONNECTS', object: 'G' });
    db.addFact({ subject: 'G', predicate: 'CONNECTS', object: 'E' });

    // 添加标签
    await db.flush();
    const labelIndex = store.getLabelIndex();
    for (const node of ['A', 'B', 'C', 'D', 'E', 'F', 'G']) {
      const nodeId = store.getNodeIdByValue(node);
      if (nodeId) {
        labelIndex.addNodeLabels(nodeId, ['Node']);
      }
    }
    await db.flush();
  }

  async function setupComplexGraph() {
    // 创建更复杂的图结构用于性能测试
    const nodes = Array.from({ length: 20 }, (_, i) => `Node${i}`);

    // 创建网格状连接
    for (let i = 0; i < nodes.length - 1; i++) {
      db.addFact({ subject: nodes[i], predicate: 'CONNECTS', object: nodes[i + 1] });

      // 添加一些交叉连接
      if (i % 3 === 0 && i + 5 < nodes.length) {
        db.addFact({ subject: nodes[i], predicate: 'CONNECTS', object: nodes[i + 5] });
      }
    }

    await db.flush();
  }

  describe('基础功能', () => {
    it('应该找到最短路径', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('E');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
      );

      const path = bidirectional.shortestPath();

      expect(path).not.toBeNull();
      expect(path!.length).toBe(3); // 应该找到更短的路径 A->F->G->E
      expect(path!.startId).toBe(startId);
      expect(path!.endId).toBe(targetId);
    });

    it('应该处理无路径的情况', async () => {
      const startId = store.getNodeIdByValue('A');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      // 创建一个不存在的目标节点 ID
      const nonExistentId = 99999;

      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([nonExistentId]),
        predicateId,
        { min: 1, max: 10 },
      );

      const path = bidirectional.shortestPath();
      expect(path).toBeNull();
    });

    it('应该尊重最小路径长度限制', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('B'); // 直接相邻
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 3, max: 10 }, // 要求至少 3 步
      );

      const path = bidirectional.shortestPath();
      expect(path).toBeNull(); // 因为 A->B 只有 1 步，不满足最小长度
    });

    it('应该处理自环路径', async () => {
      const startId = store.getNodeIdByValue('A');

      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([startId]),
        store.getNodeIdByValue('CONNECTS'),
        { min: 0, max: 10 },
      );

      const path = bidirectional.shortestPath();
      expect(path).not.toBeNull();
      expect(path!.length).toBe(0);
      expect(path!.startId).toBe(startId);
      expect(path!.endId).toBe(startId);
    });
  });

  describe('算法正确性对比', () => {
    it('双向 BFS 和单向 BFS 应该产生相同长度的最短路径', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('E');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      // 双向 BFS
      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
      );
      const bidirectionalPath = bidirectional.shortestPath();

      // 单向 BFS
      const unidirectional = new VariablePathBuilder(store, new Set([startId]), predicateId, {
        min: 1,
        max: 10,
      });
      const unidirectionalPath = unidirectional.shortest(targetId);

      expect(bidirectionalPath).not.toBeNull();
      expect(unidirectionalPath).not.toBeNull();
      expect(bidirectionalPath!.length).toBe(unidirectionalPath!.length);
    });

    it('应该找到所有可能的最短路径之一', async () => {
      // A 到 E 有两条路径: A->B->C->D->E (4步) 和 A->F->G->E (3步)
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('E');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
      );

      const path = bidirectional.shortestPath();
      expect(path).not.toBeNull();
      expect(path!.length).toBe(3); // 应该找到较短的路径 A->F->G->E
    });
  });

  describe('节点唯一性', () => {
    it('应该防止节点重复访问', async () => {
      // 添加循环: E -> A
      db.addFact({ subject: 'E', predicate: 'CONNECTS', object: 'A' });
      await db.flush();

      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('E');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10, uniqueness: 'NODE' },
      );

      const path = bidirectional.shortestPath();
      expect(path).not.toBeNull();

      // 验证路径中没有重复节点
      const visitedNodes = new Set<number>();
      visitedNodes.add(path!.startId);

      for (const edge of path!.edges) {
        const nextNode =
          edge.direction === 'forward' ? edge.record.objectId : edge.record.subjectId;
        expect(visitedNodes.has(nextNode)).toBe(false);
        visitedNodes.add(nextNode);
      }
    });
  });

  describe('性能基准测试', () => {
    it('双向 BFS 在长路径查询中应该更快', async () => {
      // 设置复杂图
      await setupComplexGraph();

      const startId = store.getNodeIdByValue('Node0');
      const targetId = store.getNodeIdByValue('Node19');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      // 测量双向 BFS 性能
      const bidirectionalStart = performance.now();
      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 20 },
      );
      const bidirectionalPath = bidirectional.shortestPath();
      const bidirectionalTime = performance.now() - bidirectionalStart;

      // 测量单向 BFS 性能
      const unidirectionalStart = performance.now();
      const unidirectional = new VariablePathBuilder(store, new Set([startId]), predicateId, {
        min: 1,
        max: 20,
      });
      const unidirectionalPath = unidirectional.shortest(targetId);
      const unidirectionalTime = performance.now() - unidirectionalStart;

      // 验证结果一致性
      expect(bidirectionalPath).not.toBeNull();
      expect(unidirectionalPath).not.toBeNull();
      expect(bidirectionalPath!.length).toBe(unidirectionalPath!.length);

      // 性能比较（在小图中差异可能不明显，但算法是正确的）
      console.log(`双向 BFS 时间: ${bidirectionalTime.toFixed(2)}ms`);
      console.log(`单向 BFS 时间: ${unidirectionalTime.toFixed(2)}ms`);
      console.log(
        `性能提升: ${(((unidirectionalTime - bidirectionalTime) / unidirectionalTime) * 100).toFixed(1)}%`,
      );

      // 至少验证双向 BFS 没有明显的性能劣化
      expect(bidirectionalTime).toBeLessThan(unidirectionalTime * 2); // 容忍 2x 的性能差异
    });
  });

  describe('边界条件', () => {
    it('应该处理单节点起点和终点', async () => {
      const nodeId = store.getNodeIdByValue('A');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([nodeId]),
        new Set([nodeId]),
        predicateId,
        { min: 0, max: 0 },
      );

      const path = bidirectional.shortestPath();
      expect(path).not.toBeNull();
      expect(path!.length).toBe(0);
    });

    it('应该处理无效的谓词', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('E');
      const invalidPredicateId = 99999;

      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        invalidPredicateId,
        { min: 1, max: 10 },
      );

      const path = bidirectional.shortestPath();
      expect(path).toBeNull();
    });
  });
});
