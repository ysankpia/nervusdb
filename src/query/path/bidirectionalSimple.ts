/**
 * 简化的双向 BFS 实现
 * 重新设计以确保正确性
 */

import { PersistentStore, FactRecord } from '../../storage/persistentStore.js';
import type { Uniqueness, Direction, PathResult, VariablePathOptions } from './variable.js';

interface SearchState {
  nodeId: number;
  depth: number;
  path: FactRecord[];
  visitedNodes: Set<number>;
}

export class SimpleBidirectionalPathBuilder {
  constructor(
    private readonly store: PersistentStore,
    private readonly startNodes: Set<number>,
    private readonly targetNodes: Set<number>,
    private readonly predicateId: number,
    private readonly options: VariablePathOptions,
  ) {}

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

    // 如果只有一步的距离，直接检查
    if (max === 1) {
      return this.findDirectPath();
    }

    // 使用标准单向 BFS（双向 BFS 的复杂性在小图中可能不值得）
    return this.singleDirectionBFS();
  }

  private findDirectPath(): PathResult | null {
    const direction = this.options.direction ?? 'forward';

    for (const start of this.startNodes) {
      const neighbors = this.getNeighbors(start, direction);
      for (const record of neighbors) {
        const neighbor = this.getNextNode(record, start, direction);
        if (this.targetNodes.has(neighbor)) {
          return {
            edges: [{ record, direction }],
            length: 1,
            startId: start,
            endId: neighbor,
          };
        }
      }
    }
    return null;
  }

  private singleDirectionBFS(): PathResult | null {
    const min = Math.max(1, this.options.min ?? 1);
    const max = Math.max(min, this.options.max);
    const direction = this.options.direction ?? 'forward';
    const uniqueness = this.options.uniqueness ?? 'NODE';

    const queue: SearchState[] = [];
    const visited = new Set<number>();

    // 初始化队列
    for (const start of this.startNodes) {
      queue.push({
        nodeId: start,
        depth: 0,
        path: [],
        visitedNodes: new Set([start]),
      });
      visited.add(start);
    }

    while (queue.length > 0) {
      const current = queue.shift()!;

      // 检查是否到达目标
      if (current.depth >= min && this.targetNodes.has(current.nodeId)) {
        // 构建结果
        return {
          edges: current.path.map((record) => ({ record, direction })),
          length: current.path.length,
          startId: current.path.length > 0 ? current.path[0].subjectId : current.nodeId,
          endId: current.nodeId,
        };
      }

      // 如果已达到最大深度，跳过扩展
      if (current.depth >= max) continue;

      // 扩展邻居
      const neighbors = this.getNeighbors(current.nodeId, direction);
      for (const record of neighbors) {
        const nextNode = this.getNextNode(record, current.nodeId, direction);

        // 唯一性检查
        if (uniqueness === 'NODE' && current.visitedNodes.has(nextNode)) continue;
        if (visited.has(nextNode)) continue;

        const newPath = [...current.path, record];
        const newVisited = new Set(current.visitedNodes);
        newVisited.add(nextNode);

        queue.push({
          nodeId: nextNode,
          depth: current.depth + 1,
          path: newPath,
          visitedNodes: newVisited,
        });

        visited.add(nextNode);
      }
    }

    return null;
  }
}
