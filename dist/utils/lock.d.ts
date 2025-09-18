export interface LockHandle {
    release(): Promise<void>;
}
export declare function acquireLock(basePath: string): Promise<LockHandle>;
//# sourceMappingURL=lock.d.ts.map