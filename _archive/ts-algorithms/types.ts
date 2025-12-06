/**
 * 图算法库类型定义
 *
 * 为图算法提供通用的数据结构和接口定义
 */

// 图节点定义
export interface GraphNode {
  /** 节点唯一标识 */
  id: string;
  /** 节点值/标签 */
  value: string;
  /** 节点属性 */
  properties?: Record<string, unknown>;
  /** 节点标签 */
  labels?: string[];
}

// 图边定义
export interface GraphEdge {
  /** 源节点ID */
  source: string;
  /** 目标节点ID */
  target: string;
  /** 边类型/谓词 */
  type: string;
  /** 边权重 */
  weight?: number;
  /** 边属性 */
  properties?: Record<string, unknown>;
  /** 是否有向边 */
  directed?: boolean;
}

// 路径结果
export interface Path {
  /** 路径上的节点序列 */
  nodes: string[];
  /** 路径上的边序列 */
  edges: GraphEdge[];
  /** 路径总长度 */
  length: number;
  /** 路径总权重 */
  weight: number;
}

// 最短路径结果
export interface ShortestPathResult {
  /** 从源节点到各节点的距离 */
  distances: Map<string, number>;
  /** 路径重建信息 */
  paths: Map<string, Path>;
  /** 算法执行统计 */
  stats?: {
    nodesVisited: number;
    edgesExamined: number;
    executionTime: number;
  };
}

// 中心性分析结果
export interface CentralityResult {
  /** 节点ID到中心性值的映射 */
  values: Map<string, number>;
  /** 排序后的节点列表 */
  ranking: Array<{ nodeId: string; value: number }>;
  /** 统计信息 */
  stats: {
    mean: number;
    max: number;
    min: number;
    standardDeviation: number;
  };
}

// 社区发现结果
export interface CommunityResult {
  /** 节点到社区的映射 */
  communities: Map<string, number>;
  /** 社区层次结构 */
  hierarchy: CommunityLevel[];
  /** 模块度值 */
  modularity: number;
  /** 社区数量 */
  communityCount: number;
}

// 社区层次
export interface CommunityLevel {
  /** 层次级别 */
  level: number;
  /** 该级别的社区划分 */
  communities: Map<string, number>;
  /** 该级别的模块度 */
  modularity: number;
}

// 相似度计算结果
export interface SimilarityResult {
  /** 节点对相似度映射 */
  similarities: Map<string, Map<string, number>>;
  /** 最相似的节点对 */
  topPairs: Array<{
    node1: string;
    node2: string;
    similarity: number;
  }>;
}

// 图统计信息
export interface GraphStats {
  /** 节点数量 */
  nodeCount: number;
  /** 边数量 */
  edgeCount: number;
  /** 平均度数 */
  averageDegree: number;
  /** 图密度 */
  density: number;
  /** 直径（最长最短路径） */
  diameter?: number;
  /** 聚类系数 */
  clusteringCoefficient?: number;
  /** 是否连通 */
  isConnected: boolean;
  /** 连通分量数 */
  componentCount: number;
}

// 图接口定义
export interface Graph {
  /** 添加节点 */
  addNode(node: GraphNode): void;

  /** 删除节点 */
  removeNode(nodeId: string): void;

  /** 添加边 */
  addEdge(edge: GraphEdge): void;

  /** 删除边 */
  removeEdge(source: string, target: string): void;

  /** 获取节点 */
  getNode(nodeId: string): GraphNode | undefined;

  /** 获取所有节点 */
  getNodes(): GraphNode[];

  /** 获取节点的所有邻居 */
  getNeighbors(nodeId: string): GraphNode[];

  /** 获取节点的出边 */
  getOutEdges(nodeId: string): GraphEdge[];

  /** 获取节点的入边 */
  getInEdges(nodeId: string): GraphEdge[];

  /** 获取所有边 */
  getEdges(): GraphEdge[];

  /** 获取节点度数 */
  getDegree(nodeId: string): number;

  /** 获取节点出度 */
  getOutDegree(nodeId: string): number;

  /** 获取节点入度 */
  getInDegree(nodeId: string): number;

  /** 检查节点是否存在 */
  hasNode(nodeId: string): boolean;

  /** 检查边是否存在 */
  hasEdge(source: string, target: string): boolean;

  /** 获取图统计信息 */
  getStats(): GraphStats;

  /** 清空图 */
  clear(): void;

  /** 克隆图 */
  clone(): Graph;
}

// 算法选项接口
export interface AlgorithmOptions {
  /** 最大迭代次数 */
  maxIterations?: number;
  /** 收敛容忍度 */
  tolerance?: number;
  /** 是否启用并行计算 */
  parallel?: boolean;
  /** 随机种子 */
  seed?: number;
  /** 算法特定参数 */
  [key: string]: unknown;
}

// PageRank 算法选项
export interface PageRankOptions extends AlgorithmOptions {
  /** 阻尼因子 */
  dampingFactor?: number;
  /** 个性化向量 */
  personalization?: Map<string, number>;
}

// Louvain 算法选项
export interface LouvainOptions extends AlgorithmOptions {
  /** 分辨率参数 */
  resolution?: number;
  /** 随机化程度 */
  randomness?: number;
}

// 路径查找选项
export interface PathOptions extends AlgorithmOptions {
  /** 最大跳数 */
  maxHops?: number;
  /** 最小跳数 */
  minHops?: number;
  /** 唯一性约束 */
  uniqueness?: 'NODE' | 'EDGE' | 'NONE';
  /** 权重函数 */
  weightFunction?: (edge: GraphEdge) => number;
}

// 中心性算法接口
export interface CentralityAlgorithm {
  /** 计算所有节点的中心性 */
  compute(graph: Graph, options?: AlgorithmOptions): CentralityResult;

  /** 计算单个节点的中心性 */
  computeNode(graph: Graph, nodeId: string, options?: AlgorithmOptions): number;
}

// 路径算法接口
export interface PathAlgorithm {
  /** 查找两点间的最短路径 */
  findShortestPath(
    graph: Graph,
    source: string,
    target: string,
    options?: PathOptions,
  ): Path | null;

  /** 查找单源最短路径 */
  findShortestPaths(graph: Graph, source: string, options?: PathOptions): ShortestPathResult;

  /** 查找所有节点对最短路径 */
  findAllShortestPaths(graph: Graph, options?: PathOptions): Map<string, Map<string, number>>;
}

// 社区发现算法接口
export interface CommunityDetectionAlgorithm {
  /** 发现社区结构 */
  detectCommunities(graph: Graph, options?: AlgorithmOptions): CommunityResult;

  /** 计算模块度 */
  calculateModularity(graph: Graph, communities: Map<string, number>): number;
}

// 相似度算法接口
export interface SimilarityAlgorithm {
  /** 计算两个节点的相似度 */
  computeSimilarity(graph: Graph, node1: string, node2: string): number;

  /** 计算所有节点对的相似度 */
  computeAllSimilarities(graph: Graph, threshold?: number): SimilarityResult;

  /** 找到与目标节点最相似的k个节点 */
  findMostSimilar(
    graph: Graph,
    targetNode: string,
    k: number,
  ): Array<{
    nodeId: string;
    similarity: number;
  }>;
}

// 图算法套件接口
export interface GraphAlgorithmSuite {
  /** 中心性算法 */
  centrality: {
    pageRank(options?: PageRankOptions): CentralityResult;
    betweenness(options?: AlgorithmOptions): CentralityResult;
    closeness(options?: AlgorithmOptions): CentralityResult;
    degree(options?: AlgorithmOptions): CentralityResult;
    eigenvector(options?: AlgorithmOptions): CentralityResult;
  };

  /** 路径算法 */
  path: {
    dijkstra(source: string, target?: string, options?: PathOptions): ShortestPathResult;
    astar(
      source: string,
      target: string,
      heuristic?: (nodeId: string) => number,
      options?: PathOptions,
    ): Path | null;
    floydWarshall(options?: PathOptions): Map<string, Map<string, number>>;
    bellmanFord(source: string, options?: PathOptions): ShortestPathResult;
  };

  /** 社区发现算法 */
  community: {
    louvain(options?: LouvainOptions): CommunityResult;
    labelPropagation(options?: AlgorithmOptions): CommunityResult;
    connectedComponents(): CommunityResult;
    stronglyConnectedComponents(): CommunityResult;
  };

  /** 相似度算法 */
  similarity: {
    jaccard(node1: string, node2: string): number;
    cosine(node1: string, node2: string): number;
    adamic(node1: string, node2: string): number;
    preferentialAttachment(node1: string, node2: string): number;
  };

  /** 图分析 */
  analysis: {
    getStats(): GraphStats;
    findBridges(): GraphEdge[];
    findArticulationPoints(): string[];
    detectCycles(): Path[];
    topologicalSort(): string[] | null;
  };
}

// 算法执行上下文
export interface AlgorithmContext {
  /** 图实例 */
  graph: Graph;
  /** 算法选项 */
  options: AlgorithmOptions;
  /** 执行统计 */
  stats: {
    startTime: number;
    endTime?: number;
    memoryUsage?: number;
    iterations?: number;
  };
  /** 取消标记 */
  cancelled: boolean;
  /** 进度回调 */
  onProgress?: (progress: number) => void;
}

// 算法结果包装器
export interface AlgorithmResult<T> {
  /** 算法结果 */
  result: T;
  /** 执行上下文 */
  context: AlgorithmContext;
  /** 算法元信息 */
  metadata: {
    algorithm: string;
    version: string;
    parameters: Record<string, unknown>;
  };
}

// 图算法工厂接口
export interface GraphAlgorithmFactory {
  /** 创建图实例 */
  createGraph(): Graph;

  /** 创建算法套件 */
  createAlgorithmSuite(graph: Graph): GraphAlgorithmSuite;

  /** 创建特定算法实例 */
  createCentralityAlgorithm(
    type: 'pagerank' | 'betweenness' | 'closeness' | 'degree',
  ): CentralityAlgorithm;
  createPathAlgorithm(type: 'dijkstra' | 'astar' | 'floyd' | 'bellman'): PathAlgorithm;
  createCommunityAlgorithm(type: 'louvain' | 'label_propagation'): CommunityDetectionAlgorithm;
  createSimilarityAlgorithm(type: 'jaccard' | 'cosine' | 'adamic'): SimilarityAlgorithm;
}
