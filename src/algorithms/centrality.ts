/**
 * 中心性算法实现
 *
 * 提供各种图中心性分析算法，包括PageRank、中介中心性、接近中心性等
 */

import {
  Graph,
  AlgorithmOptions,
  CentralityResult,
  CentralityAlgorithm,
  PageRankOptions,
} from './types.js';

/**
 * PageRank 算法实现
 */
export class PageRankCentrality implements CentralityAlgorithm {
  compute(graph: Graph, options: PageRankOptions = {}): CentralityResult {
    const {
      dampingFactor = 0.85,
      maxIterations = 100,
      tolerance = 1e-6,
      personalization,
    } = options;

    const nodes = graph.getNodes();
    const nodeCount = nodes.length;

    if (nodeCount === 0) {
      return {
        values: new Map(),
        ranking: [],
        stats: { mean: 0, max: 0, min: 0, standardDeviation: 0 },
      };
    }

    // 初始化 PageRank 值
    const pageRank = new Map<string, number>();
    const newPageRank = new Map<string, number>();
    const initialValue = 1.0 / nodeCount;

    nodes.forEach((node) => {
      pageRank.set(node.id, initialValue);
      newPageRank.set(node.id, 0);
    });

    // 计算出度
    const outDegree = new Map<string, number>();
    nodes.forEach((node) => {
      outDegree.set(node.id, graph.getOutDegree(node.id));
    });

    // 迭代计算
    for (let iteration = 0; iteration < maxIterations; iteration++) {
      let totalDiff = 0;

      // 重置新值
      nodes.forEach((node) => {
        newPageRank.set(node.id, 0);
      });

      // 计算新的 PageRank 值
      nodes.forEach((node) => {
        const nodeId = node.id;
        let rank = (1 - dampingFactor) / nodeCount;

        // 个性化向量支持
        if (personalization && personalization.has(nodeId)) {
          rank = (1 - dampingFactor) * personalization.get(nodeId)!;
        }

        // 从入链节点获取 PageRank 值
        const inEdges = graph.getInEdges(nodeId);
        for (const edge of inEdges) {
          const sourceOutDegree = outDegree.get(edge.source) || 1;
          const sourceRank = pageRank.get(edge.source) || 0;
          rank += dampingFactor * (sourceRank / sourceOutDegree);
        }

        newPageRank.set(nodeId, rank);
        totalDiff += Math.abs(rank - (pageRank.get(nodeId) || 0));
      });

      // 更新值
      nodes.forEach((node) => {
        pageRank.set(node.id, newPageRank.get(node.id)!);
      });

      // 检查收敛
      if (totalDiff < tolerance) {
        break;
      }
    }

    return this.buildCentralityResult(pageRank);
  }

  computeNode(graph: Graph, nodeId: string, options?: PageRankOptions): number {
    const result = this.compute(graph, options);
    return result.values.get(nodeId) || 0;
  }

  private buildCentralityResult(values: Map<string, number>): CentralityResult {
    const ranking = Array.from(values.entries())
      .map(([nodeId, value]) => ({ nodeId, value }))
      .sort((a, b) => b.value - a.value);

    const valueArray = Array.from(values.values());
    const mean = valueArray.reduce((sum, val) => sum + val, 0) / valueArray.length;
    const max = Math.max(...valueArray);
    const min = Math.min(...valueArray);

    const variance =
      valueArray.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) / valueArray.length;
    const standardDeviation = Math.sqrt(variance);

    return {
      values,
      ranking,
      stats: { mean, max, min, standardDeviation },
    };
  }
}

/**
 * 中介中心性算法实现
 * 基于 Brandes 算法的高效实现
 */
export class BetweennessCentrality implements CentralityAlgorithm {
  compute(graph: Graph): CentralityResult {
    const nodes = graph.getNodes();
    const betweenness = new Map<string, number>();

    // 初始化中介中心性值
    nodes.forEach((node) => {
      betweenness.set(node.id, 0);
    });

    // 对每个节点作为源节点进行 Brandes 算法
    for (const source of nodes) {
      this.brandesAlgorithm(graph, source.id, betweenness);
    }

    // 标准化：对于无向图除以2，对于有向图保持原值
    const isDirected = this.isDirectedGraph(graph);
    if (!isDirected) {
      betweenness.forEach((value, nodeId) => {
        betweenness.set(nodeId, value / 2);
      });
    }

    return this.buildCentralityResult(betweenness);
  }

  computeNode(graph: Graph, nodeId: string, options?: AlgorithmOptions): number {
    // 该算法不使用 options，但接口要求保留
    void options;
    const result = this.compute(graph);
    return result.values.get(nodeId) || 0;
  }

  /**
   * Brandes 算法实现
   */
  private brandesAlgorithm(graph: Graph, sourceId: string, betweenness: Map<string, number>): void {
    const nodes = graph.getNodes();
    const stack: string[] = [];
    const paths = new Map<string, number>();
    const distances = new Map<string, number>();
    const predecessors = new Map<string, string[]>();
    const dependency = new Map<string, number>();

    // 初始化
    nodes.forEach((node) => {
      paths.set(node.id, 0);
      distances.set(node.id, -1);
      predecessors.set(node.id, []);
      dependency.set(node.id, 0);
    });

    paths.set(sourceId, 1);
    distances.set(sourceId, 0);

    // BFS 阶段
    const queue: string[] = [sourceId];
    let queueIndex = 0;

    while (queueIndex < queue.length) {
      const currentNode = queue[queueIndex++];
      stack.push(currentNode);

      const neighbors = graph.getNeighbors(currentNode);
      for (const neighbor of neighbors) {
        const neighborId = neighbor.id;

        // 首次访问邻居节点
        if (distances.get(neighborId)! < 0) {
          queue.push(neighborId);
          distances.set(neighborId, distances.get(currentNode)! + 1);
        }

        // 找到另一条最短路径
        if (distances.get(neighborId)! === distances.get(currentNode)! + 1) {
          paths.set(neighborId, paths.get(neighborId)! + paths.get(currentNode)!);
          predecessors.get(neighborId)!.push(currentNode);
        }
      }
    }

    // 依赖性累积阶段
    while (stack.length > 0) {
      const node = stack.pop()!;
      const nodePredecessors = predecessors.get(node)!;

      for (const predecessor of nodePredecessors) {
        const pathRatio = paths.get(predecessor)! / paths.get(node)!;
        const dep = pathRatio * (1 + dependency.get(node)!);
        dependency.set(predecessor, dependency.get(predecessor)! + dep);
      }

      if (node !== sourceId) {
        betweenness.set(node, betweenness.get(node)! + dependency.get(node)!);
      }
    }
  }

  private isDirectedGraph(graph: Graph): boolean {
    // 检查是否存在有向边
    const edges = graph.getEdges();
    return edges.some((edge) => edge.directed !== false);
  }

  private buildCentralityResult(values: Map<string, number>): CentralityResult {
    const ranking = Array.from(values.entries())
      .map(([nodeId, value]) => ({ nodeId, value }))
      .sort((a, b) => b.value - a.value);

    const valueArray = Array.from(values.values());
    const mean = valueArray.reduce((sum, val) => sum + val, 0) / valueArray.length;
    const max = Math.max(...valueArray);
    const min = Math.min(...valueArray);

    const variance =
      valueArray.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) / valueArray.length;
    const standardDeviation = Math.sqrt(variance);

    return {
      values,
      ranking,
      stats: { mean, max, min, standardDeviation },
    };
  }
}

/**
 * 接近中心性算法实现
 */
export class ClosenessCentrality implements CentralityAlgorithm {
  compute(graph: Graph, options: AlgorithmOptions = {}): CentralityResult {
    // 当前实现未使用 options，保留以兼容接口
    void options;
    const nodes = graph.getNodes();
    const closeness = new Map<string, number>();

    // 计算每个节点的接近中心性
    for (const node of nodes) {
      const distances = this.dijkstraDistances(graph, node.id);
      const reachableDistances = Array.from(distances.values()).filter(
        (d) => d > 0 && d !== Infinity,
      );

      if (reachableDistances.length === 0) {
        closeness.set(node.id, 0);
      } else {
        const totalDistance = reachableDistances.reduce((sum, d) => sum + d, 0);
        const reachableNodes = reachableDistances.length;

        // 标准化接近中心性：可达节点数 / 总距离
        const normalizedCloseness =
          (reachableNodes * reachableNodes) / (totalDistance * (nodes.length - 1));

        closeness.set(node.id, normalizedCloseness);
      }
    }

    return this.buildCentralityResult(closeness);
  }

  computeNode(graph: Graph, nodeId: string, options?: AlgorithmOptions): number {
    const result = this.compute(graph, options);
    return result.values.get(nodeId) || 0;
  }

  /**
   * 使用 Dijkstra 算法计算单源最短距离
   */
  private dijkstraDistances(graph: Graph, sourceId: string): Map<string, number> {
    const distances = new Map<string, number>();
    const visited = new Set<string>();
    const priorityQueue = new MinHeap<{ nodeId: string; distance: number }>();

    // 初始化距离
    graph.getNodes().forEach((node) => {
      distances.set(node.id, node.id === sourceId ? 0 : Infinity);
    });

    priorityQueue.insert({ nodeId: sourceId, distance: 0 });

    while (!priorityQueue.isEmpty()) {
      const current = priorityQueue.extract()!;

      if (visited.has(current.nodeId)) {
        continue;
      }

      visited.add(current.nodeId);
      const currentDistance = distances.get(current.nodeId)!;

      // 更新邻居距离
      const neighbors = graph.getNeighbors(current.nodeId);
      for (const neighbor of neighbors) {
        if (visited.has(neighbor.id)) {
          continue;
        }

        // 获取边权重
        const outEdges = graph.getOutEdges(current.nodeId);
        const edge = outEdges.find((e) => e.target === neighbor.id);
        const weight = edge?.weight || 1;

        const newDistance = currentDistance + weight;
        const currentNeighborDistance = distances.get(neighbor.id)!;

        if (newDistance < currentNeighborDistance) {
          distances.set(neighbor.id, newDistance);
          priorityQueue.insert({ nodeId: neighbor.id, distance: newDistance });
        }
      }
    }

    return distances;
  }

  private buildCentralityResult(values: Map<string, number>): CentralityResult {
    const ranking = Array.from(values.entries())
      .map(([nodeId, value]) => ({ nodeId, value }))
      .sort((a, b) => b.value - a.value);

    const valueArray = Array.from(values.values());
    const mean = valueArray.reduce((sum, val) => sum + val, 0) / valueArray.length;
    const max = Math.max(...valueArray);
    const min = Math.min(...valueArray);

    const variance =
      valueArray.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) / valueArray.length;
    const standardDeviation = Math.sqrt(variance);

    return {
      values,
      ranking,
      stats: { mean, max, min, standardDeviation },
    };
  }
}

/**
 * 度中心性算法实现
 */
export class DegreeCentrality implements CentralityAlgorithm {
  compute(graph: Graph): CentralityResult {
    const nodes = graph.getNodes();
    const degree = new Map<string, number>();

    // 计算每个节点的度数
    nodes.forEach((node) => {
      degree.set(node.id, graph.getDegree(node.id));
    });

    return this.buildCentralityResult(degree);
  }

  computeNode(graph: Graph, nodeId: string): number {
    return graph.getDegree(nodeId);
  }

  private buildCentralityResult(values: Map<string, number>): CentralityResult {
    const ranking = Array.from(values.entries())
      .map(([nodeId, value]) => ({ nodeId, value }))
      .sort((a, b) => b.value - a.value);

    const valueArray = Array.from(values.values());
    const mean = valueArray.reduce((sum, val) => sum + val, 0) / valueArray.length;
    const max = Math.max(...valueArray);
    const min = Math.min(...valueArray);

    const variance =
      valueArray.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) / valueArray.length;
    const standardDeviation = Math.sqrt(variance);

    return {
      values,
      ranking,
      stats: { mean, max, min, standardDeviation },
    };
  }
}

/**
 * 特征向量中心性算法实现
 */
export class EigenvectorCentrality implements CentralityAlgorithm {
  compute(graph: Graph, options: AlgorithmOptions = {}): CentralityResult {
    const { maxIterations = 1000, tolerance = 1e-6 } = options;

    const nodes = graph.getNodes();
    const nodeCount = nodes.length;

    if (nodeCount === 0) {
      return {
        values: new Map(),
        ranking: [],
        stats: { mean: 0, max: 0, min: 0, standardDeviation: 0 },
      };
    }

    // 构建邻接矩阵
    const nodeIndexMap = new Map<string, number>();
    nodes.forEach((node, index) => {
      nodeIndexMap.set(node.id, index);
    });

    const adjacencyMatrix: number[][] = Array.from({ length: nodeCount }, () =>
      new Array<number>(nodeCount).fill(0),
    );

    const edges = graph.getEdges();
    edges.forEach((edge) => {
      const sourceIndex = nodeIndexMap.get(edge.source)!;
      const targetIndex = nodeIndexMap.get(edge.target)!;
      adjacencyMatrix[sourceIndex][targetIndex] = edge.weight || 1;

      // 对于无向图，添加反向边
      if (!edge.directed) {
        adjacencyMatrix[targetIndex][sourceIndex] = edge.weight || 1;
      }
    });

    // 幂迭代算法求解特征向量
    let eigenvector: number[] = new Array<number>(nodeCount).fill(1.0 / Math.sqrt(nodeCount));

    for (let iteration = 0; iteration < maxIterations; iteration++) {
      const newEigenvector: number[] = new Array<number>(nodeCount).fill(0);

      // 矩阵向量乘法
      for (let i = 0; i < nodeCount; i++) {
        for (let j = 0; j < nodeCount; j++) {
          newEigenvector[i] += adjacencyMatrix[i][j] * eigenvector[j];
        }
      }

      // 归一化
      const norm = Math.sqrt(newEigenvector.reduce((sum, val) => sum + val * val, 0));

      if (norm === 0) break;

      for (let i = 0; i < nodeCount; i++) {
        newEigenvector[i] /= norm;
      }

      // 检查收敛
      const diff = newEigenvector.reduce((sum, val, i) => sum + Math.abs(val - eigenvector[i]), 0);

      eigenvector = newEigenvector;

      if (diff < tolerance) {
        break;
      }
    }

    // 构建结果映射
    const values = new Map<string, number>();
    nodes.forEach((node, index) => {
      values.set(node.id, Math.abs(eigenvector[index]));
    });

    return this.buildCentralityResult(values);
  }

  computeNode(graph: Graph, nodeId: string, options?: AlgorithmOptions): number {
    const result = this.compute(graph, options);
    return result.values.get(nodeId) || 0;
  }

  private buildCentralityResult(values: Map<string, number>): CentralityResult {
    const ranking = Array.from(values.entries())
      .map(([nodeId, value]) => ({ nodeId, value }))
      .sort((a, b) => b.value - a.value);

    const valueArray = Array.from(values.values());
    const mean = valueArray.reduce((sum, val) => sum + val, 0) / valueArray.length;
    const max = Math.max(...valueArray);
    const min = Math.min(...valueArray);

    const variance =
      valueArray.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) / valueArray.length;
    const standardDeviation = Math.sqrt(variance);

    return {
      values,
      ranking,
      stats: { mean, max, min, standardDeviation },
    };
  }
}

/**
 * 最小堆实现（用于 Dijkstra 算法）
 */
class MinHeap<T extends { distance: number }> {
  private heap: T[] = [];
  private compare: (a: T, b: T) => number;

  constructor(compareFn?: (a: T, b: T) => number) {
    this.compare = compareFn || ((a: T, b: T) => a.distance - b.distance);
  }

  insert(item: T): void {
    this.heap.push(item);
    this.bubbleUp(this.heap.length - 1);
  }

  extract(): T | undefined {
    if (this.heap.length === 0) return undefined;

    const min = this.heap[0];
    const last = this.heap.pop()!;

    if (this.heap.length > 0) {
      this.heap[0] = last;
      this.bubbleDown(0);
    }

    return min;
  }

  isEmpty(): boolean {
    return this.heap.length === 0;
  }

  private bubbleUp(index: number): void {
    while (index > 0) {
      const parentIndex = Math.floor((index - 1) / 2);

      if (this.compare(this.heap[index], this.heap[parentIndex]) >= 0) {
        break;
      }

      [this.heap[index], this.heap[parentIndex]] = [this.heap[parentIndex], this.heap[index]];
      index = parentIndex;
    }
  }

  private bubbleDown(index: number): void {
    while (true) {
      const leftChild = 2 * index + 1;
      const rightChild = 2 * index + 2;
      let smallest = index;

      if (
        leftChild < this.heap.length &&
        this.compare(this.heap[leftChild], this.heap[smallest]) < 0
      ) {
        smallest = leftChild;
      }

      if (
        rightChild < this.heap.length &&
        this.compare(this.heap[rightChild], this.heap[smallest]) < 0
      ) {
        smallest = rightChild;
      }

      if (smallest === index) break;

      [this.heap[index], this.heap[smallest]] = [this.heap[smallest], this.heap[index]];
      index = smallest;
    }
  }
}

/**
 * 中心性算法工厂
 */
export class CentralityAlgorithmFactory {
  /**
   * 创建 PageRank 算法实例
   */
  static createPageRank(): PageRankCentrality {
    return new PageRankCentrality();
  }

  /**
   * 创建中介中心性算法实例
   */
  static createBetweenness(): BetweennessCentrality {
    return new BetweennessCentrality();
  }

  /**
   * 创建接近中心性算法实例
   */
  static createCloseness(): ClosenessCentrality {
    return new ClosenessCentrality();
  }

  /**
   * 创建度中心性算法实例
   */
  static createDegree(): DegreeCentrality {
    return new DegreeCentrality();
  }

  /**
   * 创建特征向量中心性算法实例
   */
  static createEigenvector(): EigenvectorCentrality {
    return new EigenvectorCentrality();
  }

  /**
   * 根据类型创建算法实例
   */
  static create(
    type: 'pagerank' | 'betweenness' | 'closeness' | 'degree' | 'eigenvector',
  ): CentralityAlgorithm {
    switch (type) {
      case 'pagerank':
        return this.createPageRank();
      case 'betweenness':
        return this.createBetweenness();
      case 'closeness':
        return this.createCloseness();
      case 'degree':
        return this.createDegree();
      case 'eigenvector':
        return this.createEigenvector();
      default:
        // 理论上不可达，类型已穷尽
        throw new Error('未知的中心性算法类型');
    }
  }
}
