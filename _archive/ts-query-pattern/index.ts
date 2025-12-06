/**
 * NervusDB 模式匹配模块入口
 *
 * 提供完整的 Cypher 模式匹配功能：
 * - 文本解析（词法分析 + 语法分析）
 * - AST 编译执行
 * - 与现有 PatternBuilder 的无缝集成
 */

// 导出核心组件
export { CypherLexer } from './lexer.js';
export type { Token, TokenType } from './lexer.js';

export { CypherParser, ParseError } from './parser.js';

export { CypherCompiler, CompileError, executeCypher, addCypherSupport } from './compiler.js';
export type { CompileResult, CypherSupport, CompilerOptions } from './compiler.js';

export { CypherQueryPlanner } from './planner.js';
export type {
  PlanNode,
  IndexScanPlan,
  JoinPlan,
  FilterPlan,
  ProjectPlan,
  LimitPlan,
  Statistics,
} from './planner.js';

export { CypherQueryExecutor } from './executor.js';

export { PatternBuilder } from './match.js';
export type { PatternResult } from './match.js';

// 导出 AST 类型
export type {
  ASTNode,
  CypherQuery,
  Clause,
  MatchClause,
  CreateClause,
  ReturnClause,
  WhereClause,
  WithClause,
  Pattern,
  PathElement,
  NodePattern,
  RelationshipPattern,
  Expression,
  Literal,
  Variable,
  PropertyAccess,
  BinaryExpression,
  UnaryExpression,
  PropertyMap,
  PropertyPair,
  VariableLength,
  Direction,
  ReturnItem,
  OrderByClause,
  OrderByItem,
  SourceLocation,
  Position,
} from './ast.js';

// 便利函数：一站式 Cypher 查询执行
import type { PersistentStore } from '../../../core/storage/persistentStore.js';
import { CypherParser } from './parser.js';
import { CypherCompiler, type CompilerOptions } from './compiler.js';

/**
 * 高级 Cypher 查询执行器
 */
export class CypherEngine {
  private parser = new CypherParser();
  private compiler: CypherCompiler;

  constructor(private readonly store: PersistentStore) {
    this.compiler = new CypherCompiler(store);
  }

  /**
   * 执行 Cypher 查询
   */
  async execute(
    cypherText: string,
    parameters: Record<string, unknown> = {},
    options: CompilerOptions = {},
  ): Promise<import('./match.js').PatternResult[]> {
    try {
      // 1. 解析文本为 AST
      const ast = this.parser.parse(cypherText);

      // 2. 编译 AST 为可执行代码（支持优化选项）
      const compiled = this.compiler.compile(ast, parameters, options);

      // 3. 执行查询
      return await compiled.execute();
    } catch (error) {
      if (error instanceof Error) {
        throw new Error(`Cypher 查询执行失败: ${error.message}`);
      }
      throw error;
    }
  }

  /**
   * 验证 Cypher 语法
   */
  validate(cypherText: string): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    try {
      this.parser.parse(cypherText);
      return { valid: true, errors: [] };
    } catch (error) {
      if (error instanceof Error) {
        errors.push(error.message);
      }
      return { valid: false, errors };
    }
  }

  /**
   * 解析并返回 AST（用于调试）
   */
  parseAST(cypherText: string) {
    return this.parser.parse(cypherText);
  }

  /**
   * 获取支持的语法帮助
   */
  getSupportedSyntax(): string[] {
    return [
      'MATCH (variable:Label {property: value})',
      'MATCH (a)-[:RELATION]->(b)',
      'MATCH (a)-[:REL*1..3]->(b)',
      'WHERE variable.property = value',
      'WHERE variable.property > value',
      'RETURN variable1, variable2',
      '参数化查询: {property: $param}',
    ];
  }

  /**
   * 清理查询优化器缓存
   */
  clearOptimizationCache(): void {
    this.compiler.clearOptimizationCache();
  }

  /**
   * 获取优化器统计信息
   */
  getOptimizerStats() {
    return this.compiler.getOptimizerStats();
  }

  /**
   * 预热查询优化器
   */
  async warmUpOptimizer(): Promise<void> {
    await this.compiler.warmUpOptimizer();
  }
}

/**
 * 创建 Cypher 引擎实例
 */
export function createCypherEngine(store: PersistentStore): CypherEngine {
  return new CypherEngine(store);
}

/**
 * 扩展类型定义：为 NervusDB 添加 Cypher 支持
 */
export interface NervusDBWithCypher {
  cypher(
    query: string,
    parameters?: Record<string, unknown>,
  ): Promise<import('./match.js').PatternResult[]>;
  cypherEngine: CypherEngine;
}

/**
 * 为 NervusDB 实例添加 Cypher 功能
 */
export function enhanceWithCypher<T extends { store?: PersistentStore }>(
  dbInstance: T,
  store: PersistentStore,
): T & NervusDBWithCypher {
  const engine = new CypherEngine(store);

  const enhanced = Object.assign(dbInstance, {
    cypher: (query: string, parameters: Record<string, unknown> = {}) => {
      return engine.execute(query, parameters);
    },
    cypherEngine: engine,
  });

  return enhanced as T & NervusDBWithCypher;
}

// 导出类型守卫和工具函数
export function isCypherQuery(text: string): boolean {
  const trimmed = text.trim().toUpperCase();
  return (
    trimmed.startsWith('MATCH') ||
    trimmed.startsWith('CREATE') ||
    trimmed.startsWith('RETURN') ||
    trimmed.startsWith('WITH')
  );
}

/**
 * 格式化 Cypher 查询结果为可读格式
 */
export function formatCypherResults(
  results: import('./match.js').PatternResult[],
  options: { limit?: number; format?: 'table' | 'json' } = {},
): string {
  const { limit = 50, format = 'table' } = options;
  const limitedResults = results.slice(0, limit);

  if (format === 'json') {
    return JSON.stringify(limitedResults, null, 2);
  }

  // 表格格式
  if (limitedResults.length === 0) {
    return '(no results)';
  }

  // 获取所有列名
  const columns = new Set<string>();
  for (const row of limitedResults) {
    Object.keys(row).forEach((key) => columns.add(key));
  }

  const columnArray = Array.from(columns).sort();

  // 构建表格
  const rows: string[] = [];

  // 表头
  rows.push('| ' + columnArray.join(' | ') + ' |');
  rows.push('|' + columnArray.map(() => '---').join('|') + '|');

  // 数据行
  for (const row of limitedResults) {
    const values = columnArray.map((col) => {
      const value = row[col];
      return value === null || value === undefined
        ? 'null'
        : typeof value === 'string'
          ? `"${value}"`
          : String(value);
    });
    rows.push('| ' + values.join(' | ') + ' |');
  }

  if (results.length > limit) {
    rows.push(`... (${results.length - limit} more rows)`);
  }

  return rows.join('\n');
}
