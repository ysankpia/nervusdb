/**
 * 图算法库主入口
 *
 * 导出所有图算法相关的类型定义、实现和工具函数
 */

// 导出类型定义
export * from './types.js';

// 导出图数据结构
export { MemoryGraph, GraphBuilder } from './graph.js';

// 导出中心性算法
export {
  PageRankCentrality,
  BetweennessCentrality,
  ClosenessCentrality,
  DegreeCentrality,
  EigenvectorCentrality,
  CentralityAlgorithmFactory,
} from './centrality.js';

// 导出社区发现算法
export {
  LouvainCommunityDetection,
  LabelPropagationCommunityDetection,
  ConnectedComponentsDetection,
  StronglyConnectedComponentsDetection,
  CommunityDetectionAlgorithmFactory,
} from './community.js';

// 导出路径算法
export {
  DijkstraPathAlgorithm,
  AStarPathAlgorithm,
  FloydWarshallPathAlgorithm,
  BellmanFordPathAlgorithm,
  PathAlgorithmFactory,
} from './pathfinding.js';

// 导出相似度算法
export {
  JaccardSimilarity,
  CosineSimilarity,
  AdamicAdarSimilarity,
  PreferentialAttachmentSimilarity,
  SimRankSimilarity,
  NodeAttributeSimilarity,
  SimilarityAlgorithmFactory,
} from './similarity.js';

// 导出统一算法套件
export {
  GraphAlgorithmSuiteImpl,
  GraphAlgorithmFactoryImpl,
  GraphAlgorithmUtils,
  GraphAlgorithms,
} from './suite.js';

// 便捷API（使用 ESM 静态导入，提供类型安全）
import { MemoryGraph, GraphBuilder } from './graph.js';
import type { Graph } from './types.js';
import { GraphAlgorithmSuiteImpl } from './suite.js';

/** 创建内存图实例 */
export const createGraph = (): MemoryGraph => new MemoryGraph();

/** 创建图构建器 */
export const createGraphBuilder = (): GraphBuilder => new GraphBuilder();

/** 创建算法套件（绑定图实例） */
export const createAlgorithmSuite = (graph: Graph): GraphAlgorithmSuiteImpl =>
  new GraphAlgorithmSuiteImpl(graph);
