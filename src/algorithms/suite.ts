/**
 * 图算法套件统一入口
 *
 * 提供完整的图算法库访问接口，整合所有算法模块
 */

import {
  Graph,
  GraphAlgorithmSuite,
  GraphAlgorithmFactory,
  AlgorithmOptions,
  PageRankOptions,
  LouvainOptions,
  PathOptions,
  CentralityResult,
  CommunityResult,
  ShortestPathResult,
  Path,
  GraphStats,
  GraphEdge,
} from './types.js';

import { MemoryGraph, GraphBuilder } from './graph.js';

// 导入算法实现
import { CentralityAlgorithmFactory } from './centrality.js';

import { CommunityDetectionAlgorithmFactory } from './community.js';

import { PathAlgorithmFactory } from './pathfinding.js';

import { SimilarityAlgorithmFactory } from './similarity.js';

/**
 * 图算法套件实现类
 */
export class GraphAlgorithmSuiteImpl implements GraphAlgorithmSuite {
  constructor(private graph: Graph) {}

  // 中心性算法
  centrality = {
    pageRank: (options?: PageRankOptions): CentralityResult => {
      const algorithm = CentralityAlgorithmFactory.createPageRank();
      return algorithm.compute(this.graph, options);
    },

    betweenness: (_options?: AlgorithmOptions): CentralityResult => {
      void _options;
      const algorithm = CentralityAlgorithmFactory.createBetweenness();
      return algorithm.compute(this.graph);
    },

    closeness: (options?: AlgorithmOptions): CentralityResult => {
      const algorithm = CentralityAlgorithmFactory.createCloseness();
      return algorithm.compute(this.graph, options);
    },

    degree: (_options?: AlgorithmOptions): CentralityResult => {
      void _options;
      const algorithm = CentralityAlgorithmFactory.createDegree();
      return algorithm.compute(this.graph);
    },

    eigenvector: (options?: AlgorithmOptions): CentralityResult => {
      const algorithm = CentralityAlgorithmFactory.createEigenvector();
      return algorithm.compute(this.graph, options);
    },
  };

  // 路径算法
  path = {
    dijkstra: (source: string, target?: string, options?: PathOptions): ShortestPathResult => {
      const algorithm = PathAlgorithmFactory.createDijkstra();
      if (target) {
        const singlePath = algorithm.findShortestPath(this.graph, source, target, options);
        const paths = new Map<string, Path>();
        const distances = new Map<string, number>();

        if (singlePath) {
          paths.set(target, singlePath);
          distances.set(target, singlePath.weight);
        }

        return {
          distances,
          paths,
          stats: { nodesVisited: 0, edgesExamined: 0, executionTime: 0 },
        };
      }
      return algorithm.findShortestPaths(this.graph, source, options);
    },

    astar: (
      source: string,
      target: string,
      heuristic?: (nodeId: string) => number,
      options?: PathOptions,
    ): Path | null => {
      const algorithm = PathAlgorithmFactory.createAStar();
      const extendedOptions = { ...options, heuristic };
      return algorithm.findShortestPath(this.graph, source, target, extendedOptions);
    },

    floydWarshall: (options?: PathOptions): Map<string, Map<string, number>> => {
      const algorithm = PathAlgorithmFactory.createFloydWarshall();
      return algorithm.findAllShortestPaths(this.graph, options);
    },

    bellmanFord: (source: string, options?: PathOptions): ShortestPathResult => {
      const algorithm = PathAlgorithmFactory.createBellmanFord();
      return algorithm.findShortestPaths(this.graph, source, options);
    },
  };

  // 社区发现算法
  community = {
    louvain: (options?: LouvainOptions): CommunityResult => {
      const algorithm = CommunityDetectionAlgorithmFactory.createLouvain();
      return algorithm.detectCommunities(this.graph, options);
    },

    labelPropagation: (options?: AlgorithmOptions): CommunityResult => {
      const algorithm = CommunityDetectionAlgorithmFactory.createLabelPropagation();
      return algorithm.detectCommunities(this.graph, options);
    },

    connectedComponents: (): CommunityResult => {
      const algorithm = CommunityDetectionAlgorithmFactory.createConnectedComponents();
      return algorithm.detectCommunities(this.graph);
    },

    stronglyConnectedComponents: (): CommunityResult => {
      const algorithm = CommunityDetectionAlgorithmFactory.createStronglyConnectedComponents();
      return algorithm.detectCommunities(this.graph);
    },
  };

  // 相似度算法
  similarity = {
    jaccard: (node1: string, node2: string): number => {
      const algorithm = SimilarityAlgorithmFactory.createJaccard();
      return algorithm.computeSimilarity(this.graph, node1, node2);
    },

    cosine: (node1: string, node2: string): number => {
      const algorithm = SimilarityAlgorithmFactory.createCosine();
      return algorithm.computeSimilarity(this.graph, node1, node2);
    },

    adamic: (node1: string, node2: string): number => {
      const algorithm = SimilarityAlgorithmFactory.createAdamic();
      return algorithm.computeSimilarity(this.graph, node1, node2);
    },

    preferentialAttachment: (node1: string, node2: string): number => {
      const algorithm = SimilarityAlgorithmFactory.createPreferentialAttachment();
      return algorithm.computeSimilarity(this.graph, node1, node2);
    },
  };

  // 图分析
  analysis = {
    getStats: (): GraphStats => {
      return this.graph.getStats();
    },

    findBridges: (): GraphEdge[] => {
      return this.findBridgeEdges();
    },

    findArticulationPoints: (): string[] => {
      return this.findArticulationNodes();
    },

    detectCycles: (): Path[] => {
      return this.detectCyclePaths();
    },

    topologicalSort: (): string[] | null => {
      return this.performTopologicalSort();
    },
  };

  /**
   * 寻找桥边（割边）
   */
  private findBridgeEdges(): GraphEdge[] {
    const bridges: GraphEdge[] = [];
    const visited = new Set<string>();
    const disc = new Map<string, number>();
    const low = new Map<string, number>();
    const parent = new Map<string, string>();
    let time = 0;

    const bridgeUtil = (nodeId: string) => {
      visited.add(nodeId);
      disc.set(nodeId, time);
      low.set(nodeId, time);
      time++;

      const neighbors = this.graph.getNeighbors(nodeId);
      for (const neighbor of neighbors) {
        const neighborId = neighbor.id;

        if (!visited.has(neighborId)) {
          parent.set(neighborId, nodeId);
          bridgeUtil(neighborId);

          // 更新 low value
          low.set(nodeId, Math.min(low.get(nodeId)!, low.get(neighborId)!));

          // 如果 low[v] > disc[u]，则 (u,v) 是桥边
          if (low.get(neighborId)! > disc.get(nodeId)!) {
            const edge =
              this.graph.getOutEdges(nodeId).find((e) => e.target === neighborId) ||
              this.graph.getInEdges(nodeId).find((e) => e.source === neighborId);

            if (edge) {
              bridges.push(edge);
            }
          }
        } else if (neighborId !== parent.get(nodeId)) {
          // 更新 low value
          low.set(nodeId, Math.min(low.get(nodeId)!, disc.get(neighborId)!));
        }
      }
    };

    // 对所有未访问的节点运行DFS
    for (const node of this.graph.getNodes()) {
      if (!visited.has(node.id)) {
        bridgeUtil(node.id);
      }
    }

    return bridges;
  }

  /**
   * 寻找关节点（割点）
   */
  private findArticulationNodes(): string[] {
    const visited = new Set<string>();
    const disc = new Map<string, number>();
    const low = new Map<string, number>();
    const parent = new Map<string, string>();
    const ap = new Set<string>();
    let time = 0;

    const apUtil = (nodeId: string) => {
      let children = 0;
      visited.add(nodeId);
      disc.set(nodeId, time);
      low.set(nodeId, time);
      time++;

      const neighbors = this.graph.getNeighbors(nodeId);
      for (const neighbor of neighbors) {
        const neighborId = neighbor.id;

        if (!visited.has(neighborId)) {
          children++;
          parent.set(neighborId, nodeId);
          apUtil(neighborId);

          low.set(nodeId, Math.min(low.get(nodeId)!, low.get(neighborId)!));

          // 根节点是关节点如果它有多于一个子节点
          if (!parent.has(nodeId) && children > 1) {
            ap.add(nodeId);
          }

          // 非根节点是关节点如果移除它会增加连通分量数
          if (parent.has(nodeId) && low.get(neighborId)! >= disc.get(nodeId)!) {
            ap.add(nodeId);
          }
        } else if (neighborId !== parent.get(nodeId)) {
          low.set(nodeId, Math.min(low.get(nodeId)!, disc.get(neighborId)!));
        }
      }
    };

    for (const node of this.graph.getNodes()) {
      if (!visited.has(node.id)) {
        apUtil(node.id);
      }
    }

    return Array.from(ap);
  }

  /**
   * 检测环路
   */
  private detectCyclePaths(): Path[] {
    const cycles: Path[] = [];
    const visited = new Set<string>();
    const recStack = new Set<string>();
    const pathStack: Array<{ nodeId: string; edge?: GraphEdge }> = [];

    const hasCycleUtil = (nodeId: string): boolean => {
      visited.add(nodeId);
      recStack.add(nodeId);

      const neighbors = this.graph.getOutEdges(nodeId);
      for (const edge of neighbors) {
        const neighborId = edge.target;
        pathStack.push({ nodeId: neighborId, edge });

        if (!visited.has(neighborId)) {
          if (hasCycleUtil(neighborId)) {
            return true;
          }
        } else if (recStack.has(neighborId)) {
          // 找到环路，构建路径
          const cycleStartIndex = pathStack.findIndex((item) => item.nodeId === neighborId);
          if (cycleStartIndex !== -1) {
            const cyclePath = pathStack.slice(cycleStartIndex);
            cycles.push({
              nodes: cyclePath.map((item) => item.nodeId),
              edges: cyclePath.slice(1).map((item) => item.edge!),
              length: cyclePath.length - 1,
              weight: cyclePath.slice(1).reduce((sum, item) => sum + (item.edge?.weight || 1), 0),
            });
          }
          return true;
        }

        pathStack.pop();
      }

      recStack.delete(nodeId);
      return false;
    };

    for (const node of this.graph.getNodes()) {
      if (!visited.has(node.id)) {
        pathStack.length = 0;
        pathStack.push({ nodeId: node.id });
        hasCycleUtil(node.id);
      }
    }

    return cycles;
  }

  /**
   * 拓扑排序
   */
  private performTopologicalSort(): string[] | null {
    const inDegree = new Map<string, number>();
    const queue: string[] = [];
    const result: string[] = [];

    // 计算所有节点的入度
    for (const node of this.graph.getNodes()) {
      inDegree.set(node.id, this.graph.getInDegree(node.id));
      if (inDegree.get(node.id) === 0) {
        queue.push(node.id);
      }
    }

    while (queue.length > 0) {
      const current = queue.shift()!;
      result.push(current);

      // 减少邻居节点的入度
      const outEdges = this.graph.getOutEdges(current);
      for (const edge of outEdges) {
        const neighborId = edge.target;
        const newInDegree = inDegree.get(neighborId)! - 1;
        inDegree.set(neighborId, newInDegree);

        if (newInDegree === 0) {
          queue.push(neighborId);
        }
      }
    }

    // 检查是否存在环路
    if (result.length !== this.graph.getNodes().length) {
      return null; // 图中存在环路，无法进行拓扑排序
    }

    return result;
  }
}

/**
 * 图算法工厂实现
 */
export class GraphAlgorithmFactoryImpl implements GraphAlgorithmFactory {
  createGraph(): Graph {
    return new MemoryGraph();
  }

  createAlgorithmSuite(graph: Graph): GraphAlgorithmSuite {
    return new GraphAlgorithmSuiteImpl(graph);
  }

  createCentralityAlgorithm(type: 'pagerank' | 'betweenness' | 'closeness' | 'degree') {
    return CentralityAlgorithmFactory.create(type);
  }

  createPathAlgorithm(type: 'dijkstra' | 'astar' | 'floyd' | 'bellman') {
    return PathAlgorithmFactory.create(type);
  }

  createCommunityAlgorithm(type: 'louvain' | 'label_propagation') {
    return CommunityDetectionAlgorithmFactory.create(type);
  }

  createSimilarityAlgorithm(type: 'jaccard' | 'cosine' | 'adamic') {
    return SimilarityAlgorithmFactory.create(type);
  }
}

/**
 * 图算法工具函数
 */
export class GraphAlgorithmUtils {
  /**
   * 从边列表创建图
   */
  static fromEdgeList(
    edges: Array<{ source: string; target: string; type?: string; weight?: number }>,
  ): Graph {
    const builder = new GraphBuilder();

    for (const edge of edges) {
      builder.addEdge(edge.source, edge.target, edge.type, edge.weight);
    }

    return builder.build();
  }

  /**
   * 从邻接矩阵创建图
   */
  static fromAdjacencyMatrix(matrix: number[][], nodeIds?: string[]): Graph {
    const builder = new GraphBuilder();
    return builder.fromAdjacencyMatrix(matrix, nodeIds).build();
  }

  /**
   * 创建随机图
   */
  static createRandomGraph(nodeCount: number, edgeProbability: number): Graph {
    const builder = new GraphBuilder();
    return builder.random(nodeCount, edgeProbability).build();
  }

  /**
   * 创建完全图
   */
  static createCompleteGraph(nodeCount: number): Graph {
    const builder = new GraphBuilder();
    return builder.complete(nodeCount).build();
  }

  /**
   * 创建星形图
   */
  static createStarGraph(nodeCount: number): Graph {
    const builder = new GraphBuilder();
    return builder.star(nodeCount).build();
  }

  /**
   * 创建环形图
   */
  static createCycleGraph(nodeCount: number): Graph {
    const builder = new GraphBuilder();
    return builder.cycle(nodeCount).build();
  }

  /**
   * 图算法性能基准测试
   */
  static benchmark(graph: Graph, algorithms: string[] = []): Record<string, number> {
    const suite = new GraphAlgorithmSuiteImpl(graph);
    const results: Record<string, number> = {};

    const benchmarkAlgorithm = (name: string, fn: () => unknown) => {
      const start = performance.now();
      fn();
      const end = performance.now();
      results[name] = end - start;
    };

    if (algorithms.length === 0 || algorithms.includes('pagerank')) {
      benchmarkAlgorithm('pagerank', () => suite.centrality.pageRank());
    }

    if (algorithms.length === 0 || algorithms.includes('betweenness')) {
      benchmarkAlgorithm('betweenness', () => suite.centrality.betweenness());
    }

    if (algorithms.length === 0 || algorithms.includes('louvain')) {
      benchmarkAlgorithm('louvain', () => suite.community.louvain());
    }

    // 只对小图进行Dijkstra测试（避免超时）
    if (
      (algorithms.length === 0 || algorithms.includes('dijkstra')) &&
      graph.getNodes().length < 100
    ) {
      const nodes = graph.getNodes();
      if (nodes.length > 0) {
        benchmarkAlgorithm('dijkstra', () => suite.path.dijkstra(nodes[0].id));
      }
    }

    return results;
  }
}

/**
 * 默认工厂实例
 */
export const GraphAlgorithms = new GraphAlgorithmFactoryImpl();
