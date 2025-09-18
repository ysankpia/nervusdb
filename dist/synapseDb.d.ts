import { FactInput, FactRecord } from './storage/persistentStore';
import { TripleKey } from './storage/propertyStore';
import { FactCriteria, FrontierOrientation, QueryBuilder } from './query/queryBuilder';
export interface FactOptions {
    subjectProperties?: Record<string, unknown>;
    objectProperties?: Record<string, unknown>;
    edgeProperties?: Record<string, unknown>;
}
export declare class SynapseDB {
    private readonly store;
    private constructor();
    static open(path: string, options?: {
        indexDirectory?: string;
        pageSize?: number;
        rebuildIndexes?: boolean;
        compression?: {
            codec: 'none' | 'brotli';
            level?: number;
        };
    }): Promise<SynapseDB>;
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
    beginBatch(): void;
    commitBatch(): void;
    abortBatch(): void;
    close(): Promise<void>;
}
export type { FactInput, FactRecord };
//# sourceMappingURL=synapseDb.d.ts.map