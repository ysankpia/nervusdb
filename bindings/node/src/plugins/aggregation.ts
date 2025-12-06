import { NervusDBPlugin } from './base.js';
import type { NervusDB } from '../synapseDb.js';
import { PersistentStore } from '../core/storage/persistentStore.js';
import { AggregationPipeline } from '../extensions/query/aggregation.js';

/**
 * 聚合查询插件
 *
 * 提供数据聚合和统计功能：
 * - 数据分组
 * - 统计计算
 * - 聚合操作
 */
export class AggregationPlugin implements NervusDBPlugin {
  readonly name = 'aggregation';
  readonly version = '1.0.0';

  private db!: NervusDB;
  private store!: PersistentStore;

  initialize(db: NervusDB, store: PersistentStore): void {
    this.db = db;
    this.store = store;
  }

  /**
   * 创建聚合管道
   */
  aggregate(): AggregationPipeline {
    return new AggregationPipeline(this.store);
  }

  /**
   * 统计节点数量
   */
  countNodes(): number {
    return this.store.getDictionarySize();
  }

  /**
   * 统计边数量
   */
  countEdges(): number {
    return this.store.listFacts().length;
  }

  /**
   * 按谓语统计
   */
  countByPredicate(): Record<string, number> {
    const facts = this.store.listFacts();
    const counts: Record<string, number> = {};

    for (const fact of facts) {
      counts[fact.predicate] = (counts[fact.predicate] || 0) + 1;
    }

    return counts;
  }

  /**
   * 获取度分布
   */
  getDegreeDistribution(): { inDegree: Record<string, number>; outDegree: Record<string, number> } {
    const facts = this.store.listFacts();
    const inDegree: Record<string, number> = {};
    const outDegree: Record<string, number> = {};

    for (const fact of facts) {
      // 出度
      outDegree[fact.subject] = (outDegree[fact.subject] || 0) + 1;
      // 入度
      inDegree[fact.object] = (inDegree[fact.object] || 0) + 1;
    }

    return { inDegree, outDegree };
  }

  /**
   * 获取热门节点（按度排序）
   */
  getTopNodes(
    limit = 10,
    type: 'in' | 'out' | 'total' = 'total',
  ): Array<{ node: string; degree: number }> {
    const { inDegree, outDegree } = this.getDegreeDistribution();
    const nodes = new Set([...Object.keys(inDegree), ...Object.keys(outDegree)]);

    const result: Array<{ node: string; degree: number }> = [];

    for (const node of nodes) {
      let degree = 0;
      switch (type) {
        case 'in':
          degree = inDegree[node] || 0;
          break;
        case 'out':
          degree = outDegree[node] || 0;
          break;
        case 'total':
          degree = (inDegree[node] || 0) + (outDegree[node] || 0);
          break;
      }
      result.push({ node, degree });
    }

    return result.sort((a, b) => b.degree - a.degree).slice(0, limit);
  }

  /**
   * 获取连通分量数量
   */
  countConnectedComponents(): number {
    const facts = this.store.listFacts();
    if (facts.length === 0) return 0;

    const graph = new Map<string, Set<string>>();
    const nodes = new Set<string>();

    // 构建邻接表
    for (const fact of facts) {
      nodes.add(fact.subject);
      nodes.add(fact.object);

      if (!graph.has(fact.subject)) {
        graph.set(fact.subject, new Set());
      }
      if (!graph.has(fact.object)) {
        graph.set(fact.object, new Set());
      }

      graph.get(fact.subject)!.add(fact.object);
      graph.get(fact.object)!.add(fact.subject);
    }

    // BFS查找连通分量
    const visited = new Set<string>();
    let components = 0;

    for (const node of nodes) {
      if (!visited.has(node)) {
        components++;
        const queue = [node];
        visited.add(node);

        while (queue.length > 0) {
          const current = queue.shift()!;
          const neighbors = graph.get(current) || new Set();

          for (const neighbor of neighbors) {
            if (!visited.has(neighbor)) {
              visited.add(neighbor);
              queue.push(neighbor);
            }
          }
        }
      }
    }

    return components;
  }

  /**
   * 获取数据库统计摘要
   */
  getStatsSummary(): {
    nodes: number;
    edges: number;
    predicates: number;
    avgDegree: number;
    connectedComponents: number;
    topPredicates: Array<{ predicate: string; count: number }>;
  } {
    const nodeCount = this.countNodes();
    const edgeCount = this.countEdges();
    const predicateCounts = this.countByPredicate();
    const predicateCount = Object.keys(predicateCounts).length;
    const avgDegree = nodeCount > 0 ? (edgeCount * 2) / nodeCount : 0;
    const connectedComponents = this.countConnectedComponents();

    const topPredicates = Object.entries(predicateCounts)
      .map(([predicate, count]) => ({ predicate, count }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 5);

    return {
      nodes: nodeCount,
      edges: edgeCount,
      predicates: predicateCount,
      avgDegree: Math.round(avgDegree * 100) / 100,
      connectedComponents,
      topPredicates,
    };
  }
}
