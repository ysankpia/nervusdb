import { type CompactStats, type IndexOrder } from './compaction';
export interface AutoCompactOptions {
    orders?: IndexOrder[];
    minMergePages?: number;
    tombstoneRatioThreshold?: number;
    pageSize?: number;
    compression?: {
        codec: 'none' | 'brotli';
        level?: number;
    };
    hotCompression?: {
        codec: 'none' | 'brotli';
        level?: number;
    };
    coldCompression?: {
        codec: 'none' | 'brotli';
        level?: number;
    };
    dryRun?: boolean;
    mode?: 'rewrite' | 'incremental';
    hotThreshold?: number;
    maxPrimariesPerOrder?: number;
    autoGC?: boolean;
    scoreWeights?: {
        hot?: number;
        pages?: number;
        tomb?: number;
    };
    minScore?: number;
    respectReaders?: boolean;
    includeLsmSegments?: boolean;
    includeLsmSegmentsAuto?: boolean;
    lsmSegmentsThreshold?: number;
    lsmTriplesThreshold?: number;
}
export interface AutoCompactDecision {
    selectedOrders: IndexOrder[];
    compact?: CompactStats;
    skipped?: boolean;
    reason?: string;
    readers?: number;
}
export declare function autoCompact(dbPath: string, options?: AutoCompactOptions): Promise<AutoCompactDecision>;
//# sourceMappingURL=autoCompact.d.ts.map