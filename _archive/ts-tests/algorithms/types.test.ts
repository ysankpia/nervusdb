import { describe, expect, it } from 'vitest';
import {
  GraphNode,
  GraphEdge,
  Path,
  ShortestPathResult,
  CentralityResult,
  CommunityResult,
  SimilarityResult,
  GraphStats,
  AlgorithmOptions,
  PageRankOptions,
  LouvainOptions,
  PathOptions,
} from '@/extensions/algorithms/types';

describe('图算法类型定义测试', () => {
  describe('基础数据结构', () => {
    it('GraphNode 应该有正确的类型结构', () => {
      const node: GraphNode = {
        id: 'node-1',
        value: 'Node 1',
        properties: { age: 30, city: 'Shanghai' },
        labels: ['Person', 'User'],
      };

      expect(node.id).toBe('node-1');
      expect(node.value).toBe('Node 1');
      expect(node.properties?.age).toBe(30);
      expect(node.labels).toContain('Person');
    });

    it('GraphNode 应该支持最小必需字段', () => {
      const minimalNode: GraphNode = {
        id: 'minimal',
        value: 'Minimal Node',
      };

      expect(minimalNode.id).toBe('minimal');
      expect(minimalNode.value).toBe('Minimal Node');
      expect(minimalNode.properties).toBeUndefined();
      expect(minimalNode.labels).toBeUndefined();
    });

    it('GraphEdge 应该有正确的类型结构', () => {
      const edge: GraphEdge = {
        source: 'node-1',
        target: 'node-2',
        type: 'KNOWS',
        weight: 0.8,
        properties: { strength: 'strong', since: '2020' },
        directed: true,
      };

      expect(edge.source).toBe('node-1');
      expect(edge.target).toBe('node-2');
      expect(edge.type).toBe('KNOWS');
      expect(edge.weight).toBe(0.8);
      expect(edge.directed).toBe(true);
    });

    it('GraphEdge 应该支持无向边', () => {
      const undirectedEdge: GraphEdge = {
        source: 'A',
        target: 'B',
        type: 'CONNECTED',
        directed: false,
      };

      expect(undirectedEdge.directed).toBe(false);
      expect(undirectedEdge.weight).toBeUndefined();
    });
  });

  describe('算法结果类型', () => {
    it('Path 应该正确表示路径结果', () => {
      const path: Path = {
        nodes: ['A', 'B', 'C'],
        edges: [
          { source: 'A', target: 'B', type: 'CONNECTS' },
          { source: 'B', target: 'C', type: 'CONNECTS' },
        ],
        length: 2,
        weight: 1.5,
      };

      expect(path.nodes).toHaveLength(3);
      expect(path.edges).toHaveLength(2);
      expect(path.length).toBe(2);
      expect(path.weight).toBe(1.5);
    });

    it('ShortestPathResult 应该包含距离和路径映射', () => {
      const distances = new Map([
        ['A', 0],
        ['B', 1],
        ['C', 2],
      ]);

      const paths = new Map([
        ['B', { nodes: ['A', 'B'], edges: [], length: 1, weight: 1 }],
        ['C', { nodes: ['A', 'B', 'C'], edges: [], length: 2, weight: 2 }],
      ]);

      const result: ShortestPathResult = {
        distances,
        paths,
        stats: {
          nodesVisited: 3,
          edgesExamined: 5,
          executionTime: 10.5,
        },
      };

      expect(result.distances.get('A')).toBe(0);
      expect(result.distances.get('C')).toBe(2);
      expect(result.stats?.nodesVisited).toBe(3);
      expect(result.paths.has('B')).toBe(true);
    });

    it('CentralityResult 应该包含值和排名', () => {
      const values = new Map([
        ['A', 0.8],
        ['B', 0.6],
        ['C', 0.4],
      ]);

      const ranking = [
        { nodeId: 'A', value: 0.8 },
        { nodeId: 'B', value: 0.6 },
        { nodeId: 'C', value: 0.4 },
      ];

      const result: CentralityResult = {
        values,
        ranking,
        stats: {
          mean: 0.6,
          max: 0.8,
          min: 0.4,
          standardDeviation: 0.163,
        },
      };

      expect(result.values.get('A')).toBe(0.8);
      expect(result.ranking[0].nodeId).toBe('A');
      expect(result.stats.mean).toBe(0.6);
    });

    it('CommunityResult 应该表示社区发现结果', () => {
      const communities = new Map([
        ['A', 0],
        ['B', 0],
        ['C', 1],
        ['D', 1],
      ]);

      const result: CommunityResult = {
        communities,
        hierarchy: [
          {
            level: 0,
            communities,
            modularity: 0.4,
          },
        ],
        modularity: 0.4,
        communityCount: 2,
      };

      expect(result.communities.get('A')).toBe(0);
      expect(result.communities.get('C')).toBe(1);
      expect(result.communityCount).toBe(2);
      expect(result.modularity).toBe(0.4);
    });
  });

  describe('算法选项类型', () => {
    it('AlgorithmOptions 应该支持基础配置', () => {
      const options: AlgorithmOptions = {
        maxIterations: 100,
        tolerance: 0.001,
        parallel: true,
        seed: 42,
      };

      expect(options.maxIterations).toBe(100);
      expect(options.tolerance).toBe(0.001);
      expect(options.parallel).toBe(true);
      expect(options.seed).toBe(42);
    });

    it('PageRankOptions 应该扩展基础选项', () => {
      const personalization = new Map([
        ['A', 0.5],
        ['B', 0.3],
        ['C', 0.2],
      ]);

      const options: PageRankOptions = {
        maxIterations: 50,
        tolerance: 0.01,
        dampingFactor: 0.85,
        personalization,
      };

      expect(options.maxIterations).toBe(50);
      expect(options.dampingFactor).toBe(0.85);
      expect(options.personalization?.get('A')).toBe(0.5);
    });

    it('LouvainOptions 应该支持社区发现参数', () => {
      const options: LouvainOptions = {
        maxIterations: 10,
        resolution: 1.0,
        randomness: 0.01,
        seed: 123,
      };

      expect(options.resolution).toBe(1.0);
      expect(options.randomness).toBe(0.01);
      expect(options.seed).toBe(123);
    });

    it('PathOptions 应该支持路径查找参数', () => {
      const options: PathOptions = {
        maxHops: 5,
        minHops: 1,
        uniqueness: 'NODE',
        weightFunction: (edge: GraphEdge) => edge.weight || 1,
      };

      expect(options.maxHops).toBe(5);
      expect(options.minHops).toBe(1);
      expect(options.uniqueness).toBe('NODE');
      expect(typeof options.weightFunction).toBe('function');

      // 测试权重函数
      const testEdge: GraphEdge = { source: 'A', target: 'B', type: 'TEST', weight: 2.5 };
      expect(options.weightFunction?.(testEdge)).toBe(2.5);
    });
  });

  describe('图统计类型', () => {
    it('GraphStats 应该包含完整的图统计信息', () => {
      const stats: GraphStats = {
        nodeCount: 100,
        edgeCount: 250,
        averageDegree: 5.0,
        density: 0.05,
        diameter: 6,
        clusteringCoefficient: 0.3,
        isConnected: true,
        componentCount: 1,
      };

      expect(stats.nodeCount).toBe(100);
      expect(stats.edgeCount).toBe(250);
      expect(stats.averageDegree).toBe(5.0);
      expect(stats.density).toBe(0.05);
      expect(stats.isConnected).toBe(true);
    });

    it('GraphStats 应该支持可选字段', () => {
      const minimalStats: GraphStats = {
        nodeCount: 10,
        edgeCount: 15,
        averageDegree: 3.0,
        density: 0.33,
        isConnected: false,
        componentCount: 2,
      };

      expect(minimalStats.diameter).toBeUndefined();
      expect(minimalStats.clusteringCoefficient).toBeUndefined();
      expect(minimalStats.componentCount).toBe(2);
    });
  });

  describe('相似度结果类型', () => {
    it('SimilarityResult 应该包含相似度映射和Top对', () => {
      const similarities = new Map([
        [
          'A',
          new Map([
            ['B', 0.8],
            ['C', 0.6],
          ]),
        ],
        [
          'B',
          new Map([
            ['A', 0.8],
            ['C', 0.7],
          ]),
        ],
      ]);

      const topPairs = [
        { node1: 'A', node2: 'B', similarity: 0.8 },
        { node1: 'B', node2: 'C', similarity: 0.7 },
        { node1: 'A', node2: 'C', similarity: 0.6 },
      ];

      const result: SimilarityResult = {
        similarities,
        topPairs,
      };

      expect(result.similarities.get('A')?.get('B')).toBe(0.8);
      expect(result.topPairs).toHaveLength(3);
      expect(result.topPairs[0].similarity).toBe(0.8);
    });
  });

  describe('类型兼容性', () => {
    it('应该支持类型扩展', () => {
      interface CustomNode extends GraphNode {
        customProperty: string;
      }

      const customNode: CustomNode = {
        id: 'custom',
        value: 'Custom Node',
        customProperty: 'custom value',
      };

      expect(customNode.id).toBe('custom');
      expect(customNode.customProperty).toBe('custom value');
    });

    it('应该支持泛型算法选项', () => {
      interface CustomAlgorithmOptions extends AlgorithmOptions {
        customParam: number;
        customFlag: boolean;
      }

      const customOptions: CustomAlgorithmOptions = {
        maxIterations: 10,
        customParam: 42,
        customFlag: true,
      };

      expect(customOptions.maxIterations).toBe(10);
      expect(customOptions.customParam).toBe(42);
      expect(customOptions.customFlag).toBe(true);
    });
  });

  describe('边界条件和验证', () => {
    it('空图统计应该有合理默认值', () => {
      const emptyStats: GraphStats = {
        nodeCount: 0,
        edgeCount: 0,
        averageDegree: 0,
        density: 0,
        isConnected: false,
        componentCount: 0,
      };

      expect(emptyStats.nodeCount).toBe(0);
      expect(emptyStats.componentCount).toBe(0);
      expect(emptyStats.isConnected).toBe(false);
    });

    it('单节点图统计应该正确', () => {
      const singleNodeStats: GraphStats = {
        nodeCount: 1,
        edgeCount: 0,
        averageDegree: 0,
        density: 0,
        isConnected: true,
        componentCount: 1,
      };

      expect(singleNodeStats.nodeCount).toBe(1);
      expect(singleNodeStats.averageDegree).toBe(0);
      expect(singleNodeStats.isConnected).toBe(true);
    });
  });
});
