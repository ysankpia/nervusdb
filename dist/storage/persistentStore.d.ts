import { TripleKey } from './propertyStore';
import { EncodedTriple } from './tripleStore';
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
    private tombstones;
    private hotness;
    private lock?;
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
    resolveRecords(triples: EncodedTriple[]): FactRecord[];
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
}
//# sourceMappingURL=persistentStore.d.ts.map