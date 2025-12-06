/**
 * Cypher 查询计划器与优化器
 *
 * 提供基于成本的查询优化，生成高效的执行计划
 * 主要优化策略：
 * - 智能起始点选择（基于索引统计）
 * - 连接顺序优化（成本模型）
 * - 谓词下推（过滤条件提前）
 * - 执行计划缓存
 */

import type { PersistentStore } from '../../../core/storage/persistentStore.js';
import type {
  CypherQuery,
  MatchClause,
  WhereClause,
  Pattern,
  NodePattern,
  RelationshipPattern,
  BinaryExpression,
  PropertyAccess,
  Variable,
  Literal,
  Expression,
} from './ast.js';

// 执行计划节点类型
export interface PlanNode {
  type: string;
  cost: number;
  cardinality: number;
  properties: Record<string, unknown>;
}

// 索引扫描计划节点
export interface IndexScanPlan extends PlanNode {
  type: 'IndexScan';
  indexType: 'label' | 'property' | 'full';
  labels?: string[];
  propertyName?: string;
  propertyValue?: unknown;
  propertyOperator?: '=' | '>' | '<' | '>=' | '<=';
  variable: string;
}

// 连接计划节点
export interface JoinPlan extends PlanNode {
  type: 'Join';
  joinType: 'nested' | 'hash' | 'merge';
  left: PlanNode;
  right: PlanNode;
  relationship: {
    direction: 'forward' | 'reverse';
    type?: string;
    variable?: string;
  };
}

// 过滤计划节点
export interface FilterPlan extends PlanNode {
  type: 'Filter';
  child: PlanNode;
  condition: Expression;
  selectivity: number;
}

// 投影计划节点
export interface ProjectPlan extends PlanNode {
  type: 'Project';
  child: PlanNode;
  columns: string[];
}

// 限制计划节点
export interface LimitPlan extends PlanNode {
  type: 'Limit';
  child: PlanNode;
  limit: number;
}

// 统计信息接口
export interface Statistics {
  totalNodes: number;
  totalRelationships: number;
  labelCounts: Map<string, number>;
  relationshipTypeCounts: Map<string, number>;
  propertyDistinctCounts: Map<string, number>;
}

// 查询计划器
export class CypherQueryPlanner {
  private stats: Statistics | null = null;
  private planCache = new Map<string, PlanNode>();

  constructor(private readonly store: PersistentStore) {}

  /**
   * 为 Cypher 查询生成优化的执行计划
   */
  async generatePlan(query: CypherQuery): Promise<PlanNode> {
    // 提取 MATCH 和 WHERE 子句
    const matchClauses = query.clauses.filter((c) => c.type === 'MatchClause') as MatchClause[];
    const whereClauses = query.clauses.filter((c) => c.type === 'WhereClause') as WhereClause[];
    const returnClause = query.clauses.find((c) => c.type === 'ReturnClause');

    if (matchClauses.length === 0) {
      throw new Error('查询必须包含至少一个 MATCH 子句');
    }

    // 生成查询签名用于缓存
    const signature = this.generateQuerySignature(query);
    const cached = this.planCache.get(signature);
    if (cached) {
      return cached;
    }

    // 获取统计信息
    await this.collectStatistics();

    // 为每个 MATCH 子句生成计划
    let plan: PlanNode | null = null;
    for (const matchClause of matchClauses) {
      const matchPlan = await this.planMatch(matchClause, whereClauses);
      plan = plan ? this.combinePatterns(plan, matchPlan) : matchPlan;
    }

    if (!plan) {
      throw new Error('无法生成执行计划');
    }

    // 应用投影和限制
    if (returnClause) {
      plan = this.addProjection(plan, returnClause as any);

      // 处理 LIMIT 子句
      const returnClauseTyped = returnClause as any;
      if (returnClauseTyped.limit && returnClauseTyped.limit > 0) {
        plan = this.addLimit(plan, returnClauseTyped.limit);
      }
    }

    // 缓存计划
    this.planCache.set(signature, plan);

    return plan;
  }

  /**
   * 为单个 MATCH 子句生成计划
   */
  private async planMatch(
    matchClause: MatchClause,
    whereClauses: WhereClause[],
  ): Promise<PlanNode> {
    const pattern = matchClause.pattern;
    const nodes = this.extractNodes(pattern);
    const relationships = this.extractRelationships(pattern);

    // 选择最优的起始节点
    const startNode = this.selectBestStartingNode(nodes, whereClauses);

    // 生成起始节点的索引扫描计划
    let plan: PlanNode = this.generateIndexScanPlan(startNode, whereClauses);

    // 按成本排序剩余的连接
    const remainingNodes = nodes.filter((n) => n !== startNode);
    const joinOrder = this.optimizeJoinOrder(remainingNodes, relationships);

    // 逐步构建连接计划
    for (const node of joinOrder) {
      const relationship = this.findRelationship(plan, node, relationships);
      if (relationship) {
        const rightScan = this.generateIndexScanPlan(node, whereClauses);
        plan = this.createJoinPlan(plan, rightScan, relationship);
      }
    }

    // 应用过滤条件
    for (const whereClause of whereClauses) {
      plan = this.applyFilter(plan, whereClause);
    }

    return plan;
  }

  /**
   * 选择最优的起始节点（基于选择性）
   */
  private selectBestStartingNode(nodes: NodePattern[], whereClauses: WhereClause[]): NodePattern {
    let bestNode = nodes[0];
    let lowestCardinality = Number.MAX_SAFE_INTEGER;

    for (const node of nodes) {
      const cardinality = this.estimateNodeCardinality(node, whereClauses);
      if (cardinality < lowestCardinality) {
        lowestCardinality = cardinality;
        bestNode = node;
      }
    }

    return bestNode;
  }

  /**
   * 估算节点的基数
   */
  private estimateNodeCardinality(node: NodePattern, whereClauses: WhereClause[]): number {
    if (!this.stats) return 1000; // 默认估算

    let cardinality = this.stats.totalNodes;

    // 基于标签过滤
    if (node.labels.length > 0) {
      let labelCardinality = this.stats.totalNodes;
      for (const label of node.labels) {
        const count = this.stats.labelCounts.get(label) || 0;
        labelCardinality = Math.min(labelCardinality, count);
      }
      cardinality = labelCardinality;
    }

    // 基于属性过滤
    const propertyFilters = this.extractPropertyFilters(node, whereClauses);
    for (const filter of propertyFilters) {
      const distinctCount = this.stats.propertyDistinctCounts.get(filter.property) || cardinality;
      const selectivity = filter.operator === '=' ? 1 / distinctCount : 0.3; // 范围查询估算30%选择性
      cardinality *= selectivity;
    }

    return Math.max(1, Math.floor(cardinality));
  }

  /**
   * 生成索引扫描计划
   */
  private generateIndexScanPlan(node: NodePattern, whereClauses: WhereClause[]): IndexScanPlan {
    const propertyFilters = this.extractPropertyFilters(node, whereClauses);

    // 优先使用属性索引
    if (propertyFilters.length > 0) {
      const bestFilter = propertyFilters[0]; // 简化：选择第一个过滤条件
      return {
        type: 'IndexScan',
        indexType: 'property',
        propertyName: bestFilter.property,
        propertyValue: bestFilter.value,
        propertyOperator: bestFilter.operator,
        variable: node.variable!,
        cost: 10,
        cardinality: this.estimateNodeCardinality(node, whereClauses),
        properties: {},
      };
    }

    // 使用标签索引
    if (node.labels.length > 0) {
      return {
        type: 'IndexScan',
        indexType: 'label',
        labels: node.labels,
        variable: node.variable!,
        cost: 50,
        cardinality: this.estimateNodeCardinality(node, whereClauses),
        properties: {},
      };
    }

    // 全表扫描
    return {
      type: 'IndexScan',
      indexType: 'full',
      variable: node.variable!,
      cost: 1000,
      cardinality: this.stats?.totalNodes || 1000,
      properties: {},
    };
  }

  /**
   * 创建连接计划
   */
  private createJoinPlan(
    left: PlanNode,
    right: PlanNode,
    relationship: RelationshipPattern,
  ): JoinPlan {
    const joinCost = left.cardinality * right.cardinality * 0.1; // 简化成本模型
    const joinCardinality = Math.floor(left.cardinality * right.cardinality * 0.01); // 简化基数估算

    return {
      type: 'Join',
      joinType: 'nested', // 简化：使用嵌套循环连接
      left,
      right,
      relationship: {
        direction: relationship.direction === 'LEFT_TO_RIGHT' ? 'forward' : 'reverse',
        type: relationship.types[0],
        variable: relationship.variable,
      },
      cost: joinCost,
      cardinality: joinCardinality,
      properties: {},
    };
  }

  /**
   * 应用过滤条件
   */
  private applyFilter(plan: PlanNode, whereClause: WhereClause): FilterPlan {
    return {
      type: 'Filter',
      child: plan,
      condition: whereClause.expression,
      selectivity: 0.1, // 简化：假设过滤条件选择性为10%
      cost: plan.cost + plan.cardinality * 0.1,
      cardinality: Math.floor(plan.cardinality * 0.1),
      properties: {},
    };
  }

  /**
   * 添加投影操作
   */
  private addProjection(plan: PlanNode, returnClause: any): ProjectPlan {
    const columns = returnClause.items.map((item: any) => item.alias || item.expression.name);

    return {
      type: 'Project',
      child: plan,
      columns,
      cost: plan.cost + plan.cardinality * 0.05,
      cardinality: plan.cardinality,
      properties: {},
    };
  }

  /**
   * 添加限制操作
   */
  private addLimit(plan: PlanNode, limit: number): LimitPlan {
    return {
      type: 'Limit',
      child: plan,
      limit,
      cost: plan.cost + 1, // LIMIT 的开销很小
      cardinality: Math.min(plan.cardinality, limit),
      properties: {},
    };
  }

  /**
   * 收集数据库统计信息
   */
  private async collectStatistics(): Promise<void> {
    if (this.stats) return; // 已收集

    const labelIndex = this.store.getLabelIndex();
    const propertyIndex = this.store.getPropertyIndex();

    // 统计总节点数（通过查询所有主语）
    const allRecords = this.store.resolveRecords(this.store.query({}), {
      includeProperties: false,
    });
    const uniqueSubjects = new Set(allRecords.map((r) => r.subjectId));
    const totalNodes = uniqueSubjects.size;

    const labelCounts = new Map<string, number>();
    // 简化：这里需要从 labelIndex 获取标签统计，当前版本可能没有直接接口
    // 实际实现中需要扩展 labelIndex 提供统计方法

    const relationshipTypeCounts = new Map<string, number>();
    // 统计关系类型分布
    const predicateCounts = new Map<number, number>();
    for (const record of allRecords) {
      const count = predicateCounts.get(record.predicateId) || 0;
      predicateCounts.set(record.predicateId, count + 1);
    }

    const propertyDistinctCounts = new Map<string, number>();
    // 简化：这里需要从 propertyIndex 获取属性分布统计

    this.stats = {
      totalNodes,
      totalRelationships: allRecords.length,
      labelCounts,
      relationshipTypeCounts,
      propertyDistinctCounts,
    };
  }

  // 工具方法
  private extractNodes(pattern: Pattern): NodePattern[] {
    return pattern.elements.filter((e) => e.type === 'NodePattern') as NodePattern[];
  }

  private extractRelationships(pattern: Pattern): RelationshipPattern[] {
    return pattern.elements.filter(
      (e) => e.type === 'RelationshipPattern',
    ) as RelationshipPattern[];
  }

  private extractPropertyFilters(node: NodePattern, whereClauses: WhereClause[]) {
    const filters: Array<{
      property: string;
      operator: '=' | '>' | '<' | '>=' | '<=';
      value: unknown;
    }> = [];

    for (const whereClause of whereClauses) {
      if (whereClause.expression.type === 'BinaryExpression') {
        const expr = whereClause.expression as BinaryExpression;
        if (
          expr.left.type === 'PropertyAccess' &&
          (expr.left as PropertyAccess).object.type === 'Variable' &&
          ((expr.left as PropertyAccess).object as Variable).name === node.variable &&
          expr.right.type === 'Literal' &&
          ['=', '>', '<', '>=', '<='].includes(expr.operator)
        ) {
          filters.push({
            property: (expr.left as PropertyAccess).property,
            operator: expr.operator as '=' | '>' | '<' | '>=' | '<=',
            value: (expr.right as Literal).value,
          });
        }
      }
    }

    return filters;
  }

  private optimizeJoinOrder(
    nodes: NodePattern[],
    relationships: RelationshipPattern[],
  ): NodePattern[] {
    // 简化：按节点在模式中的出现顺序
    return nodes;
  }

  private findRelationship(
    leftPlan: PlanNode,
    rightNode: NodePattern,
    relationships: RelationshipPattern[],
  ): RelationshipPattern | null {
    // 简化：返回第一个关系
    return relationships[0] || null;
  }

  private combinePatterns(left: PlanNode, right: PlanNode): PlanNode {
    // 简化：创建笛卡尔积连接
    return {
      type: 'CartesianProduct',
      cost: left.cost + right.cost + left.cardinality * right.cardinality,
      cardinality: left.cardinality * right.cardinality,
      properties: { left, right },
    };
  }

  private generateQuerySignature(query: CypherQuery): string {
    // 简化：基于查询文本生成签名
    return JSON.stringify(query);
  }

  /**
   * 清理计划缓存
   */
  clearCache(): void {
    this.planCache.clear();
    this.stats = null;
  }

  /**
   * 获取缓存统计
   */
  getCacheStats() {
    return {
      size: this.planCache.size,
      statistics: this.stats,
    };
  }
}
