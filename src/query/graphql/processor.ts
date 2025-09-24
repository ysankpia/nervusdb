/**
 * GraphQL 查询处理器
 *
 * 解析和执行 GraphQL 查询，将其转换为 SynapseDB 查询
 * 支持查询优化、缓存和批量加载
 */

import type { PersistentStore } from '../../storage/persistentStore.js';
import type {
  ParsedGraphQLQuery,
  ParsedField,
  ParsedFragment,
  GraphQLExecutionResult,
  GraphQLError,
  GraphQLContext,
  DataLoaderConfig,
  EntityTypeInfo,
  GeneratedSchema,
} from './types.js';
import { SchemaDiscovery } from './discovery.js';
import { SchemaBuilder } from './builder.js';

/**
 * 数据加载器（简化版）
 */
class DataLoader<K, V> {
  private cache = new Map<string, V>();
  private batchLoader: (keys: K[]) => Promise<V[]>;
  private config: DataLoaderConfig;

  constructor(batchLoader: (keys: K[]) => Promise<V[]>, config: DataLoaderConfig = {}) {
    this.batchLoader = batchLoader;
    this.config = {
      batchSize: 50,
      cache: true,
      cacheKeyFn: (key) => String(key),
      ...config,
    };
  }

  async load(key: K): Promise<V> {
    const cacheKey = this.config.cacheKeyFn!(key);

    if (this.config.cache && this.cache.has(cacheKey)) {
      return this.cache.get(cacheKey)!;
    }

    const results = await this.batchLoader([key]);
    const value = results[0];

    if (this.config.cache && value !== undefined) {
      this.cache.set(cacheKey, value);
    }

    return value;
  }

  async loadMany(keys: K[]): Promise<V[]> {
    return Promise.all(keys.map((key) => this.load(key)));
  }

  clear(key: K): void {
    const cacheKey = this.config.cacheKeyFn!(key);
    this.cache.delete(cacheKey);
  }

  clearAll(): void {
    this.cache.clear();
  }
}

/**
 * GraphQL 查询处理器
 */
export class GraphQLProcessor {
  private store: PersistentStore;
  private schema?: GeneratedSchema;
  private entityTypes: EntityTypeInfo[] = [];
  private dataLoaders = new Map<string, DataLoader<any, any>>();

  constructor(store: PersistentStore) {
    this.store = store;
  }

  /**
   * 初始化处理器，生成 Schema
   */
  async initialize(): Promise<GeneratedSchema> {
    const discovery = new SchemaDiscovery(this.store);
    this.entityTypes = await discovery.discoverEntityTypes();

    const builder = new SchemaBuilder(this.store);
    this.schema = await builder.buildSchema(this.entityTypes);

    return this.schema;
  }

  /**
   * 获取生成的 Schema
   */
  getSchema(): GeneratedSchema | undefined {
    return this.schema;
  }

  /**
   * 执行 GraphQL 查询
   */
  async executeQuery(
    query: string,
    variables: Record<string, unknown> = {},
    contextValue: Partial<GraphQLContext> = {},
  ): Promise<GraphQLExecutionResult> {
    try {
      if (!this.schema) {
        await this.initialize();
      }

      const context: GraphQLContext = {
        store: this.store,
        loaders: this.createDataLoaders(),
        ...contextValue,
      };

      const parsedQuery = this.parseQuery(query);
      const result = await this.executeOperation(parsedQuery, variables, context);

      return {
        data: result,
      };
    } catch (error) {
      return {
        errors: [this.createGraphQLError(error)],
      };
    }
  }

  /**
   * 解析 GraphQL 查询（简化实现）
   */
  private parseQuery(query: string): ParsedGraphQLQuery {
    // 这里应该使用真正的 GraphQL 解析器，这里提供一个简化的实现
    const lines = query
      .trim()
      .split('\n')
      .map((line) => line.trim());

    // 检测操作类型
    let operationType: 'query' | 'mutation' | 'subscription' = 'query';
    let operationName: string | undefined;

    const firstLine = lines[0];
    if (firstLine.startsWith('mutation')) {
      operationType = 'mutation';
    } else if (firstLine.startsWith('subscription')) {
      operationType = 'subscription';
    }

    // 提取操作名称
    const nameMatch = firstLine.match(/(?:query|mutation|subscription)\s+(\w+)/);
    if (nameMatch) {
      operationName = nameMatch[1];
    }

    // 简化的字段解析
    const fields = this.parseFields(query);

    return {
      operationType,
      operationName,
      fields,
      variables: {},
      fragments: {},
    };
  }

  /**
   * 简化的字段解析
   */
  private parseFields(query: string): ParsedField[] {
    const fields: ParsedField[] = [];

    // 仅提取第一个选择集 {...} 内的顶层字段
    const src = query;
    const firstBrace = src.indexOf('{');
    if (firstBrace < 0) return fields;
    let i = firstBrace;
    const readBalancedOuter = (open: string, close: string): string => {
      let depth = 0;
      const start = i;
      if (src[i] !== open) return '';
      while (i < src.length) {
        const ch = src[i++]!;
        if (ch === open) depth++;
        else if (ch === close) {
          depth--;
          if (depth === 0) break;
        }
      }
      return src.slice(start + 1, i - 1);
    };
    // 读取顶层选择集内容
    const body = readBalancedOuter('{', '}');
    const len = body.length;
    i = 0;

    const isWord = (c: string) => /[A-Za-z_]/.test(c);
    const isWordPart = (c: string) => /[A-Za-z0-9_]/.test(c);

    const skipWs = () => {
      while (i < len && /\s/.test(body[i]!)) i++;
    };

    const readWord = (): string => {
      let s = '';
      if (i < len && isWord(body[i]!)) {
        s += body[i++]!;
        while (i < len && isWordPart(body[i]!)) s += body[i++]!;
      }
      return s;
    };

    const readBalanced = (open: string, close: string): string => {
      let depth = 0;
      const start = i;
      if (body[i] !== open) return '';
      while (i < len) {
        const ch = body[i++]!;
        if (ch === open) depth++;
        else if (ch === close) {
          depth--;
          if (depth === 0) break;
        }
      }
      return body.slice(start + 1, i - 1);
    };

    const parseArgs = (raw: string): Record<string, unknown> => {
      // 极简参数解析器：仅支持 first/last/after/before 与 filter: { k: v }
      const args: Record<string, unknown> = {};
      const s = raw.trim();
      if (!s) return args;
      // 粗略按顶层逗号分割
      let j = 0;
      const parts: string[] = [];
      let depth = 0;
      let last = 0;
      for (; j < s.length; j++) {
        const ch = s[j]!;
        if (ch === '{') depth++;
        else if (ch === '}') depth--;
        else if (ch === ',' && depth === 0) {
          parts.push(s.slice(last, j));
          last = j + 1;
        }
      }
      parts.push(s.slice(last));

      for (const part of parts) {
        const seg = part.trim();
        if (!seg) continue;
        const idx = seg.indexOf(':');
        if (idx <= 0) continue;
        const key = seg.slice(0, idx).trim();
        const valRaw = seg.slice(idx + 1).trim();
        if (key === 'filter' && valRaw.startsWith('{')) {
          const body = valRaw.slice(1, -1); // 去掉花括号
          const f: Record<string, unknown> = {};
          let k = 0;
          let d = 0;
          let last2 = 0;
          const parts2: string[] = [];
          for (; k < body.length; k++) {
            const ch2 = body[k]!;
            if (ch2 === '{') d++;
            else if (ch2 === '}') d--;
            else if (ch2 === ',' && d === 0) {
              parts2.push(body.slice(last2, k));
              last2 = k + 1;
            }
          }
          parts2.push(body.slice(last2));
          for (const item of parts2) {
            const seg2 = item.trim();
            if (!seg2) continue;
            const i2 = seg2.indexOf(':');
            if (i2 <= 0) continue;
            const k2 = seg2.slice(0, i2).trim();
            const v2 = seg2.slice(i2 + 1).trim();
            const num = Number(v2);
            f[k2] = Number.isNaN(num) ? v2.replace(/^['"]|['"]$/g, '') : num;
          }
          args.filter = f;
        } else {
          const num = Number(valRaw);
          args[key] = Number.isNaN(num) ? valRaw.replace(/^['"]|['"]$/g, '') : num;
        }
      }

      return args;
    };

    // 扫描顶层块
    while (i < len) {
      skipWs();
      const name = readWord();
      if (!name) {
        i++;
        continue;
      }

      skipWs();
      let args: Record<string, unknown> = {};
      if (body[i] === '(') {
        const raw = readBalanced('(', ')');
        args = parseArgs(raw);
        skipWs();
      }
      if (body[i] === '{') {
        // 进入子选择集（忽略细节）
        readBalanced('{', '}');
      }
      // 跳过 GraphQL 内置类型关键字（这里 body 已经是选择集体，通常无需）
      if (!['query', 'mutation', 'subscription'].includes(name)) {
        fields.push({ name, args, fields: [], type: 'Unknown' });
      }
    }

    return fields;
  }

  /**
   * 执行操作
   */
  private async executeOperation(
    parsedQuery: ParsedGraphQLQuery,
    variables: Record<string, unknown>,
    context: GraphQLContext,
  ): Promise<Record<string, unknown>> {
    const result: Record<string, unknown> = {};

    for (const field of parsedQuery.fields) {
      try {
        const value = await this.resolveField(field, null, variables, context);
        if (value !== undefined) {
          result[field.alias || field.name] = value;
        }
      } catch (error) {
        // 字段级错误处理
        console.warn(`Failed to resolve field ${field.name}:`, error);
        // 错误场景返回 null；不存在的字段则保持 undefined（不写入 data）
        result[field.alias || field.name] = null;
      }
    }

    return result;
  }

  /**
   * 解析字段
   */
  private async resolveField(
    field: ParsedField,
    parent: unknown,
    variables: Record<string, unknown>,
    context: GraphQLContext,
  ): Promise<unknown> {
    // 查找对应的解析器
    const resolver = this.findResolver(field.name, parent);

    if (resolver) {
      // 合并变量中的通用参数键，弥补简化解析器对参数解析的缺失
      const mergedArgs: Record<string, unknown> = { ...(field.args || {}) };
      for (const k of ['first', 'last', 'after', 'before', 'filter', 'sort']) {
        if (mergedArgs[k] === undefined && variables && (variables as any)[k] !== undefined) {
          mergedArgs[k] = (variables as any)[k];
        }
      }

      return await resolver(parent, mergedArgs, context);
    }

    // 如果没有解析器，尝试从 parent 对象中获取属性
    if (parent && typeof parent === 'object' && field.name in parent) {
      return (parent as any)[field.name];
    }

    // 默认解析逻辑
    return await this.defaultFieldResolver(field, parent, context);
  }

  /**
   * 查找解析器
   */
  private findResolver(fieldName: string, parent: unknown): any {
    if (!this.schema) {
      return null;
    }

    // 根字段解析器
    if (parent === null) {
      return this.schema.resolvers.Query?.[fieldName];
    }

    // 对象字段解析器
    if (parent && typeof parent === 'object' && 'label' in parent) {
      const typeName = (parent as any).label;
      return this.schema.resolvers[typeName]?.[fieldName];
    }

    return null;
  }

  /**
   * 默认字段解析器
   */
  private async defaultFieldResolver(
    field: ParsedField,
    parent: unknown,
    context: GraphQLContext,
  ): Promise<unknown> {
    // 如果是根查询字段，尝试从实体类型中查找
    if (parent === null) {
      const entityType = this.findEntityTypeByFieldName(field.name);
      if (entityType) {
        // 返回该类型的所有实例（简化实现）
        return entityType.sampleIds.map((id) => ({
          id: Number(id),
          label: entityType.typeName,
        }));
      }
      // 未匹配到已知类型的根字段：视为不存在，返回 undefined
      return undefined;
    }

    // 属性字段解析
    if (parent && typeof parent === 'object' && 'id' in parent) {
      return await this.resolveProperty(field.name, (parent as any).id, context);
    }

    return null;
  }

  /**
   * 解析属性
   */
  private async resolveProperty(
    propertyName: string,
    entityId: number,
    context: GraphQLContext,
  ): Promise<unknown> {
    // 查找对应的谓词
    const predicateId = context.store.getNodeIdByValue(propertyName);
    if (predicateId === undefined) {
      return null;
    }

    const records = context.store.resolveRecords(
      context.store.query({ subjectId: entityId, predicateId }),
      { includeProperties: false },
    );

    if (records.length === 0) {
      return null;
    }

    if (records.length === 1) {
      return context.store.getNodeValueById(records[0].objectId);
    }

    // 多个值，返回数组
    return records.map((record: any) => context.store.getNodeValueById(record.objectId));
  }

  /**
   * 根据字段名查找实体类型
   */
  private findEntityTypeByFieldName(fieldName: string): EntityTypeInfo | undefined {
    // 尝试单数形式
    let match = this.entityTypes.find(
      (type) => type.typeName.toLowerCase() === fieldName.toLowerCase(),
    );

    if (match) {
      return match;
    }

    // 尝试复数形式
    const singularForm = fieldName.endsWith('s') ? fieldName.slice(0, -1) : fieldName;
    match = this.entityTypes.find(
      (type) => type.typeName.toLowerCase() === singularForm.toLowerCase(),
    );

    return match;
  }

  /**
   * 创建数据加载器
   */
  private createDataLoaders(): Record<string, DataLoader<any, any>> {
    const loaders: Record<string, DataLoader<any, any>> = {};

    // 节点值加载器
    loaders.nodeValue = new DataLoader(async (nodeIds: number[]) => {
      return nodeIds.map((id) => this.store.getNodeValueById(id));
    });

    // 节点属性加载器
    loaders.nodeProperties = new DataLoader(async (nodeIds: number[]) => {
      return nodeIds.map((id) => this.store.getNodeProperties(id) || {});
    });

    // 关系加载器
    loaders.relations = new DataLoader(async (params: { nodeId: number; predicate: string }[]) => {
      return params.map(({ nodeId, predicate }) => {
        const predicateId = this.store.getNodeIdByValue(predicate);
        if (predicateId === undefined) {
          return [];
        }

        const records = this.store.resolveRecords(
          this.store.query({ subjectId: nodeId, predicateId }),
          { includeProperties: false },
        );

        return records.map((record) => ({
          id: record.objectId,
          predicateId: record.predicateId,
        }));
      });
    });

    return loaders;
  }

  /**
   * 创建 GraphQL 错误
   */
  private createGraphQLError(error: unknown): GraphQLError {
    if (error instanceof Error) {
      return {
        message: error.message,
        extensions: {
          code: 'INTERNAL_ERROR',
          stack: error.stack,
        },
      };
    }

    return {
      message: String(error),
      extensions: {
        code: 'UNKNOWN_ERROR',
      },
    };
  }

  /**
   * 清理资源
   */
  dispose(): void {
    this.dataLoaders.clear();
  }
}

/**
 * GraphQL 查询验证器
 */
export class GraphQLValidator {
  private schema: GeneratedSchema;

  constructor(schema: GeneratedSchema) {
    this.schema = schema;
  }

  /**
   * 验证查询
   */
  validateQuery(query: string): GraphQLError[] {
    const errors: GraphQLError[] = [];

    try {
      // 基本语法检查
      if (!query.trim()) {
        errors.push({
          message: '查询不能为空',
        });
      }

      // 检查是否有匹配的大括号
      const openBraces = (query.match(/{/g) || []).length;
      const closeBraces = (query.match(/}/g) || []).length;

      if (openBraces !== closeBraces) {
        errors.push({
          message: '大括号不匹配',
        });
      }

      // 更多验证规则可以在这里添加
    } catch (error) {
      errors.push({
        message: `查询验证失败: ${error}`,
      });
    }

    return errors;
  }

  /**
   * 检查查询复杂度
   */
  calculateQueryComplexity(query: string): number {
    // 简化的复杂度计算
    const fieldCount = (query.match(/\w+\s*{/g) || []).length;
    const depth = this.calculateQueryDepth(query);

    return fieldCount * depth;
  }

  /**
   * 计算查询深度
   */
  private calculateQueryDepth(query: string): number {
    let maxDepth = 0;
    let currentDepth = 0;

    for (const char of query) {
      if (char === '{') {
        currentDepth++;
        maxDepth = Math.max(maxDepth, currentDepth);
      } else if (char === '}') {
        currentDepth--;
      }
    }

    return maxDepth;
  }
}
