/**
 * Cypher 模式 AST 编译器
 *
 * 将解析得到的 AST 转换为现有的 PatternBuilder 调用
 * 确保文本语法与编程式 API 的完全兼容性
 */

import type {
  CypherQuery,
  MatchClause,
  SetClause,
  DeleteClause,
  MergeClause,
  RemoveClause,
  UnwindClause,
  UnionClause,
  Pattern,
  NodePattern,
  RelationshipPattern,
  Expression,
  Literal,
  Variable,
  PropertyAccess,
  BinaryExpression,
  PropertyMap,
  Direction,
  SubqueryExpression,
  SubqueryPattern,
  WhereClause,
  ListExpression,
} from './ast.js';

import { PatternBuilder, type PatternResult } from './match.js';
import type { PersistentStore } from '../../../core/storage/persistentStore.js';
import { CypherQueryPlanner } from './planner.js';
import { CypherQueryExecutor } from './executor.js';

// 编译错误
export class CompileError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'CompileError';
  }
}

// 编译结果接口
export interface CompileResult {
  execute(): Promise<PatternResult[]>;
  builder: PatternBuilder;
  isOptimized: boolean;
  executionPlan?: any;
}

// 变量绑定上下文
interface CompileContext {
  variables: Map<string, unknown>;
  parameters: Map<string, unknown>;
}

// 编译选项
export interface CompilerOptions {
  enableOptimization?: boolean;
  optimizationLevel?: 'basic' | 'aggressive';
}

export class CypherCompiler {
  private readonly planner: CypherQueryPlanner;
  private readonly executor: CypherQueryExecutor;

  constructor(private readonly store: PersistentStore) {
    this.planner = new CypherQueryPlanner(store);
    this.executor = new CypherQueryExecutor(store);
  }

  /**
   * 编译 Cypher 查询为可执行的 PatternBuilder
   */
  compile(
    query: CypherQuery,
    parameters: Record<string, unknown> = {},
    options: CompilerOptions = {},
  ): CompileResult {
    // 如果启用优化，使用查询计划器
    if (options.enableOptimization) {
      return this.compileOptimized(query, parameters, options);
    }

    // 使用传统的PatternBuilder编译
    return this.compileLegacy(query, parameters);
  }

  /**
   * 优化编译路径
   */
  private compileOptimized(
    query: CypherQuery,
    parameters: Record<string, unknown>,
    options: CompilerOptions,
  ): CompileResult {
    // 创建懒加载的执行函数
    const optimizedExecute = async (): Promise<PatternResult[]> => {
      try {
        // 延迟生成优化的执行计划（在执行时）
        const executionPlan = await this.planner.generatePlan(query);
        return await this.executor.execute(executionPlan, parameters);
      } catch (error) {
        // 优化失败时回退到传统编译和执行
        console.warn(
          `查询优化失败，回退到传统编译: ${error instanceof Error ? error.message : '未知错误'}`,
        );
        const fallbackResult = this.compileLegacy(query, parameters);
        return await fallbackResult.execute();
      }
    };

    return {
      execute: optimizedExecute,
      builder: new PatternBuilder(this.store), // 提供兼容性
      isOptimized: true,
    };
  }

  /**
   * 传统的PatternBuilder编译路径（保持向后兼容）
   */
  private compileLegacy(
    query: CypherQuery,
    parameters: Record<string, unknown> = {},
  ): CompileResult {
    const context: CompileContext = {
      variables: new Map(),
      parameters: new Map(Object.entries(parameters)),
    };

    const builder = new PatternBuilder(this.store);

    // 处理所有子句
    for (const clause of query.clauses) {
      switch (clause.type) {
        case 'MatchClause':
          this.compileMatchClause(clause, builder, context);
          break;
        case 'CreateClause':
          throw new CompileError('CREATE 子句尚未实现');
        case 'ReturnClause':
          this.compileReturnClause(clause, builder, context);
          break;
        case 'WhereClause':
          this.compileWhereClause(clause, builder, context);
          break;
        case 'SetClause':
          this.compileSetClause(clause, builder, context);
          break;
        case 'DeleteClause':
          this.compileDeleteClause(clause, builder, context);
          break;
        case 'MergeClause':
          this.compileMergeClause(clause, builder, context);
          break;
        case 'RemoveClause':
          this.compileRemoveClause(clause, builder, context);
          break;
        case 'UnwindClause':
          this.compileUnwindClause(clause, builder, context);
          break;
        case 'UnionClause':
          this.compileUnionClause(clause, builder, context);
          break;
        default:
          throw new CompileError(`不支持的子句类型: ${(clause as any).type}`);
      }
    }

    return {
      execute: () => builder.execute(),
      builder,
      isOptimized: false,
    };
  }

  /**
   * 编译 MATCH 子句
   */
  private compileMatchClause(
    clause: MatchClause,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    if (clause.optional) {
      throw new CompileError('OPTIONAL MATCH 尚未实现');
    }

    this.compilePattern(clause.pattern, builder, context);
  }

  /**
   * 编译模式
   */
  private compilePattern(pattern: Pattern, builder: PatternBuilder, context: CompileContext): void {
    const elements = pattern.elements;

    if (elements.length === 0) {
      throw new CompileError('空模式无效');
    }

    // 第一个元素必须是节点
    if (elements[0].type !== 'NodePattern') {
      throw new CompileError('模式必须以节点开始');
    }

    let currentElement = 0;

    // 编译第一个节点
    const firstNode = elements[currentElement] as NodePattern;
    this.compileNodePattern(firstNode, builder, context);
    currentElement++;

    // 编译关系-节点链
    while (currentElement < elements.length) {
      if (currentElement + 1 >= elements.length) {
        throw new CompileError('关系后必须跟节点');
      }

      const relationship = elements[currentElement] as RelationshipPattern;
      const nextNode = elements[currentElement + 1] as NodePattern;

      if (relationship.type !== 'RelationshipPattern') {
        throw new CompileError(`期望关系模式，得到 ${relationship.type}`);
      }

      if (nextNode.type !== 'NodePattern') {
        throw new CompileError(`期望节点模式，得到 ${nextNode.type}`);
      }

      this.compileRelationshipPattern(relationship, builder, context);
      this.compileNodePattern(nextNode, builder, context);

      currentElement += 2;
    }
  }

  /**
   * 编译节点模式
   */
  private compileNodePattern(
    node: NodePattern,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // 处理属性
    let properties: Record<string, unknown> | undefined;
    if (node.properties) {
      properties = this.compilePropertyMap(node.properties, context);
    }

    // 调用 PatternBuilder.node()
    builder.node(node.variable, node.labels, properties);

    // 记录变量绑定
    if (node.variable) {
      context.variables.set(node.variable, null); // 占位，实际值在执行时确定
    }
  }

  /**
   * 编译关系模式
   */
  private compileRelationshipPattern(
    relationship: RelationshipPattern,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // 转换方向
    let direction: '->' | '<-' | '-';
    switch (relationship.direction) {
      case 'LEFT_TO_RIGHT':
        direction = '->';
        break;
      case 'RIGHT_TO_LEFT':
        direction = '<-';
        break;
      case 'UNDIRECTED':
        direction = '-';
        break;
      default:
        throw new CompileError(`无效的关系方向: ${relationship.direction}`);
    }

    // 获取关系类型（只取第一个，PatternBuilder 目前只支持单一类型）
    const relType = relationship.types.length > 0 ? relationship.types[0] : undefined;

    // 调用 PatternBuilder.edge()
    builder.edge(direction, relType, relationship.variable);

    // 处理变长关系
    if (relationship.variableLength) {
      const varLength = relationship.variableLength;
      const min = varLength.min ?? 1;
      const max = varLength.max ?? 5;
      const uniqueness = varLength.uniqueness ?? 'NODE';

      builder.variable(min, max, uniqueness);
    }

    // 记录变量绑定
    if (relationship.variable) {
      context.variables.set(relationship.variable, null); // 占位
    }
  }

  /**
   * 编译 RETURN 子句
   */
  private compileReturnClause(clause: any, builder: PatternBuilder, context: CompileContext): void {
    const returnItems: string[] = [];

    for (const item of clause.items) {
      if (item.expression.type === 'Variable') {
        const varName = item.alias || item.expression.name;
        returnItems.push(varName);
      } else {
        throw new CompileError('RETURN 子句目前只支持简单变量');
      }
    }

    if (returnItems.length > 0) {
      builder.return(returnItems);
    }
  }

  /**
   * 编译 WHERE 子句
   */
  private compileWhereClause(clause: any, builder: PatternBuilder, context: CompileContext): void {
    // 简化实现：只支持属性比较
    this.compileWhereExpression(clause.expression, builder, context);
  }

  /**
   * 编译 WHERE 表达式
   */
  private compileWhereExpression(
    expression: Expression,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    switch (expression.type) {
      case 'BinaryExpression':
        this.compileBinaryExpression(expression, builder, context);
        break;
      case 'SubqueryExpression':
        this.compileSubqueryExpression(expression, builder, context);
        break;
      default:
        throw new CompileError(`WHERE 子句中不支持的表达式类型: ${expression.type}`);
    }
  }

  /**
   * 编译二元表达式
   */
  private compileBinaryExpression(
    expr: BinaryExpression,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // 处理 IN/NOT IN 操作符
    if (expr.operator === 'IN' || expr.operator === 'NOT IN') {
      this.compileInExpression(expr, builder, context);
      return;
    }

    // 简化实现：只支持 variable.property op literal
    if (
      expr.left.type === 'PropertyAccess' &&
      expr.left.object.type === 'Variable' &&
      expr.right.type === 'Literal'
    ) {
      const variable = (expr.left.object as Variable).name;
      const property = expr.left.property;
      const value = (expr.right as Literal).value;

      // 转换操作符
      let op: '=' | '>' | '<' | '>=' | '<=';
      switch (expr.operator) {
        case '=':
          op = '=';
          break;
        case '>':
          op = '>';
          break;
        case '<':
          op = '<';
          break;
        case '>=':
          op = '>=';
          break;
        case '<=':
          op = '<=';
          break;
        default:
          throw new CompileError(`不支持的比较操作符: ${expr.operator}`);
      }

      builder.whereNodeProperty(variable, property, op, value);
    } else {
      throw new CompileError('WHERE 子句目前只支持简单的属性比较');
    }
  }

  /**
   * 编译 IN/NOT IN 表达式
   */
  private compileInExpression(
    expr: BinaryExpression,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // 暂时简化实现：支持基础的属性值 IN 字面量列表
    if (
      expr.left.type === 'PropertyAccess' &&
      expr.left.object.type === 'Variable' &&
      expr.right.type === 'ListExpression'
    ) {
      const variable = (expr.left.object as Variable).name;
      const property = expr.left.property;
      const list = expr.right as ListExpression;

      // 将 IN/NOT IN 转换为多个 = 条件的 OR/AND 组合
      const values = list.elements.map((elem: Expression) =>
        this.evaluateExpression(elem, context),
      );
      const isNotIn = expr.operator === 'NOT IN';

      // 创建多个属性过滤条件
      if (values.length === 0) {
        // 空列表：IN 返回 false，NOT IN 返回 true
        // 通过添加一个永远不匹配的条件来实现
        if (isNotIn) {
          // NOT IN [] 应该匹配所有记录，不添加任何过滤条件
          return;
        } else {
          // IN [] 应该不匹配任何记录
          builder.whereNodeProperty(variable, property, '=', Symbol('never-match'));
          return;
        }
      }

      if (isNotIn) {
        // NOT IN: 所有值都不等于 (实现为多个不等式条件)
        // 注意：当前 PatternBuilder 不直接支持 != 操作符
        // 作为简化实现，我们使用异步过滤器来处理
        const inFilter = async (currentBindings: Map<string, number>): Promise<boolean> => {
          const nodeId = currentBindings.get(variable);
          if (nodeId === undefined) return false;

          const nodeProps = this.store.getNodeProperties(nodeId);
          if (!nodeProps || !(property in nodeProps)) return true; // 属性不存在时认为不在列表中

          const propValue = nodeProps[property];
          return !values.includes(propValue);
        };

        this.addAsyncFilter(builder, inFilter);
      } else {
        // IN: 等于任一值 (实现为多个等值条件的 OR)
        // 由于 PatternBuilder 不直接支持 OR 条件，我们也使用异步过滤器
        const inFilter = async (currentBindings: Map<string, number>): Promise<boolean> => {
          const nodeId = currentBindings.get(variable);
          if (nodeId === undefined) return false;

          const nodeProps = this.store.getNodeProperties(nodeId);
          if (!nodeProps || !(property in nodeProps)) return false;

          const propValue = nodeProps[property];
          return values.includes(propValue);
        };

        this.addAsyncFilter(builder, inFilter);
      }
    } else {
      throw new CompileError('IN/NOT IN 子句目前只支持属性与字面量列表的比较');
    }
  }

  /**
   * 编译子查询表达式（EXISTS/NOT EXISTS）
   */
  private compileSubqueryExpression(
    expr: SubqueryExpression,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    const subqueryOperator = expr.operator as 'EXISTS' | 'NOT EXISTS';
    const subqueryPattern = expr.query;

    // 为子查询创建独立的 PatternBuilder
    const subqueryBuilder = new PatternBuilder(this.store);
    const subContext: CompileContext = {
      variables: new Map(context.variables),
      parameters: new Map(context.parameters),
    };

    // 编译子查询模式
    this.compilePattern(subqueryPattern.pattern, subqueryBuilder, subContext);

    // 编译子查询的 WHERE 子句（如果存在）
    if (subqueryPattern.where) {
      this.compileWhereClause(subqueryPattern.where, subqueryBuilder, subContext);
    }

    // 创建子查询过滤器函数
    const subqueryFilter = async (currentBindings: Map<string, number>): Promise<boolean> => {
      try {
        // 为子查询上下文注入当前绑定的变量值
        for (const [varName, nodeId] of currentBindings.entries()) {
          subContext.variables.set(varName, nodeId);
        }

        // 执行子查询
        const subResults = await subqueryBuilder.execute();

        // EXISTS: 有结果则返回 true
        // NOT EXISTS: 无结果则返回 true
        const hasResults = subResults.length > 0;
        return subqueryOperator === 'EXISTS' ? hasResults : !hasResults;
      } catch (error) {
        // 子查询执行失败时，根据操作符决定返回值
        return subqueryOperator === 'NOT EXISTS';
      }
    };

    // 将子查询过滤器添加到主查询的执行管道中
    // 由于当前 PatternBuilder 不直接支持异步过滤器，我们需要扩展其功能
    this.addAsyncFilter(builder, subqueryFilter);
  }

  /**
   * 为 PatternBuilder 添加异步过滤器支持
   * 这是一个临时方案，理想情况下应该在 PatternBuilder 中原生支持
   */
  private addAsyncFilter(
    builder: PatternBuilder,
    filter: (bindings: Map<string, number>) => Promise<boolean>,
  ): void {
    // 通过修改 builder 的 execute 方法来支持异步过滤
    const originalExecute = builder.execute.bind(builder);

    builder.execute = async () => {
      const originalResults = await originalExecute();
      const filteredResults = [];

      for (const result of originalResults) {
        // 将结果转换为绑定映射
        const bindings = new Map<string, number>();
        for (const [key, value] of Object.entries(result)) {
          if (value !== null && value !== undefined) {
            // 获取节点 ID（如果值存在）
            const nodeId = this.store.getNodeIdByValue(String(value));
            if (nodeId !== undefined) {
              bindings.set(key, nodeId);
            }
          }
        }

        // 应用异步过滤器
        if (await filter(bindings)) {
          filteredResults.push(result);
        }
      }

      return filteredResults;
    };
  }

  /**
   * 编译属性映射
   */
  private compilePropertyMap(
    propMap: PropertyMap,
    context: CompileContext,
  ): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    for (const pair of propMap.properties) {
      const key = pair.key;
      const value = this.evaluateExpression(pair.value, context);
      result[key] = value;
    }

    return result;
  }

  /**
   * 计算表达式值
   */
  private evaluateExpression(expression: Expression, context: CompileContext): unknown {
    switch (expression.type) {
      case 'Literal':
        return (expression as Literal).value;

      case 'Variable':
        const varName = (expression as Variable).name;
        if (varName.startsWith('$')) {
          // 参数引用
          const paramName = varName.substring(1);
          if (!context.parameters.has(paramName)) {
            throw new CompileError(`未定义的参数: ${varName}`);
          }
          return context.parameters.get(paramName);
        }
        throw new CompileError(`变量引用在此上下文中无效: ${varName}`);

      case 'ParameterExpression':
        const paramExpr = expression as any; // ParameterExpression 类型
        const paramName = paramExpr.name;
        if (!context.parameters.has(paramName)) {
          throw new CompileError(`未定义的参数: $${paramName}`);
        }
        return context.parameters.get(paramName);

      default:
        throw new CompileError(`不支持的表达式类型: ${expression.type}`);
    }
  }

  /**
   * 编译 SET 子句
   */
  private compileSetClause(
    clause: SetClause,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // SET 子句目前作为后处理步骤，需要先匹配数据再设置属性
    // 这里先标记为待实现，实际需要扩展 PatternBuilder 支持更新操作
    throw new CompileError('SET 子句需要扩展 PatternBuilder 支持更新操作');
  }

  /**
   * 编译 DELETE 子句
   */
  private compileDeleteClause(
    clause: DeleteClause,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // DELETE 子句需要在匹配后删除节点/关系
    // 标记为待实现
    throw new CompileError('DELETE 子句需要扩展 PatternBuilder 支持删除操作');
  }

  /**
   * 编译 MERGE 子句
   */
  private compileMergeClause(
    clause: MergeClause,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // MERGE = MATCH + CREATE，需要特殊处理
    // 先尝试匹配，如果没有结果则创建
    throw new CompileError('MERGE 子句需要扩展 PatternBuilder 支持 upsert 操作');
  }

  /**
   * 编译 REMOVE 子句
   */
  private compileRemoveClause(
    clause: RemoveClause,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // REMOVE 需要移除节点属性或标签
    throw new CompileError('REMOVE 子句需要扩展 PatternBuilder 支持属性移除操作');
  }

  /**
   * 编译 UNWIND 子句
   */
  private compileUnwindClause(
    clause: UnwindClause,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // UNWIND 展开列表为行
    // 需要特殊的数据流处理
    throw new CompileError('UNWIND 子句需要扩展 PatternBuilder 支持列表展开操作');
  }

  /**
   * 编译 UNION 子句
   */
  private compileUnionClause(
    clause: UnionClause,
    builder: PatternBuilder,
    context: CompileContext,
  ): void {
    // UNION 合并多个查询结果
    // 需要支持结果集合并
    throw new CompileError('UNION 子句需要扩展 PatternBuilder 支持结果集合并操作');
  }

  /**
   * 清理查询计划器缓存
   */
  clearOptimizationCache(): void {
    this.planner.clearCache();
  }

  /**
   * 获取优化器统计信息
   */
  getOptimizerStats() {
    return {
      planner: this.planner.getCacheStats(),
    };
  }

  /**
   * 预热查询计划器（收集统计信息）
   */
  async warmUpOptimizer(): Promise<void> {
    // 强制收集统计信息
    try {
      const dummyQuery = {
        type: 'CypherQuery' as const,
        clauses: [
          {
            type: 'MatchClause' as const,
            optional: false,
            pattern: {
              type: 'Pattern' as const,
              elements: [
                {
                  type: 'NodePattern' as const,
                  variable: 'n',
                  labels: [],
                  properties: undefined,
                },
              ],
            },
          },
        ],
      };
      await this.planner.generatePlan(dummyQuery);
    } catch {
      // 预热失败不影响后续操作
    }
  }
}

/**
 * 便利函数：直接从文本编译并执行 Cypher 查询
 */
export async function executeCypher(
  store: PersistentStore,
  cypherText: string,
  parameters: Record<string, unknown> = {},
): Promise<PatternResult[]> {
  const { CypherParser } = await import('./parser.js');

  const parser = new CypherParser();
  const ast = parser.parse(cypherText);

  const compiler = new CypherCompiler(store);
  const result = compiler.compile(ast, parameters);

  return await result.execute();
}

/**
 * 便利函数：扩展 NervusDB 类以支持 Cypher 文本查询
 */
export interface CypherSupport {
  cypher(query: string, parameters?: Record<string, unknown>): Promise<PatternResult[]>;
}

export function addCypherSupport(store: PersistentStore): CypherSupport {
  return {
    cypher: (query: string, parameters: Record<string, unknown> = {}) => {
      return executeCypher(store, query, parameters);
    },
  };
}
