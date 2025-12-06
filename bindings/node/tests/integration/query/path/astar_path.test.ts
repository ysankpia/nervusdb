/**
 * A*启发式搜索算法测试
 *
 * 验证A*算法的正确性、不同启发式函数的效果和性能改进
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { join } from 'node:path';
import { cleanupWorkspace, makeWorkspace } from '../../../helpers/tempfs';

import { NervusDB } from '@/synapseDb';
import {
  AStarPathBuilder,
  createAStarPathBuilder,
  createGraphDistanceHeuristic,
} from '@/extensions/query/path/astar';
import { VariablePathBuilder } from '@/extensions/query/path/variable';
import { SimpleBidirectionalPathBuilder } from '@/extensions/query/path/bidirectionalSimple';

describe('A*启发式搜索算法', () => {
  let testDir: string;
  let db: NervusDB;
  let store: any;

  beforeEach(async () => {
    // 统一使用测试助手创建临时工作区
    testDir = await makeWorkspace('astar');
    db = await NervusDB.open(join(testDir, 'test.synapsedb'));
    store = db.getStore();

    // 构建测试图
    await setupTestGraph();
  });

  afterEach(async () => {
    await db.close();
    await cleanupWorkspace(testDir);
  });

  async function setupTestGraph() {
    // 创建更复杂的图结构：
    //     A
    //   /   \
    //  B     C
    //  |     |
    //  D --- E --- F
    //  |           |
    //  G --- H --- I
    //
    // 以及一些其他连接用于测试启发式效果

    const connections = [
      ['A', 'B'],
      ['A', 'C'],
      ['B', 'D'],
      ['C', 'E'],
      ['D', 'E'],
      ['E', 'F'],
      ['D', 'G'],
      ['F', 'I'],
      ['G', 'H'],
      ['H', 'I'],
      // 添加一些"诱惑"路径（较长但看起来更有希望）
      ['A', 'X'],
      ['X', 'Y'],
      ['Y', 'Z'], // 死胡同
      ['B', 'P'],
      ['P', 'Q'],
      ['Q', 'R'],
      ['R', 'I'], // 较长的替代路径
    ];

    for (const [from, to] of connections) {
      db.addFact({ subject: from, predicate: 'CONNECTS', object: to });
      // 添加双向连接以测试不同方向的搜索
      db.addFact({ subject: to, predicate: 'CONNECTS', object: from });
    }

    await db.flush();

    // 添加标签
    const labelIndex = store.getLabelIndex();
    const allNodes = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'X', 'Y', 'Z', 'P', 'Q', 'R'];
    for (const node of allNodes) {
      const nodeId = store.getNodeIdByValue(node);
      if (nodeId) {
        labelIndex.addNodeLabels(nodeId, ['Node']);
      }
    }

    await db.flush();
  }

  describe('基础功能', () => {
    it('应该找到最短路径', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'hop', weight: 1.0 },
      );

      const path = astar.shortestPath();

      expect(path).not.toBeNull();
      expect(path!.startId).toBe(startId);
      expect(path!.endId).toBe(targetId);
      expect(path!.length).toBeGreaterThan(0);
    });

    it('应该处理零长度路径', async () => {
      const nodeId = store.getNodeIdByValue('A');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const astar = new AStarPathBuilder(
        store,
        new Set([nodeId]),
        new Set([nodeId]),
        predicateId,
        { min: 0, max: 10 },
        { type: 'hop' },
      );

      const path = astar.shortestPath();
      expect(path).not.toBeNull();
      expect(path!.length).toBe(0);
      expect(path!.startId).toBe(nodeId);
      expect(path!.endId).toBe(nodeId);
    });

    it('应该处理无路径的情况', async () => {
      const startId = store.getNodeIdByValue('A');
      const predicateId = store.getNodeIdByValue('CONNECTS');
      const nonExistentId = 99999;

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([nonExistentId]),
        predicateId,
        { min: 1, max: 10 },
      );

      const path = astar.shortestPath();
      expect(path).toBeNull();
    });

    it('应该尊重最小路径长度限制', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('B'); // 直接相邻
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 3, max: 10 }, // 要求至少3步
      );

      const path = astar.shortestPath();
      if (path !== null) {
        expect(path.length).toBeGreaterThanOrEqual(3);
      }
    });
  });

  describe('不同启发式函数', () => {
    it('hop启发式应该工作', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'hop', weight: 1.0 },
      );

      const path = astar.shortestPath();
      expect(path).not.toBeNull();
    });

    it('manhattan启发式应该工作', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'manhattan', weight: 0.5 },
      );

      const path = astar.shortestPath();
      expect(path).not.toBeNull();
    });

    it('euclidean启发式应该工作', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'euclidean', weight: 1.2 },
      );

      const path = astar.shortestPath();
      expect(path).not.toBeNull();
    });

    it('自定义启发式应该工作', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const customHeuristic = (from: number, to: number) => {
        // 简单的自定义启发式：基于节点ID差值
        return Math.abs(from - to) / 100;
      };

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'custom', customHeuristic, weight: 1.0 },
      );

      const path = astar.shortestPath();
      expect(path).not.toBeNull();
    });

    it('图距离启发式应该工作', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const graphHeuristic = createGraphDistanceHeuristic(store, predicateId, 2);

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'custom', customHeuristic: graphHeuristic, weight: 1.0 },
      );

      const path = astar.shortestPath();
      expect(path).not.toBeNull();
    });
  });

  describe('算法正确性对比', () => {
    it('A*和单向BFS应该找到相同长度的最短路径', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      // A*搜索
      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'hop', weight: 1.0 },
      );
      const astarPath = astar.shortestPath();

      // 单向BFS
      const bfs = new VariablePathBuilder(store, new Set([startId]), predicateId, {
        min: 1,
        max: 10,
      });
      const bfsPath = bfs.shortest(targetId);

      expect(astarPath).not.toBeNull();
      expect(bfsPath).not.toBeNull();
      expect(astarPath!.length).toBe(bfsPath!.length);
    });

    it('A*和双向BFS应该找到相同长度的最短路径', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      // A*搜索
      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'hop', weight: 1.0 },
      );
      const astarPath = astar.shortestPath();

      // 双向BFS
      const bidirectional = new SimpleBidirectionalPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
      );
      const bidirectionalPath = bidirectional.shortestPath();

      expect(astarPath).not.toBeNull();
      expect(bidirectionalPath).not.toBeNull();
      expect(astarPath!.length).toBe(bidirectionalPath!.length);
    });
  });

  describe('性能基准测试', () => {
    it('A*应该在复杂图中提供良好的性能', async () => {
      // 创建更大的测试图
      await setupLargeGraph();

      const startId = store.getNodeIdByValue('Start');
      const targetId = store.getNodeIdByValue('End');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      // 测试A*性能
      const astarStart = performance.now();
      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 20 },
        { type: 'hop', weight: 1.0 },
      );
      const astarPath = astar.shortestPath();
      const astarTime = performance.now() - astarStart;

      // 测试单向BFS性能
      const bfsStart = performance.now();
      const bfs = new VariablePathBuilder(store, new Set([startId]), predicateId, {
        min: 1,
        max: 20,
      });
      const bfsPath = bfs.shortest(targetId);
      const bfsTime = performance.now() - bfsStart;

      console.log(`A*搜索时间: ${astarTime.toFixed(2)}ms`);
      console.log(`BFS搜索时间: ${bfsTime.toFixed(2)}ms`);

      if (astarPath && bfsPath) {
        console.log(
          `性能比较: A*相对BFS的效率为 ${(((bfsTime - astarTime) / bfsTime) * 100).toFixed(1)}%`,
        );
        expect(astarPath.length).toBe(bfsPath.length);
      }

      // A*不应该比BFS慢太多
      expect(astarTime).toBeLessThan(bfsTime * 3);
    });

    async function setupLargeGraph() {
      // 创建一个有30个节点的更大图
      const nodes = Array.from({ length: 30 }, (_, i) => `Node${i}`);

      // 添加起点和终点
      db.addFact({ subject: 'Start', predicate: 'CONNECTS', object: 'Node0' });
      db.addFact({ subject: 'Node29', predicate: 'CONNECTS', object: 'End' });

      // 创建链式连接和一些交叉连接
      for (let i = 0; i < nodes.length - 1; i++) {
        db.addFact({ subject: nodes[i], predicate: 'CONNECTS', object: nodes[i + 1] });

        // 添加一些随机连接增加复杂性
        if (i % 5 === 0 && i + 10 < nodes.length) {
          db.addFact({ subject: nodes[i], predicate: 'CONNECTS', object: nodes[i + 10] });
        }

        // 添加一些回退连接
        if (i > 5 && i % 7 === 0) {
          db.addFact({ subject: nodes[i], predicate: 'CONNECTS', object: nodes[i - 3] });
        }
      }

      await db.flush();

      // 添加标签
      const labelIndex = store.getLabelIndex();
      const allNodes = ['Start', 'End', ...nodes];
      for (const node of allNodes) {
        const nodeId = store.getNodeIdByValue(node);
        if (nodeId) {
          labelIndex.addNodeLabels(nodeId, ['Node']);
        }
      }

      await db.flush();
    }
  });

  describe('启发式权重测试', () => {
    it('不同权重应该影响搜索行为', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      // 低权重（更接近Dijkstra）
      const lowWeight = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'hop', weight: 0.1 },
      );

      // 高权重（更贪心）
      const highWeight = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'hop', weight: 2.0 },
      );

      const lowWeightPath = lowWeight.shortestPath();
      const highWeightPath = highWeight.shortestPath();

      expect(lowWeightPath).not.toBeNull();
      expect(highWeightPath).not.toBeNull();

      // 两种方式都应该找到最短路径（因为图相对简单）
      expect(lowWeightPath!.length).toBe(highWeightPath!.length);
    });
  });

  describe('节点唯一性', () => {
    it('应该防止节点重复访问', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const astar = new AStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10, uniqueness: 'NODE' },
        { type: 'hop', weight: 1.0 },
      );

      const path = astar.shortestPath();
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

  describe('便利函数', () => {
    it('createAStarPathBuilder应该工作', async () => {
      const startId = store.getNodeIdByValue('A');
      const targetId = store.getNodeIdByValue('I');
      const predicateId = store.getNodeIdByValue('CONNECTS');

      const astar = createAStarPathBuilder(
        store,
        new Set([startId]),
        new Set([targetId]),
        predicateId,
        { min: 1, max: 10 },
        { type: 'hop', weight: 1.0 },
      );

      const path = astar.shortestPath();
      expect(path).not.toBeNull();
    });
  });
});
