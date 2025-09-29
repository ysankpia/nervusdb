import { SynapseDBPlugin } from './base.js';
import type { SynapseDB } from '../synapseDb.js';
import { PersistentStore } from '../storage/persistentStore.js';
import { warnExperimental } from '../utils/experimental.js';
import {
  createCypherSupport,
  type CypherSupport,
  type CypherResult,
  type CypherExecutionOptions,
} from '../query/cypher.js';
import { VariablePathBuilder } from '../query/path/variable.js';

/**
 * Cypher查询插件
 *
 * 提供Cypher查询语言支持，包括：
 * - 标准Cypher查询接口
 * - 简化版Cypher（向后兼容）
 * - 语法验证
 * - 查询优化
 */
export class CypherPlugin implements SynapseDBPlugin {
  readonly name = 'cypher';
  readonly version = '1.0.0';

  private db!: SynapseDB;
  private store!: PersistentStore;
  private cypherSupport?: CypherSupport;

  initialize(db: SynapseDB, store: PersistentStore): void {
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
   * 简化版Cypher（向后兼容）
   * 仅支持 MATCH (a)-[:REL]->(b) RETURN a,b 模式
   */
  cypherSimple(query: string): Array<Record<string, unknown>> {
    const m =
      /MATCH\s*\((\w+)\)\s*-\s*\[:(\w+)(?:\*(\d+)?\.\.(\d+)?)?\]\s*->\s*\((\w+)\)\s*RETURN\s+(.+)/i.exec(
        query,
      );
    if (!m) throw new Error('仅支持最小子集：MATCH (a)-[:REL]->(b) RETURN ...');

    const aliasA = m[1];
    const rel = m[2];
    const minStr = m[3];
    const maxStr = m[4];
    const aliasB = m[5];
    const returnList = m[6].split(',').map((s) => s.trim());

    const hasVar = Boolean(minStr || maxStr);
    if (!hasVar) {
      const rows = this.db.find({ predicate: rel }).all();
      return rows.map((r) => {
        const env: Record<string, unknown> = {};
        const mapping: Record<string, string> = {
          [aliasA]: r.subject,
          [aliasB]: r.object,
        };
        for (const item of returnList) env[item] = mapping[item] ?? null;
        return env;
      });
    }

    const min = minStr ? Number(minStr) : 1;
    const max = maxStr ? Number(maxStr) : min;
    const pid = this.store.getNodeIdByValue(rel);
    if (pid === undefined) return [];

    const startIds = new Set<number>();
    const triples = this.db.find({ predicate: rel }).all();
    triples.forEach((t) => startIds.add(t.subjectId));

    const builder = new VariablePathBuilder(this.store, startIds, pid, {
      min,
      max,
      uniqueness: 'NODE',
      direction: 'forward',
    });
    const paths = builder.all();

    const out: Array<Record<string, unknown>> = [];
    for (const p of paths) {
      const env: Record<string, unknown> = {};
      const mapping: Record<string, string | null> = {
        [aliasA]: this.store.getNodeValueById(p.startId) ?? null,
        [aliasB]: this.store.getNodeValueById(p.endId) ?? null,
      };
      for (const item of returnList) env[item] = mapping[item] ?? null;
      out.push(env);
    }
    return out;
  }

  /**
   * 清理资源
   */
  cleanup(): void {
    this.cypherSupport = undefined;
  }
}
