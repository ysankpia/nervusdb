/**
 * 类型安全的 SynapseDB 包装器实现
 * 在保持运行时兼容的同时，提供强类型的 TypeScript 接口
 */

import { SynapseDB } from './synapseDb.js';
import { QueryBuilder } from './query/queryBuilder.js';
import type {
  TypedSynapseDB,
  TypedQueryBuilder,
  TypedFactInput,
  TypedFactOptions,
  TypedFactRecord,
  TypedPropertyFilter,
  NodeProperties,
  EdgeProperties,
} from './types/enhanced.js';
import type {
  SynapseDBOpenOptions,
  FactCriteria,
  FrontierOrientation,
  PropertyFilter,
} from './index.js';

/**
 * 类型安全的查询构建器包装器
 */
class TypedQueryBuilderImpl<
  TNodeProps extends NodeProperties = NodeProperties,
  TEdgeProps extends EdgeProperties = EdgeProperties,
  TCriteria extends FactCriteria = FactCriteria,
> implements TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria>
{
  constructor(private readonly queryBuilder: QueryBuilder) {}

  anchor(orientation: FrontierOrientation): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria> {
    return new TypedQueryBuilderImpl(this.queryBuilder.anchor(orientation));
  }

  follow(predicate: string): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria> {
    return new TypedQueryBuilderImpl(this.queryBuilder.follow(predicate));
  }

  followReverse(predicate: string): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria> {
    return new TypedQueryBuilderImpl(this.queryBuilder.followReverse(predicate));
  }

  where(
    predicate: (record: TypedFactRecord<TNodeProps, TEdgeProps>) => boolean,
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria> {
    // 类型转换：运行时实际上仍使用原始的 FactRecord
    const untypedPredicate = predicate as (record: any) => boolean;
    return new TypedQueryBuilderImpl(this.queryBuilder.where(untypedPredicate));
  }

  limit(count: number): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria> {
    return new TypedQueryBuilderImpl(this.queryBuilder.limit(count));
  }

  skip(count: number): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria> {
    return new TypedQueryBuilderImpl(this.queryBuilder.skip(count));
  }

  all(): TypedFactRecord<TNodeProps, TEdgeProps>[] {
    return this.queryBuilder.all() as TypedFactRecord<TNodeProps, TEdgeProps>[];
  }

  toArray(): TypedFactRecord<TNodeProps, TEdgeProps>[] {
    return this.all();
  }

  async *[Symbol.asyncIterator](): AsyncIterator<TypedFactRecord<TNodeProps, TEdgeProps>> {
    for await (const record of this.queryBuilder) {
      yield record as TypedFactRecord<TNodeProps, TEdgeProps>;
    }
  }
}

/**
 * 类型安全的 SynapseDB 包装器实现
 */
class TypedSynapseDBImpl<
  TNodeProps extends NodeProperties = NodeProperties,
  TEdgeProps extends EdgeProperties = EdgeProperties,
> implements TypedSynapseDB<TNodeProps, TEdgeProps>
{
  constructor(private readonly db: SynapseDB) {}

  addFact(
    fact: TypedFactInput<TNodeProps, TEdgeProps>,
    options?: TypedFactOptions<TNodeProps, TEdgeProps>,
  ): TypedFactRecord<TNodeProps, TEdgeProps> {
    // 运行时转换为兼容格式
    const untypedOptions = options
      ? {
          subjectProperties: options.subjectProperties as Record<string, unknown>,
          objectProperties: options.objectProperties as Record<string, unknown>,
          edgeProperties: options.edgeProperties as Record<string, unknown>,
        }
      : undefined;

    const result = this.db.addFact(fact, untypedOptions);
    return result as TypedFactRecord<TNodeProps, TEdgeProps>;
  }

  find<TCriteria extends FactCriteria>(
    criteria: TCriteria,
    options?: { anchor?: FrontierOrientation },
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, TCriteria> {
    const queryBuilder = this.db.find(criteria, options);
    return new TypedQueryBuilderImpl(queryBuilder);
  }

  findByNodeProperty<T>(
    propertyFilter: TypedPropertyFilter<T>,
    options?: { anchor?: FrontierOrientation },
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria> {
    // 转换为兼容的 PropertyFilter 格式
    const untypedFilter: PropertyFilter = {
      propertyName: propertyFilter.propertyName,
      ...(propertyFilter.value !== undefined && { value: propertyFilter.value }),
      ...(propertyFilter.range && { range: propertyFilter.range }),
    };

    const queryBuilder = this.db.findByNodeProperty(untypedFilter, options);
    return new TypedQueryBuilderImpl(queryBuilder);
  }

  findByEdgeProperty<T>(
    propertyFilter: TypedPropertyFilter<T>,
    options?: { anchor?: FrontierOrientation },
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria> {
    const untypedFilter: PropertyFilter = {
      propertyName: propertyFilter.propertyName,
      ...(propertyFilter.value !== undefined && { value: propertyFilter.value }),
      ...(propertyFilter.range && { range: propertyFilter.range }),
    };

    const queryBuilder = this.db.findByEdgeProperty(untypedFilter, options);
    return new TypedQueryBuilderImpl(queryBuilder);
  }

  findByLabel(
    labels: string | string[],
    options?: { mode?: 'AND' | 'OR'; anchor?: FrontierOrientation },
  ): TypedQueryBuilder<TNodeProps, TEdgeProps, FactCriteria> {
    const queryBuilder = this.db.findByLabel(labels, options);
    return new TypedQueryBuilderImpl(queryBuilder);
  }

  getNodeProperties(nodeId: number): TNodeProps | null {
    const result = this.db.getNodeProperties(nodeId);
    return result as TNodeProps | null;
  }

  getEdgeProperties(key: {
    subjectId: number;
    predicateId: number;
    objectId: number;
  }): TEdgeProps | null {
    const result = this.db.getEdgeProperties(key);
    return result as TEdgeProps | null;
  }

  setNodeProperties(nodeId: number, properties: TNodeProps): void {
    this.db.setNodeProperties(nodeId, properties as Record<string, unknown>);
  }

  setEdgeProperties(
    key: { subjectId: number; predicateId: number; objectId: number },
    properties: TEdgeProps,
  ): void {
    this.db.setEdgeProperties(key, properties as Record<string, unknown>);
  }

  // 委托其他方法到原始实例
  async flush(): Promise<void> {
    return this.db.flush();
  }

  async close(): Promise<void> {
    return this.db.close();
  }

  // 提供对原始实例的访问（用于高级操作）
  get raw(): SynapseDB {
    return this.db;
  }
}

/**
 * 类型安全的 SynapseDB 工厂
 */
export const TypedSynapseDBFactory = {
  /**
   * 打开类型化的 SynapseDB 实例
   *
   * @example
   * ```typescript
   * // 使用预定义类型
   * const socialDb = await TypedSynapseDB.open<PersonNode, RelationshipEdge>('./social.db');
   *
   * // 使用自定义类型
   * interface MyNode { name: string; score: number; }
   * interface MyEdge { weight: number; }
   * const customDb = await TypedSynapseDB.open<MyNode, MyEdge>('./custom.db');
   * ```
   */
  async open<
    TNodeProps extends NodeProperties = NodeProperties,
    TEdgeProps extends EdgeProperties = EdgeProperties,
  >(path: string, options?: SynapseDBOpenOptions): Promise<TypedSynapseDB<TNodeProps, TEdgeProps>> {
    const db = await SynapseDB.open(path, options);
    return new TypedSynapseDBImpl<TNodeProps, TEdgeProps>(db);
  },

  /**
   * 从现有的 SynapseDB 实例创建类型化包装器
   *
   * @example
   * ```typescript
   * const rawDb = await SynapseDB.open('./existing.db');
   * const typedDb = TypedSynapseDB.wrap<PersonNode, RelationshipEdge>(rawDb);
   * ```
   */
  wrap<
    TNodeProps extends NodeProperties = NodeProperties,
    TEdgeProps extends EdgeProperties = EdgeProperties,
  >(db: SynapseDB): TypedSynapseDB<TNodeProps, TEdgeProps> {
    return new TypedSynapseDBImpl<TNodeProps, TEdgeProps>(db);
  },
};

/**
 * 类型安全的查询辅助函数
 */
export const TypeSafeQueries = {
  /**
   * 创建类型安全的属性过滤器
   */
  propertyFilter<T>(
    propertyName: string,
    value?: T,
    range?: { min?: T; max?: T; includeMin?: boolean; includeMax?: boolean },
  ): TypedPropertyFilter<T> {
    return { propertyName, value, range };
  },

  /**
   * 类型安全的范围查询过滤器
   */
  rangeFilter<T extends number | string | Date>(
    propertyName: string,
    min?: T,
    max?: T,
    options?: { includeMin?: boolean; includeMax?: boolean },
  ): TypedPropertyFilter<T> {
    return {
      propertyName,
      range: {
        min,
        max,
        includeMin: options?.includeMin ?? true,
        includeMax: options?.includeMax ?? true,
      },
    };
  },
};

// 导出所有类型和实现
export * from './types/enhanced.js';
export { TypedQueryBuilderImpl };

/**
 * 便捷类型别名
 */
export type TypedDB<
  TNode extends NodeProperties = NodeProperties,
  TEdge extends EdgeProperties = EdgeProperties,
> = TypedSynapseDB<TNode, TEdge>;

export type TypedQuery<
  TNode extends NodeProperties = NodeProperties,
  TEdge extends EdgeProperties = EdgeProperties,
> = TypedQueryBuilder<TNode, TEdge>;

export type TypedRecord<
  TNode extends NodeProperties = NodeProperties,
  TEdge extends EdgeProperties = EdgeProperties,
> = TypedFactRecord<TNode, TEdge>;
