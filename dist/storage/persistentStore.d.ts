import { TripleKey } from './propertyStore.js';
import { EncodedTriple } from './tripleStore.js';
export interface FactInput {
    subject: string;
    predicate: string;
    object: string;
}
export interface PersistedFact extends FactInput {
    subjectId: number;
    predicateId: number;
    objectId: number;
}
export interface FactRecord extends PersistedFact {
    subjectProperties?: Record<string, unknown>;
    objectProperties?: Record<string, unknown>;
    edgeProperties?: Record<string, unknown>;
}
export interface PersistentStoreOptions {
    indexDirectory?: string;
    pageSize?: number;
    rebuildIndexes?: boolean;
    compression?: {
        codec: 'none' | 'brotli';
        level?: number;
    };
    enableLock?: boolean;
    registerReader?: boolean;
    enablePersistentTxDedupe?: boolean;
    maxRememberTxIds?: number;
    stagingMode?: 'default' | 'lsm-lite';
}
export declare class PersistentStore {
    private readonly path;
    private readonly dictionary;
    private readonly triples;
    private readonly properties;
    private readonly indexes;
    private readonly indexDirectory;
    private constructor();
    private dirty;
    private wal;
    private closed;
    private tombstones;
    private hotness;
    private lock?;
    private propertyIndexManager;
    private labelManager;
    private batchDepth;
    private batchMetaStack;
    private txStack;
    private currentEpoch;
    private lastManifestCheck;
    private pinnedEpochStack;
    private readerRegistered;
    private snapshotRefCount;
    private activeReaderOperation;
    private lsm?;
    static open(path: string, options?: PersistentStoreOptions): Promise<PersistentStore>;
    private pagedReaders;
    private hydratePagedReaders;
    private buildPagedIndexes;
    private appendPagedIndexesFromStaging;
    addFact(fact: FactInput): PersistedFact;
    private addFactDirect;
    listFacts(): FactRecord[];
    streamQuery(criteria: Partial<EncodedTriple>, batchSize?: number): AsyncGenerator<EncodedTriple[], void, unknown>;
    streamFactRecords(criteria?: Partial<EncodedTriple>, batchSize?: number): AsyncGenerator<FactRecord[], void, unknown>;
    getDictionarySize(): number;
    getNodeIdByValue(value: string): number | undefined;
    getNodeValueById(id: number): string | undefined;
    deleteFact(fact: FactInput): void;
    private deleteFactDirect;
    setNodeProperties(nodeId: number, properties: Record<string, unknown>): void;
    setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void;
    beginBatch(options?: {
        txId?: string;
        sessionId?: string;
    }): void;
    commitBatch(options?: {
        durable?: boolean;
    }): void;
    abortBatch(): void;
    private setNodePropertiesDirect;
    private setEdgePropertiesDirect;
    getNodeProperties(nodeId: number): Record<string, unknown> | undefined;
    getEdgeProperties(key: TripleKey): Record<string, unknown> | undefined;
    query(criteria: Partial<EncodedTriple>): EncodedTriple[];
    /**
     * 纯磁盘查询方法：仅依赖分页索引，不使用内存缓存
     * 用于快照查询期间，确保内存占用最小化
     */
    private queryFromDisk;
    /**
     * 流式查询：避免一次性加载所有数据到内存
     */
    queryStreaming(criteria: Partial<EncodedTriple>): AsyncIterableIterator<EncodedTriple>;
    /**
     * 流式查询所有数据：避免一次性加载到内存
     */
    private queryAllStreaming;
    /**
     * 快照模式下的流式查询 - 真正的流式实现
     */
    private queryFromDiskStreaming;
    resolveRecords(triples: EncodedTriple[], options?: {
        includeProperties?: boolean;
    }): FactRecord[];
    private toFactRecord;
    flush(): Promise<void>;
    private flushLsmSegments;
    private static CRC32_TABLE;
    private crc32;
    private refreshReadersIfEpochAdvanced;
    private ensureReaderRegistered;
    pushPinnedEpoch(epoch: number): Promise<void>;
    popPinnedEpoch(): Promise<void>;
    getCurrentEpoch(): number;
    getStagingMetrics(): {
        lsmMemtable: number;
    };
    close(): Promise<void>;
    private bumpHot;
    private stageAdd;
    private applyStage;
    private peekTx;
    /**
     * 重建属性索引
     */
    private rebuildPropertyIndex;
    /**
     * 获取属性索引管理器的内存索引
     */
    getPropertyIndex(): import("./propertyIndex.js").MemoryPropertyIndex;
    /**
     * 获取标签管理器的内存索引
     */
    getLabelIndex(): import("../graph/labels.js").MemoryLabelIndex;
    /**
     * 应用属性变更到索引
     */
    private applyPropertyIndexChange;
    /**
     * 更新节点标签索引
     */
    private updateNodeLabelIndex;
    /**
     * 更新节点属性索引
     */
    private updateNodePropertyIndex;
    /**
     * 更新边属性索引
     */
    private updateEdgePropertyIndex;
}
//# sourceMappingURL=persistentStore.d.ts.map