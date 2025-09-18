export type IndexOrder = 'SPO' | 'SOP' | 'POS' | 'PSO' | 'OSP' | 'OPS';
export interface CompactOptions {
    pageSize?: number;
    orders?: IndexOrder[];
    minMergePages?: number;
    tombstoneRatioThreshold?: number;
    dryRun?: boolean;
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
    mode?: 'rewrite' | 'incremental';
    onlyPrimaries?: Partial<Record<IndexOrder, number[]>>;
}
export interface CompactStats {
    ordersRewritten: IndexOrder[];
    pagesBefore: number;
    pagesAfter: number;
    primariesMerged: number;
    removedByTombstones: number;
}
export declare function compactDatabase(dbPath: string, options?: CompactOptions): Promise<CompactStats>;
//# sourceMappingURL=compaction.d.ts.map