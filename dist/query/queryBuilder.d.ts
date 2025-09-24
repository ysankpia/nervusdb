import { FactInput, FactRecord } from '../storage/persistentStore.js';
import { PersistentStore } from '../storage/persistentStore.js';
import { VariablePathBuilder } from './path/variable.js';
export type FactCriteria = Partial<FactInput>;
export type FrontierOrientation = 'subject' | 'object' | 'both';
export interface PropertyFilter {
    propertyName: string;
    value?: unknown;
    range?: {
        min?: unknown;
        max?: unknown;
        includeMin?: boolean;
        includeMax?: boolean;
    };
}
interface QueryContext {
    facts: FactRecord[];
    frontier: Set<number>;
    orientation: FrontierOrientation;
}
interface StreamingQueryContext {
    factsStream: AsyncIterableIterator<FactRecord>;
    frontier: Set<number>;
    orientation: FrontierOrientation;
}
export declare class QueryBuilder {
    private readonly store;
    private readonly facts;
    private readonly frontier;
    private readonly orientation;
    private readonly pinnedEpoch?;
    constructor(store: PersistentStore, context: QueryContext, pinnedEpoch?: number);
    variablePath(relation: string, options: {
        min?: number;
        max: number;
        uniqueness?: 'NODE' | 'EDGE' | 'NONE';
        direction?: 'forward' | 'reverse';
    }): VariablePathBuilder;
    get length(): number;
    slice(start?: number, end?: number): FactRecord[];
    [Symbol.iterator](): IterableIterator<FactRecord>;
    [Symbol.asyncIterator](): AsyncIterableIterator<FactRecord>;
    toArray(): FactRecord[];
    all(): FactRecord[];
    where(predicate: (record: FactRecord) => boolean): QueryBuilder;
    union(other: QueryBuilder): QueryBuilder;
    unionAll(other: QueryBuilder): QueryBuilder;
    /**
     * 基于节点标签过滤当前结果集
     * @param labels 单个标签或标签数组
     * @param options 过滤选项：匹配模式与过滤对象
     * - mode: AND(默认) | OR
     * - on: 过滤作用于 subject | object | both(默认)
     */
    whereLabel(labels: string | string[], options?: {
        mode?: 'AND' | 'OR';
        on?: 'subject' | 'object' | 'both';
    }): QueryBuilder;
    limit(n: number): QueryBuilder;
    take(n: number): QueryBuilder;
    skip(n: number): QueryBuilder;
    batch(size: number): AsyncIterableIterator<FactRecord[]>;
    /**
     * 属性索引下推查询 - 通用接口
     * @param propertyName 属性名
     * @param operator 操作符
     * @param value 值
     * @param target 查询目标（节点或边）
     */
    whereProperty(propertyName: string, operator: '=' | '>' | '<' | '>=' | '<=', value: unknown, target?: 'node' | 'edge'): QueryBuilder;
    /**
     * 根据操作符构建范围查询参数
     */
    private buildRangeFromOperator;
    /**
     * 根据节点属性过滤当前前沿
     * @param filter 属性过滤条件
     */
    whereNodeProperty(filter: PropertyFilter): QueryBuilder;
    /**
     * 根据边属性过滤当前事实
     * @param filter 属性过滤条件
     */
    whereEdgeProperty(filter: PropertyFilter): QueryBuilder;
    /**
     * 基于属性条件进行联想查询
     * @param predicate 关系谓词
     * @param nodePropertyFilter 可选的目标节点属性过滤条件
     */
    followWithNodeProperty(predicate: string, nodePropertyFilter?: PropertyFilter): QueryBuilder;
    /**
     * 基于属性条件进行反向联想查询
     * @param predicate 关系谓词
     * @param nodePropertyFilter 可选的目标节点属性过滤条件
     */
    followReverseWithNodeProperty(predicate: string, nodePropertyFilter?: PropertyFilter): QueryBuilder;
    /**
     * 带属性过滤的联想查询实现
     */
    private traverseWithProperty;
    anchor(orientation: FrontierOrientation): QueryBuilder;
    follow(predicate: string): QueryBuilder;
    followReverse(predicate: string): QueryBuilder;
    private traverse;
    /**
     * 变长路径查询：支持 [min..max] 跳数的同谓词遍历
     * 默认正向遍历，返回满足跳数范围的“最后一跳”三元组集合
     */
    followPath(predicate: string, range: {
        min?: number;
        max: number;
    }, options?: {
        direction?: 'forward' | 'reverse';
    }): QueryBuilder;
    static fromFindResult(store: PersistentStore, context: QueryContext, pinnedEpoch?: number): QueryBuilder;
    static empty(store: PersistentStore): QueryBuilder;
    private pin;
    private unpin;
}
/**
 * 流式查询构建器 - 真正的内存高效流式查询
 */
export declare class StreamingQueryBuilder {
    private readonly store;
    private readonly factsStream;
    private readonly frontier;
    private readonly orientation;
    private readonly pinnedEpoch?;
    constructor(store: PersistentStore, context: StreamingQueryContext, pinnedEpoch?: number);
    /**
     * 真正的流式异步迭代器 - 逐条处理，不预加载所有数据
     */
    [Symbol.asyncIterator](): AsyncIterator<FactRecord>;
    /**
     * 转换为普通 QueryBuilder（向后兼容）
     */
    toQueryBuilder(): Promise<QueryBuilder>;
    private pin;
    private unpin;
}
export declare function buildFindContext(store: PersistentStore, criteria: FactCriteria, anchor: FrontierOrientation): QueryContext;
/**
 * 构建流式查询上下文 - 真正的内存高效查询
 */
export declare function buildStreamingFindContext(store: PersistentStore, criteria: FactCriteria, anchor: FrontierOrientation): Promise<StreamingQueryContext>;
/**
 * 基于属性条件构建查询上下文
 * @param store 数据存储实例
 * @param propertyFilter 属性过滤条件
 * @param anchor 前沿方向
 * @param target 查询目标（节点或边）
 */
export declare function buildFindContextFromProperty(store: PersistentStore, propertyFilter: PropertyFilter, anchor: FrontierOrientation, target?: 'node' | 'edge'): QueryContext;
/**
 * 基于标签条件构建查询上下文
 * @param store 数据存储实例
 * @param labels 单个或多个标签
 * @param options 模式：AND/OR
 * @param anchor 前沿方向
 */
export declare function buildFindContextFromLabel(store: PersistentStore, labels: string | string[], options: {
    mode?: 'AND' | 'OR';
} | undefined, anchor: FrontierOrientation): QueryContext;
export {};
//# sourceMappingURL=queryBuilder.d.ts.map