/**
 * Cypher 查询处理器
 *
 * 简化版本 - 通过 Native 调用 Rust Core 的 Cypher 实现
 * TS 版解析器已归档到 _archive/ts-query-pattern/
 */

import type { PersistentStore } from '../../core/storage/persistentStore.js';

export type PatternResult = Record<string, unknown>;

export interface CypherResult {
  records: PatternResult[];
  summary: {
    statement: string;
    parameters: Record<string, unknown>;
    resultAvailableAfter: number;
    resultConsumedAfter: number;
    statementType: 'READ_ONLY' | 'WRITE_ONLY' | 'READ_WRITE' | 'SCHEMA_WRITE';
    native?: boolean;
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

export interface CypherExecutionOptions {
  timeout?: number;
  explain?: boolean;
  profile?: boolean;
  readonly?: boolean;
  enableOptimization?: boolean;
  optimizationLevel?: 'basic' | 'aggressive';
}

/**
 * Cypher 查询处理器
 * 依赖 Rust Core 的 Cypher 实现
 */
export class CypherProcessor {
  constructor(private readonly store: PersistentStore) {}

  async execute(
    statement: string,
    parameters: Record<string, unknown> = {},
    options: CypherExecutionOptions = {},
  ): Promise<CypherResult> {
    const startTime = Date.now();

    try {
      // 通过 Native 执行 Cypher
      const records = this.store.executeQuery(statement, parameters);
      const executeTime = Date.now();

      const statementType = this.analyzeStatementType(statement);

      if (options.readonly && statementType !== 'READ_ONLY') {
        throw new Error('在只读模式下不能执行写操作');
      }

      return {
        records: records as PatternResult[],
        summary: {
          statement,
          parameters,
          resultAvailableAfter: executeTime - startTime,
          resultConsumedAfter: executeTime - startTime,
          statementType,
          native: true,
        },
      };
    } catch (error) {
      throw new CypherError(
        `Cypher 查询执行失败: ${error instanceof Error ? error.message : '未知错误'}`,
        statement,
        parameters,
      );
    }
  }

  validate(statement: string): { valid: boolean; errors: string[] } {
    // 基础语法检查
    const errors: string[] = [];

    if (!statement.trim()) {
      errors.push('查询语句不能为空');
    }

    // 检查基本关键字
    const upper = statement.toUpperCase();
    const hasClause =
      upper.includes('MATCH') ||
      upper.includes('CREATE') ||
      upper.includes('RETURN') ||
      upper.includes('DELETE') ||
      upper.includes('SET');

    if (!hasClause) {
      errors.push('缺少有效的 Cypher 子句 (MATCH, CREATE, RETURN, DELETE, SET)');
    }

    return { valid: errors.length === 0, errors };
  }

  private analyzeStatementType(statement: string): CypherResult['summary']['statementType'] {
    const upper = statement.toUpperCase().trim();

    if (
      upper.includes('CREATE') ||
      upper.includes('DELETE') ||
      upper.includes('SET') ||
      upper.includes('REMOVE') ||
      upper.includes('MERGE')
    ) {
      return 'READ_WRITE';
    }

    if (upper.includes('MATCH') || upper.includes('RETURN') || upper.includes('WITH')) {
      return 'READ_ONLY';
    }

    return 'READ_WRITE';
  }

  clearOptimizationCache(): void {
    // Rust Core 管理缓存
  }

  getOptimizerStats(): Record<string, unknown> {
    return { source: 'rust-core' };
  }

  async warmUpOptimizer(): Promise<void> {
    // Rust Core 处理
  }
}

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

export interface CypherSupport {
  cypher(
    statement: string,
    parameters?: Record<string, unknown>,
    options?: CypherExecutionOptions,
  ): Promise<CypherResult>;

  cypherRead(
    statement: string,
    parameters?: Record<string, unknown>,
    options?: CypherExecutionOptions,
  ): Promise<CypherResult>;

  validateCypher(statement: string): { valid: boolean; errors: string[] };
  clearOptimizationCache(): void;
  getOptimizerStats(): unknown;
  warmUpOptimizer(): Promise<void>;
}

export function createCypherSupport(store: PersistentStore): CypherSupport {
  const processor = new CypherProcessor(store);

  return {
    cypher: (statement, parameters = {}, options = {}) =>
      processor.execute(statement, parameters, options),

    cypherRead: (statement, parameters = {}, options = {}) =>
      processor.execute(statement, parameters, { ...options, readonly: true }),

    validateCypher: (statement) => processor.validate(statement),
    clearOptimizationCache: () => processor.clearOptimizationCache(),
    getOptimizerStats: () => processor.getOptimizerStats(),
    warmUpOptimizer: () => processor.warmUpOptimizer(),
  };
}
