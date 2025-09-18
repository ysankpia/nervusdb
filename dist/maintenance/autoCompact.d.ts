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
}
export interface AutoCompactDecision {
    selectedOrders: IndexOrder[];
    compact?: CompactStats;
}
export declare function autoCompact(dbPath: string, options?: AutoCompactOptions): Promise<AutoCompactDecision>;
//# sourceMappingURL=autoCompact.d.ts.map