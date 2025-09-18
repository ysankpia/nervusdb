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
    beginBatch(): void;
    commitBatch(): void;
    abortBatch(): void;
    private setNodePropertiesDirect;
    private setEdgePropertiesDirect;
    getNodeProperties(nodeId: number): Record<string, unknown> | undefined;
    getEdgeProperties(key: TripleKey): Record<string, unknown> | undefined;
    query(criteria: Partial<EncodedTriple>): EncodedTriple[];
    resolveRecords(triples: EncodedTriple[]): FactRecord[];
    private toFactRecord;
    flush(): Promise<void>;
    close(): Promise<void>;
    private bumpHot;
}
//# sourceMappingURL=persistentStore.d.ts.map