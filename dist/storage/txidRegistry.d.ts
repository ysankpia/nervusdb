export interface TxIdEntry {
    id: string;
    ts: number;
    sessionId?: string;
}
export interface TxIdRegistryData {
    version: number;
    txIds: TxIdEntry[];
    max?: number;
}
export declare function readTxIdRegistry(directory: string): Promise<TxIdRegistryData>;
export declare function writeTxIdRegistry(directory: string, data: TxIdRegistryData): Promise<void>;
export declare function toSet(reg: TxIdRegistryData): Set<string>;
export declare function mergeTxIds(reg: TxIdRegistryData, items: Array<{
    id: string;
    ts?: number;
    sessionId?: string;
}>, max: number | undefined): TxIdRegistryData;
//# sourceMappingURL=txidRegistry.d.ts.map