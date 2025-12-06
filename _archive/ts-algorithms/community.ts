/**
 * 社区发现算法实现
 *
 * 提供图中社区结构检测算法，包括Louvain算法、标签传播算法等
 */

import {
  Graph,
  AlgorithmOptions,
  CommunityResult,
  CommunityLevel,
  CommunityDetectionAlgorithm,
  LouvainOptions,
} from './types.js';

/**
 * Louvain 社区发现算法实现
 * 基于模块度优化的多层次社区发现算法
 */
export class LouvainCommunityDetection implements CommunityDetectionAlgorithm {
  detectCommunities(graph: Graph, options: LouvainOptions = {}): CommunityResult {
    const { resolution = 1.0, maxIterations = 100, randomness = 0.01, tolerance = 1e-5 } = options;

    const nodes = graph.getNodes();
    if (nodes.length === 0) {
      return {
        communities: new Map(),
        hierarchy: [],
        modularity: 0,
        communityCount: 0,
      };
    }

    // 初始化社区分配（每个节点一个社区）
    let communities = new Map<string, number>();
    nodes.forEach((node, index) => {
      communities.set(node.id, index);
    });

    const hierarchy: CommunityLevel[] = [];
    let currentGraph = graph.clone();
    let level = 0;

    while (true) {
      // 第一阶段：局部优化
      const { newCommunities, improved } = this.louvainPhaseOne(
        currentGraph,
        communities,
        resolution,
        maxIterations,
        tolerance,
        randomness,
      );

      if (!improved) {
        break;
      }

      communities = newCommunities;
      const modularity = this.calculateModularity(currentGraph, communities);

      // 记录当前层次
      hierarchy.push({
        level,
        communities: new Map(communities),
        modularity,
      });

      // 第二阶段：图折叠
      const previousNodeCount = currentGraph.getNodes().length;
      currentGraph = this.buildCommunityGraph(currentGraph, communities);
      const currentNodeCount = currentGraph.getNodes().length;

      // 防御性编程：如果图没有被折叠（节点数未减少），则跳出循环避免死循环
      if (currentNodeCount === previousNodeCount) {
        // console.warn('[WARN] Louvain: Graph folding not implemented. Breaking loop to prevent infinite execution.');
        break;
      }

      // 更新社区映射为新图的节点
      const newNodes = currentGraph.getNodes();
      const tempCommunities = new Map<string, number>();
      newNodes.forEach((node, index) => {
        tempCommunities.set(node.id, index);
      });
      communities = tempCommunities;

      level++;
    }

    // 计算最终结果
    const finalModularity =
      hierarchy.length > 0
        ? hierarchy[hierarchy.length - 1].modularity
        : this.calculateModularity(graph, communities);

    const communityCount = new Set(communities.values()).size;

    return {
      communities,
      hierarchy,
      modularity: finalModularity,
      communityCount,
    };
  }

  calculateModularity(graph: Graph, communities: Map<string, number>): number {
    const edges = graph.getEdges();
    const totalWeight = edges.reduce((sum, edge) => sum + (edge.weight || 1), 0);

    if (totalWeight === 0) return 0;

    let modularity = 0;
    const communityWeights = new Map<number, number>();
    const nodeWeights = new Map<string, number>();

    // 计算节点权重
    graph.getNodes().forEach((node) => {
      const outEdges = graph.getOutEdges(node.id);
      const inEdges = graph.getInEdges(node.id);
      const weight =
        outEdges.reduce((sum, edge) => sum + (edge.weight || 1), 0) +
        inEdges.reduce((sum, edge) => sum + (edge.weight || 1), 0);
      nodeWeights.set(node.id, weight);
    });

    // 计算社区内部权重
    edges.forEach((edge) => {
      const sourceCommunity = communities.get(edge.source);
      const targetCommunity = communities.get(edge.target);
      const weight = edge.weight || 1;

      if (sourceCommunity === targetCommunity) {
        modularity += weight;
      }
    });

    // 计算社区总权重
    communities.forEach((community, nodeId) => {
      const nodeWeight = nodeWeights.get(nodeId) || 0;
      communityWeights.set(community, (communityWeights.get(community) || 0) + nodeWeight);
    });

    // 减去期望值
    communityWeights.forEach((weight) => {
      modularity -= (weight * weight) / (4 * totalWeight);
    });

    return modularity / (2 * totalWeight);
  }

  /**
   * Louvain算法第一阶段：局部模块度优化
   */
  private louvainPhaseOne(
    graph: Graph,
    initialCommunities: Map<string, number>,
    resolution: number,
    maxIterations: number,
    tolerance: number,
    randomness: number,
  ): { newCommunities: Map<string, number>; improved: boolean } {
    const nodes = graph.getNodes();
    const communities = new Map(initialCommunities);
    let improved = false;

    for (let iteration = 0; iteration < maxIterations; iteration++) {
      let hasChange = false;

      // 随机访问节点顺序
      const shuffledNodes = [...nodes].sort(() => Math.random() - 0.5);

      for (const node of shuffledNodes) {
        const nodeId = node.id;
        const currentCommunity = communities.get(nodeId)!;

        // 计算移动到邻居社区的模块度增益
        const neighborCommunities = new Set<number>();
        const neighbors = graph.getNeighbors(nodeId);

        neighbors.forEach((neighbor) => {
          neighborCommunities.add(communities.get(neighbor.id)!);
        });

        let bestCommunity = currentCommunity;
        let bestGain = 0;

        // 尝试移动到每个邻居社区
        for (const targetCommunity of neighborCommunities) {
          if (targetCommunity === currentCommunity) continue;

          const gain = this.calculateModularityGain(
            graph,
            nodeId,
            currentCommunity,
            targetCommunity,
            communities,
            resolution,
          );

          if (gain > bestGain + randomness * Math.random()) {
            bestGain = gain;
            bestCommunity = targetCommunity;
          }
        }

        // 移动节点到最优社区
        if (bestCommunity !== currentCommunity && bestGain > tolerance) {
          communities.set(nodeId, bestCommunity);
          hasChange = true;
          improved = true;
        }
      }

      if (!hasChange) {
        break;
      }
    }

    return { newCommunities: communities, improved };
  }

  /**
   * 计算将节点从一个社区移动到另一个社区的模块度增益
   */
  private calculateModularityGain(
    graph: Graph,
    nodeId: string,
    fromCommunity: number,
    toCommunity: number,
    communities: Map<string, number>,
    resolution: number,
  ): number {
    // 简化的模块度增益计算
    let gain = 0;
    const neighbors = graph.getNeighbors(nodeId);

    for (const neighbor of neighbors) {
      const neighborCommunity = communities.get(neighbor.id)!;
      const edge =
        graph.getOutEdges(nodeId).find((e) => e.target === neighbor.id) ||
        graph.getInEdges(nodeId).find((e) => e.source === neighbor.id);
      const weight = edge?.weight || 1;

      if (neighborCommunity === toCommunity) {
        gain += weight; // 增加内部连接
      }
      if (neighborCommunity === fromCommunity) {
        gain -= weight; // 减少原有内部连接
      }
    }

    return gain * resolution;
  }

  /**
   * 构建社区图（将同一社区的节点合并）
   */
  private buildCommunityGraph(graph: Graph, communities: Map<string, number>): Graph {
    // 参数目前未使用，保留接口以便后续实现真实合并逻辑
    void communities;
    // 为简化实现，这里返回原图的克隆
    // 在实际实现中，应该将同一社区的节点合并为一个超节点
    return graph.clone();
  }
}

/**
 * 标签传播算法实现
 */
export class LabelPropagationCommunityDetection implements CommunityDetectionAlgorithm {
  detectCommunities(graph: Graph, options: AlgorithmOptions = {}): CommunityResult {
    const { maxIterations = 100 } = options;

    const nodes = graph.getNodes();
    if (nodes.length === 0) {
      return {
        communities: new Map(),
        hierarchy: [],
        modularity: 0,
        communityCount: 0,
      };
    }

    // 初始化标签（每个节点一个唯一标签）
    const labels = new Map<string, number>();
    nodes.forEach((node, index) => {
      labels.set(node.id, index);
    });

    // 标签传播迭代
    for (let iteration = 0; iteration < maxIterations; iteration++) {
      let hasChange = false;

      // 随机访问节点顺序
      const shuffledNodes = [...nodes].sort(() => Math.random() - 0.5);

      for (const node of shuffledNodes) {
        const nodeId = node.id;
        const currentLabel = labels.get(nodeId)!;

        // 统计邻居标签频次
        const labelCounts = new Map<number, number>();
        const neighbors = graph.getNeighbors(nodeId);

        for (const neighbor of neighbors) {
          const neighborLabel = labels.get(neighbor.id)!;

          // 获取边权重
          const edge =
            graph.getOutEdges(nodeId).find((e) => e.target === neighbor.id) ||
            graph.getInEdges(nodeId).find((e) => e.source === neighbor.id);
          const weight = edge?.weight || 1;

          labelCounts.set(neighborLabel, (labelCounts.get(neighborLabel) || 0) + weight);
        }

        if (labelCounts.size === 0) continue;

        // 找到最频繁的标签
        let maxCount = 0;
        let bestLabels: number[] = [];

        labelCounts.forEach((count, label) => {
          if (count > maxCount) {
            maxCount = count;
            bestLabels = [label];
          } else if (count === maxCount) {
            bestLabels.push(label);
          }
        });

        // 随机选择一个最优标签
        const newLabel = bestLabels[Math.floor(Math.random() * bestLabels.length)];

        if (newLabel !== currentLabel) {
          labels.set(nodeId, newLabel);
          hasChange = true;
        }
      }

      if (!hasChange) {
        break;
      }
    }

    // 重新映射社区标签为连续整数
    const uniqueLabels = Array.from(new Set(labels.values())).sort((a, b) => a - b);
    const labelMapping = new Map<number, number>();
    uniqueLabels.forEach((label, index) => {
      labelMapping.set(label, index);
    });

    const communities = new Map<string, number>();
    labels.forEach((label, nodeId) => {
      communities.set(nodeId, labelMapping.get(label)!);
    });

    const modularity = this.calculateModularity(graph, communities);
    const communityCount = uniqueLabels.length;

    return {
      communities,
      hierarchy: [
        {
          level: 0,
          communities: new Map(communities),
          modularity,
        },
      ],
      modularity,
      communityCount,
    };
  }

  calculateModularity(graph: Graph, communities: Map<string, number>): number {
    const louvain = new LouvainCommunityDetection();
    return louvain.calculateModularity(graph, communities);
  }
}

/**
 * 连通分量检测算法
 */
export class ConnectedComponentsDetection implements CommunityDetectionAlgorithm {
  detectCommunities(graph: Graph, options: AlgorithmOptions = {}): CommunityResult {
    void options;
    const nodes = graph.getNodes();
    if (nodes.length === 0) {
      return {
        communities: new Map(),
        hierarchy: [],
        modularity: 0,
        communityCount: 0,
      };
    }

    const communities = new Map<string, number>();
    const visited = new Set<string>();
    let componentId = 0;

    // 深度优先搜索找连通分量
    for (const node of nodes) {
      if (!visited.has(node.id)) {
        this.dfsVisit(graph, node.id, visited, communities, componentId);
        componentId++;
      }
    }

    const modularity = this.calculateModularity(graph, communities);

    return {
      communities,
      hierarchy: [
        {
          level: 0,
          communities: new Map(communities),
          modularity,
        },
      ],
      modularity,
      communityCount: componentId,
    };
  }

  calculateModularity(graph: Graph, communities: Map<string, number>): number {
    const louvain = new LouvainCommunityDetection();
    return louvain.calculateModularity(graph, communities);
  }

  private dfsVisit(
    graph: Graph,
    nodeId: string,
    visited: Set<string>,
    communities: Map<string, number>,
    componentId: number,
  ): void {
    visited.add(nodeId);
    communities.set(nodeId, componentId);

    const neighbors = graph.getNeighbors(nodeId);
    for (const neighbor of neighbors) {
      if (!visited.has(neighbor.id)) {
        this.dfsVisit(graph, neighbor.id, visited, communities, componentId);
      }
    }
  }
}

/**
 * 强连通分量检测算法（Kosaraju算法）
 */
export class StronglyConnectedComponentsDetection implements CommunityDetectionAlgorithm {
  detectCommunities(graph: Graph, options: AlgorithmOptions = {}): CommunityResult {
    void options;
    const nodes = graph.getNodes();
    if (nodes.length === 0) {
      return {
        communities: new Map(),
        hierarchy: [],
        modularity: 0,
        communityCount: 0,
      };
    }

    const communities = new Map<string, number>();
    const visited = new Set<string>();
    const finishOrder: string[] = [];
    let componentId = 0;

    // 第一遍DFS，记录完成时间顺序
    for (const node of nodes) {
      if (!visited.has(node.id)) {
        this.dfsFinishTime(graph, node.id, visited, finishOrder);
      }
    }

    // 构建转置图
    const transposeGraph = this.buildTransposeGraph(graph);

    // 第二遍DFS，按完成时间倒序访问
    visited.clear();
    finishOrder.reverse();

    for (const nodeId of finishOrder) {
      if (!visited.has(nodeId)) {
        this.dfsAssignComponent(transposeGraph, nodeId, visited, communities, componentId);
        componentId++;
      }
    }

    const modularity = this.calculateModularity(graph, communities);

    return {
      communities,
      hierarchy: [
        {
          level: 0,
          communities: new Map(communities),
          modularity,
        },
      ],
      modularity,
      communityCount: componentId,
    };
  }

  calculateModularity(graph: Graph, communities: Map<string, number>): number {
    const louvain = new LouvainCommunityDetection();
    return louvain.calculateModularity(graph, communities);
  }

  private dfsFinishTime(
    graph: Graph,
    nodeId: string,
    visited: Set<string>,
    finishOrder: string[],
  ): void {
    visited.add(nodeId);

    const neighbors = graph.getNeighbors(nodeId);
    for (const neighbor of neighbors) {
      // 只考虑有向边的出邻居
      const hasDirectedEdge = graph
        .getOutEdges(nodeId)
        .some((edge) => edge.target === neighbor.id && edge.directed !== false);

      if (hasDirectedEdge && !visited.has(neighbor.id)) {
        this.dfsFinishTime(graph, neighbor.id, visited, finishOrder);
      }
    }

    finishOrder.push(nodeId);
  }

  private buildTransposeGraph(graph: Graph): Graph {
    const transposeGraph = graph.clone();
    transposeGraph.clear();

    // 添加节点
    graph.getNodes().forEach((node) => {
      transposeGraph.addNode(node);
    });

    // 添加反向边
    graph.getEdges().forEach((edge) => {
      if (edge.directed !== false) {
        transposeGraph.addEdge({
          ...edge,
          source: edge.target,
          target: edge.source,
        });
      } else {
        // 无向边保持不变
        transposeGraph.addEdge(edge);
      }
    });

    return transposeGraph;
  }

  private dfsAssignComponent(
    graph: Graph,
    nodeId: string,
    visited: Set<string>,
    communities: Map<string, number>,
    componentId: number,
  ): void {
    visited.add(nodeId);
    communities.set(nodeId, componentId);

    const neighbors = graph.getNeighbors(nodeId);
    for (const neighbor of neighbors) {
      if (!visited.has(neighbor.id)) {
        this.dfsAssignComponent(graph, neighbor.id, visited, communities, componentId);
      }
    }
  }
}

/**
 * 社区发现算法工厂
 */
export class CommunityDetectionAlgorithmFactory {
  /**
   * 创建Louvain算法实例
   */
  static createLouvain(): LouvainCommunityDetection {
    return new LouvainCommunityDetection();
  }

  /**
   * 创建标签传播算法实例
   */
  static createLabelPropagation(): LabelPropagationCommunityDetection {
    return new LabelPropagationCommunityDetection();
  }

  /**
   * 创建连通分量检测算法实例
   */
  static createConnectedComponents(): ConnectedComponentsDetection {
    return new ConnectedComponentsDetection();
  }

  /**
   * 创建强连通分量检测算法实例
   */
  static createStronglyConnectedComponents(): StronglyConnectedComponentsDetection {
    return new StronglyConnectedComponentsDetection();
  }

  /**
   * 根据类型创建算法实例
   */
  static create(
    type:
      | 'louvain'
      | 'label_propagation'
      | 'connected_components'
      | 'strongly_connected_components',
  ): CommunityDetectionAlgorithm {
    switch (type) {
      case 'louvain':
        return this.createLouvain();
      case 'label_propagation':
        return this.createLabelPropagation();
      case 'connected_components':
        return this.createConnectedComponents();
      case 'strongly_connected_components':
        return this.createStronglyConnectedComponents();
      default:
        // 理论上不可达，类型已穷尽
        throw new Error('未知的社区发现算法类型');
    }
  }
}
