import { FactInput, FactRecord } from './storage/persistentStore.js';
import { TripleKey } from './storage/propertyStore.js';
import { FactCriteria, FrontierOrientation, QueryBuilder, StreamingQueryBuilder, PropertyFilter } from './query/queryBuilder.js';
import { SynapseDBOpenOptions, CommitBatchOptions, BeginBatchOptions } from './types/openOptions.js';
import { AggregationPipeline } from './query/aggregation.js';
import { PatternBuilder } from './query/pattern/match.js';
export interface FactOptions {
    subjectProperties?: Record<string, unknown>;
    objectProperties?: Record<string, unknown>;
    edgeProperties?: Record<string, unknown>;
}
/**
 * SynapseDB - 嵌入式三元组知识库
 *
 * 基于 TypeScript 实现的类 SQLite 单文件数据库，专门用于存储和查询 SPO 三元组数据。
 * 支持分页索引、WAL 事务、快照一致性、自动压缩和垃圾回收。
 *
 * @example
 * ```typescript
 * const db = await SynapseDB.open('/path/to/database.synapsedb', {
 *   pageSize: 2000,
 *   enableLock: true,
 *   compression: { codec: 'brotli', level: 6 }
 * });
 *
 * db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
 * await db.flush();
 *
 * const results = db.find({ predicate: 'knows' }).all();
 * await db.close();
 * ```
 */
export declare class SynapseDB {
    private readonly store;
    private constructor();
    /**
     * 打开或创建 SynapseDB 数据库
     *
     * @param path 数据库文件路径，如果不存在将自动创建
     * @param options 数据库配置选项
     * @returns Promise<SynapseDB> 数据库实例
     *
     * @example
     * ```typescript
     * // 基本用法
     * const db = await SynapseDB.open('./my-database.synapsedb');
     *
     * // 带配置的用法
     * const db = await SynapseDB.open('./my-database.synapsedb', {
     *   pageSize: 1500,
     *   enableLock: true,
     *   registerReader: true,
     *   compression: { codec: 'brotli', level: 4 }
     * });
     * ```
     *
     * @throws {Error} 当文件无法访问或锁定冲突时
     */
    static open(path: string, options?: SynapseDBOpenOptions): Promise<SynapseDB>;
    addFact(fact: FactInput, options?: FactOptions): FactRecord;
    listFacts(): FactRecord[];
    streamFacts(criteria?: Partial<{
        subject: string;
        predicate: string;
        object: string;
    }>, batchSize?: number): AsyncGenerator<FactRecord[], void, unknown>;
    findStream(criteria?: Partial<{
        subject: string;
        predicate: string;
        object: string;
    }>, options?: {
        batchSize?: number;
    }): AsyncIterable<FactRecord[]>;
    getNodeId(value: string): number | undefined;
    getNodeValue(id: number): string | undefined;
    getNodeProperties(nodeId: number): Record<string, unknown> | null;
    getEdgeProperties(key: TripleKey): Record<string, unknown> | null;
    flush(): Promise<void>;
    /**
     * 流式查询 - 真正内存高效的大数据集查询
     * @param criteria 查询条件
     * @param options 查询选项
     * @returns StreamingQueryBuilder 支持异步迭代，内存占用恒定
     * @example
     * ```typescript
     * // 流式处理大数据集，内存占用恒定
     * for await (const fact of db.findStreaming({ predicate: 'HAS_METHOD' })) {
     *   console.log(fact);
     * }
     * ```
     */
    findStreaming(criteria: FactCriteria, options?: {
        anchor?: FrontierOrientation;
    }): Promise<StreamingQueryBuilder>;
    find(criteria: FactCriteria, options?: {
        anchor?: FrontierOrientation;
    }): QueryBuilder;
    /**
     * 基于节点属性进行查询
     * @param propertyFilter 属性过滤条件
     * @param options 查询选项
     * @example
     * ```typescript
     * // 查找所有年龄为25的用户
     * const users = db.findByNodeProperty(
     *   { propertyName: 'age', value: 25 },
     *   { anchor: 'subject' }
     * ).all();
     *
     * // 查找年龄在25-35之间的用户
     * const adults = db.findByNodeProperty({
     *   propertyName: 'age',
     *   range: { min: 25, max: 35, includeMin: true, includeMax: true }
     * }).all();
     * ```
     */
    findByNodeProperty(propertyFilter: PropertyFilter, options?: {
        anchor?: FrontierOrientation;
    }): QueryBuilder;
    /**
     * 基于边属性进行查询
     * @param propertyFilter 属性过滤条件
     * @param options 查询选项
     * @example
     * ```typescript
     * // 查找所有权重为0.8的关系
     * const strongRelations = db.findByEdgeProperty(
     *   { propertyName: 'weight', value: 0.8 }
     * ).all();
     * ```
     */
    findByEdgeProperty(propertyFilter: PropertyFilter, options?: {
        anchor?: FrontierOrientation;
    }): QueryBuilder;
    /**
     * 基于节点标签进行查询
     * @param labels 单个或多个标签
     * @param options 查询选项：{ mode?: 'AND' | 'OR', anchor?: 'subject'|'object'|'both' }
     */
    findByLabel(labels: string | string[], options?: {
        mode?: 'AND' | 'OR';
        anchor?: FrontierOrientation;
    }): QueryBuilder;
    deleteFact(fact: FactInput): void;
    setNodeProperties(nodeId: number, properties: Record<string, unknown>): void;
    setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void;
    beginBatch(options?: BeginBatchOptions): void;
    commitBatch(options?: CommitBatchOptions): void;
    abortBatch(): void;
    close(): Promise<void>;
    withSnapshot<T>(fn: (db: SynapseDB) => Promise<T> | T): Promise<T>;
    getStagingMetrics(): {
        lsmMemtable: number;
    };
    aggregate(): AggregationPipeline;
    match(): PatternBuilder;
    shortestPath(from: string, to: string, options?: {
        predicates?: string[];
        maxHops?: number;
        direction?: 'forward' | 'reverse' | 'both';
    }): FactRecord[] | null;
    shortestPathBidirectional(from: string, to: string, options?: {
        predicates?: string[];
        maxHops?: number;
    }): FactRecord[] | null;
    shortestPathWeighted(from: string, to: string, options?: {
        predicate?: string;
        weightProperty?: string;
    }): FactRecord[] | null;
    cypher(query: string): Array<Record<string, unknown>>;
}
export type { FactInput, FactRecord, SynapseDBOpenOptions, CommitBatchOptions, BeginBatchOptions, PropertyFilter, FrontierOrientation, };
//# sourceMappingURL=synapseDb.d.ts.map