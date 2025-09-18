export interface PageError {
    order: string;
    primaryValue: number;
    offset: number;
    length: number;
    expectedCrc?: number;
    actualCrc?: number;
    reason: string;
}
export interface StrictCheckResult {
    ok: boolean;
    errors: PageError[];
}
export declare function checkStrict(dbPath: string): Promise<StrictCheckResult>;
//# sourceMappingURL=check.d.ts.map