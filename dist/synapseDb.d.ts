import { FactInput, FactRecord } from './storage/persistentStore';
import { TripleKey } from './storage/propertyStore';
import { FactCriteria, FrontierOrientation, QueryBuilder } from './query/queryBuilder';
import { SynapseDBOpenOptions, CommitBatchOptions, BeginBatchOptions } from './types/openOptions';
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
    getNodeId(value: string): number | undefined;
    getNodeValue(id: number): string | undefined;
    getNodeProperties(nodeId: number): Record<string, unknown> | undefined;
    getEdgeProperties(key: TripleKey): Record<string, unknown> | undefined;
    flush(): Promise<void>;
    find(criteria: FactCriteria, options?: {
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
}
export type { FactInput, FactRecord, SynapseDBOpenOptions, CommitBatchOptions, BeginBatchOptions };
//# sourceMappingURL=synapseDb.d.ts.map