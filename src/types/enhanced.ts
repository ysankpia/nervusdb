/**
 * TypeScript 类型系统增强 - v1.1 里程碑
 * 提供泛型化的 NervusDB API，支持强类型的属性和查询结果
 */

import type { FactInput, FactRecord } from '../storage/persistentStore.js';
import type { FactCriteria, FrontierOrientation } from '../query/queryBuilder.js';

/**
 * 节点属性约束类型
 */
export type NodeProperties = Record<string, unknown>;

/**
 * 边属性约束类型
 */
export type EdgeProperties = Record<string, unknown>;

/**
 * 标签数组类型
 */
export type Labels = string[];

/**
 * 带类型的节点属性（包含可选的 labels）
 */
export interface TypedNodeProperties extends NodeProperties {
  labels?: Labels;
}

/**
 * 泛型化的 Fact 输入类型
 */
// TypedFactInput 仅为语义化别名，避免空接口报错与未用泛型
export type TypedFactInput = FactInput;

/**
 * 泛型化的 Fact 选项
 */
export interface TypedFactOptions<
  TNodeProps extends NodeProperties = NodeProperties,
  TEdgeProps extends EdgeProperties = EdgeProperties,
> {
  subjectProperties?: TNodeProps;
  objectProperties?: TNodeProps;
  edgeProperties?: TEdgeProps;
}

/**
 * 泛型化的 FactRecord
 */
export interface TypedFactRecord<
  TNodeProps extends NodeProperties = NodeProperties,
  TEdgeProps extends EdgeProperties = EdgeProperties,
> extends FactRecord {
  subjectProperties?: TNodeProps;
  objectProperties?: TNodeProps;
  edgeProperties?: TEdgeProps;
}

/**
 * 条件类型：基于查询条件推断返回类型的辅助类型
 */
export type InferQueryResult<
  TNodeProps extends NodeProperties = NodeProperties,
  TEdgeProps extends EdgeProperties = EdgeProperties,
> = TypedFactRecord<TNodeProps, TEdgeProps>;

/**
 * 查询构建器的泛型化版本
 */
export interface TypedQueryBuilder<
  TNodeProps extends NodeProperties = NodeProperties,
  TEdgeProps extends EdgeProperties = EdgeProperties,
  TCriteria extends FactCriteria = FactCriteria,
> {
  /**
   * 设置查询锚点
   */
  anchor(orientation: FrontierOrientation): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria>;

  /**
   * 正向联想查询
   */
  follow(predicate: string): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria>;

  /**
   * 反向联想查询
   */
  followReverse(predicate: string): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria>;

  /**
   * 条件过滤（类型安全）
   */
  where(
    predicate: (record: TypedFactRecord<TNodeProps, TEdgeProps>) => boolean,
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria>;

  /**
   * 限制返回数量
   */
  limit(count: number): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria>;

  /**
   * 跳过指定数量
   */
  skip(count: number): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria>;

  /**
   * 获取所有结果
   */
  all(): TypedFactRecord<TNodeProps, TEdgeProps>[];

  /**
   * 转为数组（别名）
   */
  toArray(): TypedFactRecord<TNodeProps, TEdgeProps>[];

  /**
   * 异步迭代器支持
   */
  [Symbol.asyncIterator](): AsyncIterator<TypedFactRecord<TNodeProps, TEdgeProps>>;
}

/**
 * 属性过滤器的泛型化版本
 */
export interface TypedPropertyFilter<T = unknown> {
  propertyName: string;
  value?: T;
  range?: {
    min?: T;
    max?: T;
    includeMin?: boolean;
    includeMax?: boolean;
  };
}

/**
 * 泛型化的 NervusDB 主类接口
 */
export interface TypedNervusDB<
  TNodeProps extends NodeProperties = NodeProperties,
  TEdgeProps extends EdgeProperties = EdgeProperties,
> {
  /**
   * 添加带类型的 Fact
   */
  addFact(
    fact: TypedFactInput,
    options?: TypedFactOptions<TNodeProps, TEdgeProps>,
  ): TypedFactRecord<TNodeProps, TEdgeProps>;

  /**
   * 类型安全的查询
   */
  find<TCriteria extends FactCriteria>(
    criteria: TCriteria,
    options?: { anchor?: FrontierOrientation },
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria>;

  /**
   * 基于节点属性查询（类型安全）
   */
  findByNodeProperty<T>(
    propertyFilter: TypedPropertyFilter<T>,
    options?: { anchor?: FrontierOrientation },
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria>;

  /**
   * 基于边属性查询（类型安全）
   */
  findByEdgeProperty<T>(
    propertyFilter: TypedPropertyFilter<T>,
    options?: { anchor?: FrontierOrientation },
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria>;

  /**
   * 基于标签查询
   */
  findByLabel(
    labels: string | string[],
    options?: { mode?: 'AND' | 'OR'; anchor?: FrontierOrientation },
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria>;

  /**
   * 获取节点属性（类型安全）
   */
  getNodeProperties(nodeId: number): TNodeProps | null;

  /**
   * 获取边属性（类型安全）
   */
  getEdgeProperties(key: {
    subjectId: number;
    predicateId: number;
    objectId: number;
  }): TEdgeProps | null;

  /**
   * 设置节点属性（类型安全）
   */
  setNodeProperties(nodeId: number, properties: TNodeProps): void;

  /**
   * 设置边属性（类型安全）
   */
  setEdgeProperties(
    key: { subjectId: number; predicateId: number; objectId: number },
    properties: TEdgeProps,
  ): void;

  // 其他核心方法保持不变
  flush(): Promise<void>;
  close(): Promise<void>;
}

/**
 * 工厂函数：创建类型安全的 NervusDB 实例
 */
export type TypedNervusDBFactory = {
  /**
   * 打开类型化的数据库实例
   */
  open<
    TNodeProps extends NodeProperties = NodeProperties,
    TEdgeProps extends EdgeProperties = EdgeProperties,
  >(
    path: string,
    options?: unknown,
  ): Promise<TypedNervusDB<TNodeProps, TEdgeProps>>;
};

/**
 * 常见类型约束的预定义接口
 */

/**
 * 社交网络节点类型
 */
export interface PersonNode extends TypedNodeProperties {
  name: string;
  age?: number;
  email?: string;
  labels?: ('Person' | 'User')[];
}

/**
 * 社交关系边类型
 */
export interface RelationshipEdge extends EdgeProperties {
  since?: Date | number;
  strength?: number;
  type?: 'friend' | 'colleague' | 'family';
}

/**
 * 知识图谱实体节点
 */
export interface EntityNode extends TypedNodeProperties {
  type: string;
  title?: string;
  description?: string;
  confidence?: number;
  labels?: string[];
}

/**
 * 知识图谱关系边
 */
export interface KnowledgeEdge extends EdgeProperties {
  confidence: number;
  source?: string;
  timestamp?: number;
  weight?: number;
}

/**
 * 代码依赖节点
 */
export interface CodeNode extends TypedNodeProperties {
  path: string;
  type: 'file' | 'function' | 'class' | 'module';
  language?: string;
  size?: number;
  labels?: string[];
}

/**
 * 代码依赖关系
 */
export interface DependencyEdge extends EdgeProperties {
  type: 'imports' | 'calls' | 'extends' | 'implements';
  line?: number;
  column?: number;
}

/**
 * 类型安全的查询示例接口（供文档和测试使用）
 */
export interface QueryExamples {
  // 社交网络查询
  findFriends(
    db: TypedNervusDB<PersonNode, RelationshipEdge>,
    personName: string,
  ): TypedFactRecord<PersonNode, RelationshipEdge>[];

  // 知识图谱查询
  findRelatedEntities(
    db: TypedNervusDB<EntityNode, KnowledgeEdge>,
    entityType: string,
  ): TypedFactRecord<EntityNode, KnowledgeEdge>[];

  // 代码依赖查询
  findDependencies(
    db: TypedNervusDB<CodeNode, DependencyEdge>,
    filePath: string,
  ): TypedFactRecord<CodeNode, DependencyEdge>[];
}

/**
 * 判断输入是否符合 TypedPropertyFilter 的基本结构
 */
export function isTypedPropertyFilter(value: unknown): value is TypedPropertyFilter<unknown> {
  if (value === null || typeof value !== 'object') {
    return false;
  }

  const filter = value as Record<string, unknown>;
  if (typeof filter.propertyName !== 'string' || filter.propertyName.length === 0) {
    return false;
  }

  if ('range' in filter) {
    const range = filter.range;
    if (range !== undefined && (range === null || typeof range !== 'object')) {
      return false;
    }

    if (range !== undefined && range !== null) {
      const rangeRecord = range as Record<string, unknown>;

      if (!['min', 'max', 'includeMin', 'includeMax'].some((key) => key in rangeRecord)) {
        return false;
      }

      if ('includeMin' in rangeRecord && typeof rangeRecord.includeMin !== 'boolean') {
        return false;
      }

      if ('includeMax' in rangeRecord && typeof rangeRecord.includeMax !== 'boolean') {
        return false;
      }
    }
  }

  return true;
}

/**
 * 断言输入符合 TypedPropertyFilter 结构
 */
export function assertTypedPropertyFilter(
  value: unknown,
  message?: string,
): asserts value is TypedPropertyFilter<unknown> {
  if (!isTypedPropertyFilter(value)) {
    throw new TypeError(message ?? '属性过滤器结构错误');
  }
}
