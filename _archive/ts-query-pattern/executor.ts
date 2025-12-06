/**
 * Cypher 查询计划执行器
 *
 * 基于查询计划器生成的执行计划，高效执行查询
 * 支持多种执行算子：索引扫描、连接、过滤、投影等
 */

import type { PersistentStore } from '../../../core/storage/persistentStore.js';
import type {
  PlanNode,
  IndexScanPlan,
  JoinPlan,
  FilterPlan,
  ProjectPlan,
  LimitPlan,
} from './planner.js';
import type { PatternResult } from './match.js';
import type { Expression, BinaryExpression, PropertyAccess, Variable, Literal } from './ast.js';

// 执行上下文
interface ExecutionContext {
  bindings: Map<string, number>;
  parameters: Map<string, unknown>;
}

// 中间结果集
interface IntermediateResult {
  bindings: Map<string, number>;
  cost: number;
}

/**
 * 查询执行器
 */
export class CypherQueryExecutor {
  constructor(private readonly store: PersistentStore) {}

  /**
   * 执行查询计划
   */
  async execute(
    plan: PlanNode,
    parameters: Record<string, unknown> = {},
  ): Promise<PatternResult[]> {
    const context: ExecutionContext = {
      bindings: new Map(),
      parameters: new Map(Object.entries(parameters)),
    };

    const results = await this.executePlan(plan, context);
    return this.materializeResults(results, plan);
  }

  /**
   * 递归执行计划节点
   */
  private async executePlan(
    plan: PlanNode,
    context: ExecutionContext,
  ): Promise<IntermediateResult[]> {
    switch (plan.type) {
      case 'IndexScan':
        return await this.executeIndexScan(plan as IndexScanPlan, context);
      case 'Join':
        return await this.executeJoin(plan as JoinPlan, context);
      case 'Filter':
        return await this.executeFilter(plan as FilterPlan, context);
      case 'Project':
        return await this.executeProject(plan as ProjectPlan, context);
      case 'Limit':
        return await this.executeLimit(plan as LimitPlan, context);
      case 'CartesianProduct':
        return await this.executeCartesianProduct(plan, context);
      default:
        throw new Error(`不支持的计划节点类型: ${plan.type}`);
    }
  }

  /**
   * 执行索引扫描
   */
  private async executeIndexScan(
    plan: IndexScanPlan,
    context: ExecutionContext,
  ): Promise<IntermediateResult[]> {
    const results: IntermediateResult[] = [];
    const labelIndex = this.store.getLabelIndex();
    const propertyIndex = this.store.getPropertyIndex();

    let candidates: Set<number>;

    switch (plan.indexType) {
      case 'label':
        if (plan.labels && plan.labels.length > 0) {
          candidates = labelIndex.findNodesByLabels(plan.labels, { mode: 'AND' });
        } else {
          candidates = new Set();
        }
        break;

      case 'property':
        if (plan.propertyName && plan.propertyValue !== undefined) {
          if (plan.propertyOperator === '=') {
            candidates = propertyIndex.queryNodesByProperty(plan.propertyName, plan.propertyValue);
          } else {
            candidates = propertyIndex.queryNodesByRange(
              plan.propertyName,
              plan.propertyOperator === '>' || plan.propertyOperator === '>='
                ? plan.propertyValue
                : undefined,
              plan.propertyOperator === '<' || plan.propertyOperator === '<='
                ? plan.propertyValue
                : undefined,
              plan.propertyOperator === '>=' || plan.propertyOperator === '<=',
              plan.propertyOperator === '<=' || plan.propertyOperator === '>=',
            );
          }
        } else {
          candidates = new Set();
        }
        break;

      case 'full':
        // 全表扫描
        const allRecords = this.store.resolveRecords(this.store.query({}), {
          includeProperties: false,
        });
        candidates = new Set(allRecords.map((r) => r.subjectId));
        break;

      default:
        candidates = new Set();
    }

    // 将候选节点转换为中间结果
    for (const nodeId of candidates) {
      const bindings = new Map(context.bindings);
      bindings.set(plan.variable, nodeId);
      results.push({
        bindings,
        cost: 1,
      });
    }

    return results;
  }

  /**
   * 执行连接操作
   */
  private async executeJoin(
    plan: JoinPlan,
    context: ExecutionContext,
  ): Promise<IntermediateResult[]> {
    const leftResults = await this.executePlan(plan.left, context);
    const results: IntermediateResult[] = [];

    for (const leftResult of leftResults) {
      // 为每个左侧结果执行右侧计划
      const rightContext: ExecutionContext = {
        bindings: leftResult.bindings,
        parameters: context.parameters,
      };

      // 基于连接条件查找匹配的右侧节点
      const joinResults = await this.performJoin(leftResult, plan, rightContext);
      results.push(...joinResults);
    }

    return results;
  }

  /**
   * 执行实际的连接操作
   */
  private async performJoin(
    leftResult: IntermediateResult,
    plan: JoinPlan,
    context: ExecutionContext,
  ): Promise<IntermediateResult[]> {
    const results: IntermediateResult[] = [];
    const relationship = plan.relationship;

    // 获取左侧节点ID（这里简化处理，假设左侧只有一个变量）
    const leftNodeId = Array.from(leftResult.bindings.values())[0];

    if (leftNodeId === undefined) {
      return results;
    }

    // 基于关系查找邻居节点
    const predicateId = relationship.type
      ? this.store.getNodeIdByValue(relationship.type)
      : undefined;

    const criteria =
      relationship.direction === 'forward' ? { subjectId: leftNodeId } : { objectId: leftNodeId };

    const query =
      predicateId !== undefined
        ? this.store.query({ ...criteria, predicateId })
        : this.store.query(criteria);

    const records = this.store.resolveRecords(query, { includeProperties: false });

    for (const record of records) {
      const rightNodeId = relationship.direction === 'forward' ? record.objectId : record.subjectId;

      // 执行右侧计划
      const rightResults = await this.executePlan(plan.right, context);

      // 连接左右结果
      for (const rightResult of rightResults) {
        if (
          rightResult.bindings.has(plan.right.type) &&
          rightResult.bindings.get(plan.right.type) === rightNodeId
        ) {
          const combinedBindings = new Map([...leftResult.bindings, ...rightResult.bindings]);

          results.push({
            bindings: combinedBindings,
            cost: leftResult.cost + rightResult.cost + 1,
          });
        }
      }
    }

    return results;
  }

  /**
   * 执行过滤操作
   */
  private async executeFilter(
    plan: FilterPlan,
    context: ExecutionContext,
  ): Promise<IntermediateResult[]> {
    const childResults = await this.executePlan(plan.child, context);
    const results: IntermediateResult[] = [];

    for (const result of childResults) {
      const filterContext: ExecutionContext = {
        bindings: result.bindings,
        parameters: context.parameters,
      };

      if (await this.evaluateCondition(plan.condition, filterContext)) {
        results.push({
          bindings: result.bindings,
          cost: result.cost + 0.1, // 过滤成本
        });
      }
    }

    return results;
  }

  /**
   * 执行投影操作
   */
  private async executeProject(
    plan: ProjectPlan,
    context: ExecutionContext,
  ): Promise<IntermediateResult[]> {
    const childResults = await this.executePlan(plan.child, context);
    const results: IntermediateResult[] = [];

    for (const result of childResults) {
      // 投影只影响最终输出，中间结果保持不变
      results.push({
        bindings: result.bindings,
        cost: result.cost + 0.05, // 投影成本
      });
    }

    return results;
  }

  /**
   * 执行限制操作
   */
  private async executeLimit(
    plan: LimitPlan,
    context: ExecutionContext,
  ): Promise<IntermediateResult[]> {
    const childResults = await this.executePlan(plan.child, context);

    // 应用 LIMIT，只返回前 N 个结果
    return childResults.slice(0, plan.limit);
  }

  /**
   * 执行笛卡尔积
   */
  private async executeCartesianProduct(
    plan: PlanNode,
    context: ExecutionContext,
  ): Promise<IntermediateResult[]> {
    const left = (plan.properties as any).left as PlanNode;
    const right = (plan.properties as any).right as PlanNode;

    const leftResults = await this.executePlan(left, context);
    const rightResults = await this.executePlan(right, context);
    const results: IntermediateResult[] = [];

    for (const leftResult of leftResults) {
      for (const rightResult of rightResults) {
        const combinedBindings = new Map([...leftResult.bindings, ...rightResult.bindings]);

        results.push({
          bindings: combinedBindings,
          cost: leftResult.cost + rightResult.cost,
        });
      }
    }

    return results;
  }

  /**
   * 评估条件表达式
   */
  private async evaluateCondition(
    condition: Expression,
    context: ExecutionContext,
  ): Promise<boolean> {
    switch (condition.type) {
      case 'BinaryExpression':
        return await this.evaluateBinaryExpression(condition as BinaryExpression, context);
      case 'Literal':
        return Boolean((condition as Literal).value);
      default:
        // 简化：未支持的条件默认返回true
        return true;
    }
  }

  /**
   * 评估二元表达式
   */
  private async evaluateBinaryExpression(
    expr: BinaryExpression,
    context: ExecutionContext,
  ): Promise<boolean> {
    if (expr.operator === 'AND') {
      const left = await this.evaluateCondition(expr.left, context);
      const right = await this.evaluateCondition(expr.right, context);
      return left && right;
    }

    if (expr.operator === 'OR') {
      const left = await this.evaluateCondition(expr.left, context);
      const right = await this.evaluateCondition(expr.right, context);
      return left || right;
    }

    // 属性比较
    if (expr.left.type === 'PropertyAccess' && expr.right.type === 'Literal') {
      const propAccess = expr.left as PropertyAccess;
      const literal = expr.right as Literal;

      if (propAccess.object.type === 'Variable') {
        const variable = propAccess.object as Variable;
        const nodeId = context.bindings.get(variable.name);

        if (nodeId !== undefined) {
          const nodeProps = this.store.getNodeProperties(nodeId);
          if (nodeProps && propAccess.property in nodeProps) {
            const propValue = nodeProps[propAccess.property];
            return this.compareValues(propValue, expr.operator, literal.value);
          }
        }
      }
    }

    return false;
  }

  /**
   * 比较值
   */
  private compareValues(left: unknown, operator: string, right: unknown): boolean {
    switch (operator) {
      case '=':
        return left === right;
      case '<>':
      case '!=':
        return left !== right;
      case '<':
        return (left as any) < (right as any);
      case '<=':
        return (left as any) <= (right as any);
      case '>':
        return (left as any) > (right as any);
      case '>=':
        return (left as any) >= (right as any);
      default:
        return false;
    }
  }

  /**
   * 物化最终结果
   */
  private materializeResults(
    intermediateResults: IntermediateResult[],
    plan: PlanNode,
  ): PatternResult[] {
    const results: PatternResult[] = [];

    for (const intermediate of intermediateResults) {
      const result: PatternResult = {};

      // 转换节点ID为节点值
      for (const [variable, nodeId] of intermediate.bindings) {
        result[variable] = this.store.getNodeValueById(nodeId) ?? null;
      }

      results.push(result);
    }

    return results;
  }

  /**
   * 获取执行统计信息
   */
  getExecutionStats(results: IntermediateResult[]) {
    const totalCost = results.reduce((sum, r) => sum + r.cost, 0);
    const avgCost = results.length > 0 ? totalCost / results.length : 0;

    return {
      resultCount: results.length,
      totalCost,
      averageCost: avgCost,
    };
  }
}
