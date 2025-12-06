/**
 * 高级路径查找算法实现
 *
 * 提供基于图算法库的路径查找功能，包括Dijkstra、Floyd-Warshall、Bellman-Ford算法
 * 以及针对NervusDB优化的路径搜索实现
 */

import type {
  Graph,
  PathAlgorithm,
  PathOptions,
  Path,
  ShortestPathResult,
  GraphEdge,
} from './types.js';

/**
 * Dijkstra 最短路径算法实现
 * 适用于有权图的单源最短路径问题
 */
export class DijkstraPathAlgorithm implements PathAlgorithm {
  findShortestPath(
    graph: Graph,
    source: string,
    target: string,
    options: PathOptions = {},
  ): Path | null {
    const result = this.findShortestPaths(graph, source, options);
    const targetPath = result.paths.get(target);

    if (!targetPath || (options.maxHops && targetPath.length > options.maxHops)) {
      return null;
    }

    if (options.minHops && targetPath.length < options.minHops) {
      return null;
    }

    return targetPath;
  }

  findShortestPaths(graph: Graph, source: string, options: PathOptions = {}): ShortestPathResult {
    const { maxHops = Infinity, weightFunction = (edge: GraphEdge) => edge.weight || 1 } = options;

    const distances = new Map<string, number>();
    const paths = new Map<string, Path>();
    const visited = new Set<string>();
    const previousNodes = new Map<string, string>();
    const previousEdges = new Map<string, GraphEdge>();

    // 优先队列实现（使用最小堆）
    const priorityQueue = new PriorityQueue<{ nodeId: string; distance: number }>(
      (a, b) => a.distance - b.distance,
    );

    // 初始化
    const nodes = graph.getNodes();
    for (const node of nodes) {
      distances.set(node.id, node.id === source ? 0 : Infinity);
      paths.set(node.id, { nodes: [], edges: [], length: 0, weight: Infinity });
    }

    // 设置源节点
    distances.set(source, 0);
    paths.set(source, { nodes: [source], edges: [], length: 0, weight: 0 });
    priorityQueue.enqueue({ nodeId: source, distance: 0 });

    // 统计信息（用于返回）
    let nodesVisited = 0;
    let edgesExamined = 0;
    const startTime = performance.now();

    while (!priorityQueue.isEmpty()) {
      const current = priorityQueue.dequeue()!;
      const currentNodeId = current.nodeId;

      if (visited.has(currentNodeId)) {
        continue;
      }

      visited.add(currentNodeId);
      nodesVisited++;

      const currentDistance = distances.get(currentNodeId)!;
      const currentPath = paths.get(currentNodeId)!;

      // 达到最大跳数限制
      if (currentPath.length >= maxHops) {
        continue;
      }

      // 检查所有邻居
      const outEdges = graph.getOutEdges(currentNodeId);
      for (const edge of outEdges) {
        edgesExamined++;
        const neighborId = edge.target;
        const weight = weightFunction(edge);

        if (weight < 0) {
          throw new Error('Dijkstra算法不支持负权重边');
        }

        const newDistance = currentDistance + weight;
        const existingDistance = distances.get(neighborId)!;

        if (newDistance < existingDistance) {
          distances.set(neighborId, newDistance);
          previousNodes.set(neighborId, currentNodeId);
          previousEdges.set(neighborId, edge);

          // 构建路径
          const newPath: Path = {
            nodes: [...currentPath.nodes, neighborId],
            edges: [...currentPath.edges, edge],
            length: currentPath.length + 1,
            weight: newDistance,
          };
          paths.set(neighborId, newPath);

          priorityQueue.enqueue({ nodeId: neighborId, distance: newDistance });
        }
      }
    }

    const executionTime = performance.now() - startTime;

    return {
      distances,
      paths,
      stats: {
        nodesVisited,
        edgesExamined,
        executionTime,
      },
    };
  }

  findAllShortestPaths(graph: Graph, options: PathOptions = {}): Map<string, Map<string, number>> {
    const nodes = graph.getNodes();
    const allPairsDistances = new Map<string, Map<string, number>>();

    // 对每个节点运行Dijkstra算法
    for (const node of nodes) {
      const result = this.findShortestPaths(graph, node.id, options);
      allPairsDistances.set(node.id, result.distances);
    }

    return allPairsDistances;
  }
}

/**
 * A*算法实现，结合启发式函数的最短路径算法
 */
export class AStarPathAlgorithm implements PathAlgorithm {
  findShortestPath(
    graph: Graph,
    source: string,
    target: string,
    options: PathOptions & { heuristic?: (nodeId: string) => number } = {},
  ): Path | null {
    const {
      maxHops = Infinity,
      weightFunction = (edge: GraphEdge) => edge.weight || 1,
      heuristic = () => 0,
    } = options;

    const openSet = new PriorityQueue<AStarNode>((a, b) => a.fScore - b.fScore);
    const closedSet = new Set<string>();
    const gScore = new Map<string, number>();
    const cameFrom = new Map<string, { nodeId: string; edge: GraphEdge }>();

    // 初始化起始节点
    gScore.set(source, 0);
    openSet.enqueue({
      nodeId: source,
      gScore: 0,
      hScore: heuristic(source),
      fScore: heuristic(source),
      path: { nodes: [source], edges: [], length: 0, weight: 0 },
    });

    // 统计信息由上层统一计算，此处不单独记录

    while (!openSet.isEmpty()) {
      const current = openSet.dequeue()!;

      if (current.nodeId === target) {
        // 找到目标，重建路径
        return this.reconstructAStarPath(current, cameFrom, source);
      }

      if (closedSet.has(current.nodeId)) {
        continue;
      }

      closedSet.add(current.nodeId);

      // 达到最大跳数限制
      if (current.path.length >= maxHops) {
        continue;
      }

      // 检查邻居节点
      const outEdges = graph.getOutEdges(current.nodeId);
      for (const edge of outEdges) {
        // 处理邻居
        const neighborId = edge.target;

        if (closedSet.has(neighborId)) {
          continue;
        }

        const tentativeGScore = current.gScore + weightFunction(edge);
        const existingGScore = gScore.get(neighborId) || Infinity;

        if (tentativeGScore < existingGScore) {
          // 找到更好的路径
          gScore.set(neighborId, tentativeGScore);
          cameFrom.set(neighborId, { nodeId: current.nodeId, edge });

          const hScore = heuristic(neighborId);
          const fScore = tentativeGScore + hScore;

          const newPath: Path = {
            nodes: [...current.path.nodes, neighborId],
            edges: [...current.path.edges, edge],
            length: current.path.length + 1,
            weight: tentativeGScore,
          };

          openSet.enqueue({
            nodeId: neighborId,
            gScore: tentativeGScore,
            hScore,
            fScore,
            path: newPath,
          });
        }
      }
    }

    return null; // 未找到路径
  }

  private reconstructAStarPath(
    goalNode: AStarNode,
    cameFrom: Map<string, { nodeId: string; edge: GraphEdge }>,
    source: string,
  ): Path {
    // 当前实现直接返回构建中的路径，参数保留用于后续真实重建
    void cameFrom;
    void source;
    return goalNode.path;
  }

  findShortestPaths(graph: Graph, source: string, options?: PathOptions): ShortestPathResult {
    // A*主要用于单一目标的路径查找，对于单源多目标，回退到Dijkstra
    const dijkstra = new DijkstraPathAlgorithm();
    return dijkstra.findShortestPaths(graph, source, options);
  }

  findAllShortestPaths(graph: Graph, options?: PathOptions): Map<string, Map<string, number>> {
    const dijkstra = new DijkstraPathAlgorithm();
    return dijkstra.findAllShortestPaths(graph, options);
  }
}

/**
 * Floyd-Warshall算法实现，适用于稠密图的所有点对最短路径
 */
export class FloydWarshallPathAlgorithm implements PathAlgorithm {
  findShortestPath(
    graph: Graph,
    source: string,
    target: string,
    options: PathOptions = {},
  ): Path | null {
    const allPairs = this.findAllShortestPaths(graph, options);
    const distances = allPairs.get(source);

    if (!distances || !distances.has(target)) {
      return null;
    }

    const distance = distances.get(target)!;
    if (distance === Infinity) {
      return null;
    }

    // Floyd-Warshall通常不直接存储路径，这里提供简化实现
    return {
      nodes: [source, target],
      edges: [], // 实际实现中需要重建路径
      length: 1,
      weight: distance,
    };
  }

  findShortestPaths(graph: Graph, source: string, options: PathOptions = {}): ShortestPathResult {
    const allPairs = this.findAllShortestPaths(graph, options);
    const distances: Map<string, number> = allPairs.get(source) || new Map<string, number>();
    const paths = new Map<string, Path>();

    // 构建路径映射
    distances.forEach((distance, target) => {
      if (distance !== Infinity && target !== source) {
        paths.set(target, {
          nodes: [source, target],
          edges: [],
          length: 1,
          weight: distance,
        });
      }
    });

    return {
      distances,
      paths,
      stats: {
        nodesVisited: 0,
        edgesExamined: 0,
        executionTime: 0,
      },
    };
  }

  findAllShortestPaths(graph: Graph, options: PathOptions = {}): Map<string, Map<string, number>> {
    const { weightFunction = (edge: GraphEdge) => edge.weight || 1 } = options;

    const nodes = graph.getNodes();
    const nodeList = nodes.map((n) => n.id);
    const n = nodeList.length;

    // 初始化距离矩阵
    const dist: number[][] = Array.from({ length: n }, () => new Array<number>(n).fill(Infinity));
    const nodeIndexMap = new Map<string, number>();

    nodeList.forEach((nodeId, index) => {
      nodeIndexMap.set(nodeId, index);
      dist[index][index] = 0; // 对角线为0
    });

    // 填充直接边的权重
    const edges = graph.getEdges();
    for (const edge of edges) {
      const sourceIndex = nodeIndexMap.get(edge.source)!;
      const targetIndex = nodeIndexMap.get(edge.target)!;
      const weight = weightFunction(edge);

      dist[sourceIndex][targetIndex] = Math.min(dist[sourceIndex][targetIndex], weight);

      // 对于无向图，添加反向边
      if (!edge.directed) {
        dist[targetIndex][sourceIndex] = Math.min(dist[targetIndex][sourceIndex], weight);
      }
    }

    // Floyd-Warshall核心算法
    for (let k = 0; k < n; k++) {
      for (let i = 0; i < n; i++) {
        for (let j = 0; j < n; j++) {
          if (dist[i][k] !== Infinity && dist[k][j] !== Infinity) {
            dist[i][j] = Math.min(dist[i][j], dist[i][k] + dist[k][j]);
          }
        }
      }
    }

    // 转换回Map格式
    const result = new Map<string, Map<string, number>>();
    for (let i = 0; i < n; i++) {
      const sourceDistances = new Map<string, number>();
      for (let j = 0; j < n; j++) {
        sourceDistances.set(nodeList[j], dist[i][j]);
      }
      result.set(nodeList[i], sourceDistances);
    }

    return result;
  }
}

/**
 * Bellman-Ford算法实现，支持负权重边的单源最短路径
 */
export class BellmanFordPathAlgorithm implements PathAlgorithm {
  findShortestPath(
    graph: Graph,
    source: string,
    target: string,
    options: PathOptions = {},
  ): Path | null {
    const result = this.findShortestPaths(graph, source, options);
    return result.paths.get(target) || null;
  }

  findShortestPaths(graph: Graph, source: string, options: PathOptions = {}): ShortestPathResult {
    const { maxIterations = 1000, weightFunction = (edge: GraphEdge) => edge.weight || 1 } =
      options;

    const nodes = graph.getNodes();
    const edges = graph.getEdges();
    const distances = new Map<string, number>();
    const predecessors = new Map<string, { nodeId: string; edge: GraphEdge }>();

    let nodesVisited = 0;
    let edgesExamined = 0;
    const startTime = performance.now();

    // 初始化距离
    nodes.forEach((node) => {
      distances.set(node.id, node.id === source ? 0 : Infinity);
    });

    // 松弛操作（最多进行V-1次迭代）
    const maxIterationsActual = Math.min(maxIterations, nodes.length - 1);

    for (let iteration = 0; iteration < maxIterationsActual; iteration++) {
      let hasUpdate = false;

      for (const edge of edges) {
        edgesExamined++;
        const sourceDistance = distances.get(edge.source)!;
        const targetDistance = distances.get(edge.target)!;
        const weight = weightFunction(edge);

        if (sourceDistance !== Infinity && sourceDistance + weight < targetDistance) {
          distances.set(edge.target, sourceDistance + weight);
          predecessors.set(edge.target, { nodeId: edge.source, edge });
          hasUpdate = true;
          nodesVisited++;
        }

        // 处理无向图
        if (!edge.directed) {
          const reverseSourceDistance = distances.get(edge.target)!;
          const reverseTargetDistance = distances.get(edge.source)!;

          if (
            reverseSourceDistance !== Infinity &&
            reverseSourceDistance + weight < reverseTargetDistance
          ) {
            distances.set(edge.source, reverseSourceDistance + weight);
            predecessors.set(edge.source, { nodeId: edge.target, edge });
            hasUpdate = true;
            nodesVisited++;
          }
        }
      }

      if (!hasUpdate) {
        break; // 提前收敛
      }
    }

    // 检查负权重回路
    for (const edge of edges) {
      const sourceDistance = distances.get(edge.source)!;
      const targetDistance = distances.get(edge.target)!;
      const weight = weightFunction(edge);

      if (sourceDistance !== Infinity && sourceDistance + weight < targetDistance) {
        throw new Error('图中存在负权重回路');
      }
    }

    // 重建路径
    const paths = new Map<string, Path>();
    for (const node of nodes) {
      if (node.id !== source && distances.get(node.id) !== Infinity) {
        const path = this.reconstructBellmanFordPath(node.id, source, predecessors);
        if (path) {
          path.weight = distances.get(node.id)!;
          paths.set(node.id, path);
        }
      }
    }

    const executionTime = performance.now() - startTime;

    return {
      distances,
      paths,
      stats: {
        nodesVisited,
        edgesExamined,
        executionTime,
      },
    };
  }

  private reconstructBellmanFordPath(
    target: string,
    source: string,
    predecessors: Map<string, { nodeId: string; edge: GraphEdge }>,
  ): Path | null {
    const pathNodes: string[] = [];
    const pathEdges: GraphEdge[] = [];
    let current = target;

    while (current !== source) {
      pathNodes.unshift(current);
      const predecessor = predecessors.get(current);

      if (!predecessor) {
        return null; // 无法到达
      }

      pathEdges.unshift(predecessor.edge);
      current = predecessor.nodeId;
    }

    pathNodes.unshift(source);

    return {
      nodes: pathNodes,
      edges: pathEdges,
      length: pathEdges.length,
      weight: 0, // 将在调用处设置
    };
  }

  findAllShortestPaths(graph: Graph, options: PathOptions = {}): Map<string, Map<string, number>> {
    const nodes = graph.getNodes();
    const allPairsDistances = new Map<string, Map<string, number>>();

    // 对每个节点运行Bellman-Ford算法
    for (const node of nodes) {
      const result = this.findShortestPaths(graph, node.id, options);
      allPairsDistances.set(node.id, result.distances);
    }

    return allPairsDistances;
  }
}

/**
 * A*节点接口
 */
interface AStarNode {
  nodeId: string;
  gScore: number;
  hScore: number;
  fScore: number;
  path: Path;
}

/**
 * 优先队列实现
 */
class PriorityQueue<T> {
  private heap: T[] = [];
  private compare: (a: T, b: T) => number;

  constructor(compareFn: (a: T, b: T) => number) {
    this.compare = compareFn;
  }

  enqueue(item: T): void {
    this.heap.push(item);
    this.bubbleUp(this.heap.length - 1);
  }

  dequeue(): T | undefined {
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
 * 路径算法工厂
 */
export class PathAlgorithmFactory {
  /**
   * 创建Dijkstra算法实例
   */
  static createDijkstra(): DijkstraPathAlgorithm {
    return new DijkstraPathAlgorithm();
  }

  /**
   * 创建A*算法实例
   */
  static createAStar(): AStarPathAlgorithm {
    return new AStarPathAlgorithm();
  }

  /**
   * 创建Floyd-Warshall算法实例
   */
  static createFloydWarshall(): FloydWarshallPathAlgorithm {
    return new FloydWarshallPathAlgorithm();
  }

  /**
   * 创建Bellman-Ford算法实例
   */
  static createBellmanFord(): BellmanFordPathAlgorithm {
    return new BellmanFordPathAlgorithm();
  }

  /**
   * 根据类型创建算法实例
   */
  static create(type: 'dijkstra' | 'astar' | 'floyd' | 'bellman'): PathAlgorithm {
    switch (type) {
      case 'dijkstra':
        return this.createDijkstra();
      case 'astar':
        return this.createAStar();
      case 'floyd':
        return this.createFloydWarshall();
      case 'bellman':
        return this.createBellmanFord();
      default:
        // 理论上不可达，类型已穷尽
        throw new Error('未知的路径算法类型');
    }
  }

  /**
   * 根据图的特性自动选择最适合的算法
   */
  static createOptimal(
    graph: Graph,
    sourceCount: number = 1,
    targetCount: number = 1,
    hasNegativeWeights: boolean = false,
  ): PathAlgorithm {
    const nodeCount = graph.getNodes().length;
    const edgeCount = graph.getEdges().length;
    const density = edgeCount / (nodeCount * (nodeCount - 1));

    // 有负权重边时使用Bellman-Ford
    if (hasNegativeWeights) {
      return this.createBellmanFord();
    }

    // 稠密图且需要所有点对距离时使用Floyd-Warshall
    if (density > 0.5 && sourceCount > nodeCount * 0.8) {
      return this.createFloydWarshall();
    }

    // 单源单目标且有启发式信息时使用A*
    if (sourceCount === 1 && targetCount === 1) {
      return this.createAStar();
    }

    // 其他情况使用Dijkstra
    return this.createDijkstra();
  }
}
