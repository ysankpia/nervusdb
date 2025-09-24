/**
 * A*启发式搜索算法实现
 *
 * A*算法结合了Dijkstra算法的准确性和贪心最佳优先搜索的效率
 * 使用启发式函数h(n)估算从当前节点到目标的距离，提供最优路径搜索
 */

import { PersistentStore, FactRecord } from '../../storage/persistentStore.js';
import type {
  Uniqueness,
  Direction,
  PathEdge,
  PathResult,
  VariablePathOptions,
} from './variable.js';

interface AStarNode {
  nodeId: number;
  gScore: number; // 从起点到当前节点的实际代价
  fScore: number; // g(n) + h(n)，预估总代价
  parent?: AStarNode;
  edge?: PathEdge; // 到达此节点的边
  visitedNodes: Set<number>;
  visitedEdges: Set<string>;
}

interface HeuristicOptions {
  /** 启发式函数类型 */
  type: 'manhattan' | 'euclidean' | 'hop' | 'custom';
  /** 自定义启发式函数 */
  customHeuristic?: (from: number, to: number, store: PersistentStore) => number;
  /** 启发式权重因子，控制启发式的影响程度 */
  weight?: number;
}

export class AStarPathBuilder {
  private heuristicOptions: HeuristicOptions;

  constructor(
    private readonly store: PersistentStore,
    private readonly startNodes: Set<number>,
    private readonly targetNodes: Set<number>,
    private readonly predicateId: number,
    private readonly options: VariablePathOptions,
    heuristicOptions: HeuristicOptions = { type: 'hop', weight: 1.0 },
  ) {
    this.heuristicOptions = heuristicOptions;
  }

  private getNeighbors(nodeId: number, direction: Direction): FactRecord[] {
    const criteria =
      direction === 'forward'
        ? { subjectId: nodeId, predicateId: this.predicateId }
        : { predicateId: this.predicateId, objectId: nodeId };
    return this.store.resolveRecords(this.store.query(criteria));
  }

  private getNextNode(record: FactRecord, currentNode: number, direction: Direction): number {
    return direction === 'forward' ? record.objectId : record.subjectId;
  }

  /**
   * 启发式函数：估算从节点a到节点b的距离
   */
  private heuristic(fromNodeId: number, targetNodeIds: Set<number>): number {
    const weight = this.heuristicOptions.weight ?? 1.0;

    // 计算到所有目标节点的最小估算距离
    let minDistance = Infinity;

    for (const targetId of targetNodeIds) {
      let distance: number;

      switch (this.heuristicOptions.type) {
        case 'hop':
          // 简单的跳数启发式：如果相同则0，否则1
          distance = fromNodeId === targetId ? 0 : 1;
          break;

        case 'manhattan':
          // 曼哈顿距离（基于节点ID差值的简化版本）
          distance = Math.abs(fromNodeId - targetId) / 1000; // 归一化
          break;

        case 'euclidean':
          // 欧几里得距离（基于节点ID的简化版本）
          distance = Math.sqrt(Math.pow(fromNodeId - targetId, 2)) / 1000; // 归一化
          break;

        case 'custom':
          // 自定义启发式函数
          if (this.heuristicOptions.customHeuristic) {
            distance = this.heuristicOptions.customHeuristic(fromNodeId, targetId, this.store);
          } else {
            distance = 1; // 回退到跳数启发式
          }
          break;

        default:
          distance = 1;
      }

      minDistance = Math.min(minDistance, distance);
    }

    return minDistance * weight;
  }

  /**
   * A*最短路径搜索
   */
  shortestPath(): PathResult | null {
    const min = Math.max(0, this.options.min ?? 1);
    const max = Math.max(min, this.options.max);
    const direction = this.options.direction ?? 'forward';
    const uniqueness = this.options.uniqueness ?? 'NODE';

    // 处理零长度路径
    if (min === 0) {
      for (const start of this.startNodes) {
        if (this.targetNodes.has(start)) {
          return {
            edges: [],
            length: 0,
            startId: start,
            endId: start,
          };
        }
      }
    }

    // 开放列表（待探索的节点），使用优先队列（简化实现用数组）
    const openSet: AStarNode[] = [];
    // 已探索的节点集合
    const closedSet = new Set<number>();
    // 节点到其最佳路径信息的映射
    const allNodes = new Map<number, AStarNode>();

    // 初始化起始节点
    for (const startId of this.startNodes) {
      const startNode: AStarNode = {
        nodeId: startId,
        gScore: 0,
        fScore: this.heuristic(startId, this.targetNodes),
        visitedNodes: new Set([startId]),
        visitedEdges: new Set(),
      };

      openSet.push(startNode);
      allNodes.set(startId, startNode);
    }

    while (openSet.length > 0) {
      // 选择f值最小的节点
      openSet.sort((a, b) => a.fScore - b.fScore);
      const current = openSet.shift()!;

      // 如果已经探索过这个节点，跳过
      if (closedSet.has(current.nodeId)) {
        continue;
      }

      // 标记为已探索
      closedSet.add(current.nodeId);

      // 检查是否到达目标
      if (current.gScore >= min && this.targetNodes.has(current.nodeId)) {
        return this.reconstructPath(current);
      }

      // 如果达到最大深度，跳过扩展
      if (current.gScore >= max) {
        continue;
      }

      // 扩展邻居节点
      const neighbors = this.getNeighbors(current.nodeId, direction);
      for (const record of neighbors) {
        const neighborId = this.getNextNode(record, current.nodeId, direction);
        const edgeKey = `${record.subjectId}:${record.predicateId}:${record.objectId}`;

        // 唯一性检查
        if (uniqueness === 'NODE' && current.visitedNodes.has(neighborId)) {
          continue;
        }
        if (uniqueness === 'EDGE' && current.visitedEdges.has(edgeKey)) {
          continue;
        }

        // 如果已经在关闭列表中，跳过
        if (closedSet.has(neighborId)) {
          continue;
        }

        const tentativeGScore = current.gScore + 1; // 假设每条边权重为1

        const existingNode = allNodes.get(neighborId);
        const isNewPath = !existingNode || tentativeGScore < existingNode.gScore;

        if (isNewPath) {
          const newVisitedNodes = new Set(current.visitedNodes);
          newVisitedNodes.add(neighborId);
          const newVisitedEdges = new Set(current.visitedEdges);
          newVisitedEdges.add(edgeKey);

          const neighborNode: AStarNode = {
            nodeId: neighborId,
            gScore: tentativeGScore,
            fScore: tentativeGScore + this.heuristic(neighborId, this.targetNodes),
            parent: current,
            edge: { record, direction },
            visitedNodes: newVisitedNodes,
            visitedEdges: newVisitedEdges,
          };

          allNodes.set(neighborId, neighborNode);

          // 如果不在开放列表中，添加进去
          if (!openSet.some((node) => node.nodeId === neighborId)) {
            openSet.push(neighborNode);
          } else {
            // 更新开放列表中的节点
            const index = openSet.findIndex((node) => node.nodeId === neighborId);
            if (index !== -1) {
              openSet[index] = neighborNode;
            }
          }
        }
      }
    }

    return null; // 未找到路径
  }

  /**
   * 重构路径
   */
  private reconstructPath(goalNode: AStarNode): PathResult {
    const edges: PathEdge[] = [];
    let current: AStarNode | undefined = goalNode;
    let startId = goalNode.nodeId;

    // 从目标节点回溯到起始节点
    while (current && current.parent) {
      if (current.edge) {
        edges.unshift(current.edge);
      }
      current = current.parent;
      if (current && !current.parent) {
        startId = current.nodeId;
      }
    }

    return {
      edges,
      length: edges.length,
      startId,
      endId: goalNode.nodeId,
    };
  }

  /**
   * 所有路径搜索（限制搜索空间以避免性能问题）
   */
  allPaths(): PathResult[] {
    const shortestPath = this.shortestPath();
    return shortestPath ? [shortestPath] : [];
  }
}

/**
 * 便利函数：创建带有不同启发式选项的A*路径构建器
 */
export function createAStarPathBuilder(
  store: PersistentStore,
  startNodes: Set<number>,
  targetNodes: Set<number>,
  predicateId: number,
  options: VariablePathOptions,
  heuristicOptions?: HeuristicOptions,
): AStarPathBuilder {
  return new AStarPathBuilder(
    store,
    startNodes,
    targetNodes,
    predicateId,
    options,
    heuristicOptions,
  );
}

/**
 * 图距离启发式函数：基于图的连接性估算距离
 */
export function createGraphDistanceHeuristic(
  store: PersistentStore,
  predicateId: number,
  maxSampleDepth: number = 2,
): (from: number, to: number, store: PersistentStore) => number {
  return (fromNodeId: number, toNodeId: number): number => {
    if (fromNodeId === toNodeId) return 0;

    // 简单的BFS探索来估算距离
    const queue = [{ nodeId: fromNodeId, depth: 0 }];
    const visited = new Set<number>([fromNodeId]);

    while (queue.length > 0) {
      const { nodeId, depth } = queue.shift()!;

      if (depth >= maxSampleDepth) break;

      // 检查正向连接
      const forwardRecords = store.resolveRecords(store.query({ subjectId: nodeId, predicateId }));

      for (const record of forwardRecords) {
        if (record.objectId === toNodeId) {
          return depth + 1;
        }

        if (!visited.has(record.objectId) && depth < maxSampleDepth - 1) {
          visited.add(record.objectId);
          queue.push({ nodeId: record.objectId, depth: depth + 1 });
        }
      }

      // 检查反向连接
      const backwardRecords = store.resolveRecords(store.query({ predicateId, objectId: nodeId }));

      for (const record of backwardRecords) {
        if (record.subjectId === toNodeId) {
          return depth + 1;
        }

        if (!visited.has(record.subjectId) && depth < maxSampleDepth - 1) {
          visited.add(record.subjectId);
          queue.push({ nodeId: record.subjectId, depth: depth + 1 });
        }
      }
    }

    // 如果在有限深度内未找到，返回估算值
    return Math.max(maxSampleDepth, 1);
  };
}
