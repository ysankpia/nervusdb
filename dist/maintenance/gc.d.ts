export interface GCStats {
    orders: Array<{
        order: string;
        bytesBefore: number;
        bytesAfter: number;
        pages: number;
    }>;
    bytesBefore: number;
    bytesAfter: number;
    skipped?: boolean;
    reason?: string;
    readers?: number;
}
export declare function garbageCollectPages(dbPath: string, options?: {
    respectReaders?: boolean;
}): Promise<GCStats>;
//# sourceMappingURL=gc.d.ts.map