export interface GCStats {
    orders: Array<{
        order: string;
        bytesBefore: number;
        bytesAfter: number;
        pages: number;
    }>;
    bytesBefore: number;
    bytesAfter: number;
}
export declare function garbageCollectPages(dbPath: string): Promise<GCStats>;
//# sourceMappingURL=gc.d.ts.map