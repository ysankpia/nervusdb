/**
 * Cypher 查询处理器
 *
 * 整合词法分析、语法分析、编译和执行的完整 Cypher 查询处理流程
 * 提供统一的 API 供 SynapseDB 使用
 */

import type { PersistentStore } from '../storage/persistentStore.js';
import type { PatternResult } from './pattern/match.js';
import { CypherLexer } from './pattern/lexer.js';
import { CypherParser } from './pattern/parser.js';
import { CypherCompiler, type CompilerOptions } from './pattern/compiler.js';

// Cypher 查询结果
export interface CypherResult {
  records: PatternResult[];
  summary: {
    statement: string;
    parameters: Record<string, unknown>;
    resultAvailableAfter: number;
    resultConsumedAfter: number;
    statementType: 'READ_ONLY' | 'WRITE_ONLY' | 'READ_WRITE' | 'SCHEMA_WRITE';
  };
  statistics?: {
    nodesCreated: number;
    nodesDeleted: number;
    relationshipsCreated: number;
    relationshipsDeleted: number;
    propertiesSet: number;
    labelsAdded: number;
    labelsRemoved: number;
  };
}

// Cypher 执行选项
export interface CypherExecutionOptions {
  timeout?: number; // 查询超时时间（毫秒）
  explain?: boolean; // 是否只返回执行计划
  profile?: boolean; // 是否返回性能分析
  readonly?: boolean; // 是否为只读查询
  enableOptimization?: boolean; // 是否启用查询优化
  optimizationLevel?: 'basic' | 'aggressive'; // 优化级别
}

/**
 * Cypher 查询处理器
 */
export class CypherProcessor {
  private readonly lexer: CypherLexer;
  private readonly parser: CypherParser;
  private readonly compiler: CypherCompiler;

  constructor(private readonly store: PersistentStore) {
    this.lexer = new CypherLexer('');
    this.parser = new CypherParser();
    this.compiler = new CypherCompiler(store);
  }

  /**
   * 执行 Cypher 查询
   */
  async execute(
    statement: string,
    parameters: Record<string, unknown> = {},
    options: CypherExecutionOptions = {},
  ): Promise<CypherResult> {
    const startTime = Date.now();

    try {
      // 1. 词法分析
      let ast;
      if (options.enableOptimization) {
        // 优化路径：直接复用解析器的一体化入口，减少一次对象创建
        ast = this.parser.parse(statement);
      } else {
        // 传统路径：分步词法+语法，便于调试与观测
        const lexer = new CypherLexer(statement);
        const tokens = lexer.tokenize();
        const parser = new CypherParser();
        ast = parser.parseTokens(tokens);
      }

      const parseTime = Date.now();

      // 3. 查询分析和分类
      const statementType = this.analyzeStatementType(statement);

      // 4. 只读检查
      if (options.readonly && statementType !== 'READ_ONLY') {
        throw new Error('在只读模式下不能执行写操作');
      }

      // 5. 编译执行
      const compilerOptions: CompilerOptions = {
        enableOptimization: options.enableOptimization || false,
        optimizationLevel: options.optimizationLevel || 'basic',
      };
      const compileResult = this.compiler.compile(ast, parameters, compilerOptions);
      // const compileTime = Date.now(); // 编译时间可用于分析，当前未使用

      // 6. 执行查询
      const records = await compileResult.execute();
      const executeTime = Date.now();

      // 7. 构建结果
      const result: CypherResult = {
        records,
        summary: {
          statement,
          parameters,
          resultAvailableAfter: parseTime - startTime,
          resultConsumedAfter: executeTime - startTime,
          statementType,
        },
      };

      return result;
    } catch (error) {
      throw new CypherError(
        `Cypher 查询执行失败: ${error instanceof Error ? error.message : '未知错误'}`,
        statement,
        parameters,
      );
    }
  }

  /**
   * 验证 Cypher 查询语法
   */
  validate(statement: string): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    try {
      const lexer = new CypherLexer(statement);
      const tokens = lexer.tokenize();

      const parser = new CypherParser();
      parser.parseTokens(tokens);

      return { valid: true, errors: [] };
    } catch (error) {
      if (error instanceof Error) {
        errors.push(error.message);
      } else {
        errors.push('语法验证失败');
      }
      return { valid: false, errors };
    }
  }

  /**
   * 分析语句类型
   */
  private analyzeStatementType(statement: string): CypherResult['summary']['statementType'] {
    const upperStatement = statement.toUpperCase().trim();

    // 简单的启发式分析
    if (
      upperStatement.includes('CREATE') ||
      upperStatement.includes('DELETE') ||
      upperStatement.includes('SET') ||
      upperStatement.includes('REMOVE') ||
      upperStatement.includes('MERGE')
    ) {
      return 'READ_WRITE';
    }

    if (
      upperStatement.includes('MATCH') ||
      upperStatement.includes('RETURN') ||
      upperStatement.includes('WITH')
    ) {
      return 'READ_ONLY';
    }

    // 默认为读写操作
    return 'READ_WRITE';
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
 * Cypher 错误类
 */
export class CypherError extends Error {
  constructor(
    message: string,
    public readonly statement?: string,
    public readonly parameters?: Record<string, unknown>,
  ) {
    super(message);
    this.name = 'CypherError';
  }
}

/**
 * 扩展 SynapseDB 以支持 Cypher 查询
 */
export interface CypherSupport {
  /**
   * 执行 Cypher 查询
   */
  cypher(
    statement: string,
    parameters?: Record<string, unknown>,
    options?: CypherExecutionOptions,
  ): Promise<CypherResult>;

  /**
   * 执行只读 Cypher 查询
   */
  cypherRead(
    statement: string,
    parameters?: Record<string, unknown>,
    options?: CypherExecutionOptions,
  ): Promise<CypherResult>;

  /**
   * 验证 Cypher 查询语法
   */
  validateCypher(statement: string): { valid: boolean; errors: string[] };

  /**
   * 清理查询优化器缓存
   */
  clearOptimizationCache(): void;

  /**
   * 获取优化器统计信息
   */
  getOptimizerStats(): unknown;

  /**
   * 预热查询优化器
   */
  warmUpOptimizer(): Promise<void>;
}

/**
 * 为 SynapseDB 添加 Cypher 支持的工厂函数
 */
export function createCypherSupport(store: PersistentStore): CypherSupport {
  const processor = new CypherProcessor(store);

  return {
    cypher: (
      statement: string,
      parameters: Record<string, unknown> = {},
      options: CypherExecutionOptions = {},
    ) => processor.execute(statement, parameters, options),

    cypherRead: (
      statement: string,
      parameters: Record<string, unknown> = {},
      options: CypherExecutionOptions = {},
    ) => processor.execute(statement, parameters, { ...options, readonly: true }),

    validateCypher: (statement: string) => processor.validate(statement),

    clearOptimizationCache: () => processor.clearOptimizationCache(),

    getOptimizerStats: () => processor.getOptimizerStats(),

    warmUpOptimizer: () => processor.warmUpOptimizer(),
  };
}
