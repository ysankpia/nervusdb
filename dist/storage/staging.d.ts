export interface StagingStrategy<T> {
    add(rec: T): void;
    size(): number;
}
export type StagingMode = 'default' | 'lsm-lite';
export declare class LsmLiteStaging<T> implements StagingStrategy<T> {
    private memtable;
    add(rec: T): void;
    size(): number;
    drain(): T[];
}
//# sourceMappingURL=staging.d.ts.map