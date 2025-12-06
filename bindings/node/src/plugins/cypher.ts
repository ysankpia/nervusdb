import { NervusDBPlugin } from './base.js';
import type { NervusDB } from '../synapseDb.js';
import { PersistentStore } from '../core/storage/persistentStore.js';
import { warnExperimental } from '../utils/experimental.js';
import {
  createCypherSupport,
  type CypherSupport,
  type CypherResult,
  type CypherExecutionOptions,
} from '../extensions/query/cypher.js';

/**
 * Cypher查询插件
 *
 * 提供Cypher查询语言支持，包括：
 * - 标准Cypher查询接口
 * - 简化版Cypher（向后兼容）
 * - 语法验证
 * - 查询优化
 */
export class CypherPlugin implements NervusDBPlugin {
  readonly name = 'cypher';
  readonly version = '1.0.0';

  private db!: NervusDB;
  private store!: PersistentStore;
  private cypherSupport?: CypherSupport;

  initialize(db: NervusDB, store: PersistentStore): void {
    this.db = db;
    this.store = store;
    warnExperimental('Cypher 查询语言前端');
  }

  /**
   * 获取Cypher支持实例（延迟初始化）
   */
  private getCypherSupport(): CypherSupport {
    if (!this.cypherSupport) {
      this.cypherSupport = createCypherSupport(this.store);
    }
    return this.cypherSupport;
  }

  /**
   * 执行Cypher查询（标准异步接口）
   */
  async cypherQuery(
    statement: string,
    parameters: Record<string, unknown> = {},
    options: CypherExecutionOptions = {},
  ): Promise<CypherResult> {
    warnExperimental('Cypher 查询语言前端');
    const cypher = this.getCypherSupport();
    return cypher.cypher(statement, parameters, options);
  }

  /**
   * 执行只读Cypher查询
   */
  async cypherRead(
    statement: string,
    parameters: Record<string, unknown> = {},
    options: CypherExecutionOptions = {},
  ): Promise<CypherResult> {
    warnExperimental('Cypher 查询语言前端');
    const cypher = this.getCypherSupport();
    return cypher.cypherRead(statement, parameters, options);
  }

  /**
   * 验证Cypher语法
   */
  validateCypher(statement: string): { valid: boolean; errors: string[] } {
    warnExperimental('Cypher 查询语言前端');
    const cypher = this.getCypherSupport();
    return cypher.validateCypher(statement);
  }

  /**
   * 清理Cypher优化器缓存
   */
  clearCypherOptimizationCache(): void {
    warnExperimental('Cypher 查询语言前端');
    const cypher = this.getCypherSupport();
    cypher.clearOptimizationCache();
  }

  /**
   * 获取Cypher优化器统计信息
   */
  getCypherOptimizerStats(): unknown {
    warnExperimental('Cypher 查询语言前端');
    const cypher = this.getCypherSupport();
    return cypher.getOptimizerStats();
  }

  /**
   * 预热Cypher优化器
   */
  async warmUpCypherOptimizer(): Promise<void> {
    warnExperimental('Cypher 查询语言前端');
    const cypher = this.getCypherSupport();
    await cypher.warmUpOptimizer();
  }

  /**
   * 清理资源
   */
  cleanup(): void {
    this.cypherSupport = undefined;
  }
}
