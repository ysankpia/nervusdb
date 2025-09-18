export type WalRecordType = 0x10 | 0x20 | 0x30 | 0x31 | 0x40 | 0x41 | 0x42;
export interface FactInput {
    subject: string;
    predicate: string;
    object: string;
}
export declare class WalWriter {
    private readonly walPath;
    private fd;
    private offset;
    private constructor();
    static open(dbPath: string): Promise<WalWriter>;
    appendAddTriple(fact: FactInput): Promise<void>;
    appendDeleteTriple(fact: FactInput): Promise<void>;
    appendSetNodeProps(nodeId: number, props: unknown): Promise<void>;
    appendSetEdgeProps(ids: {
        subjectId: number;
        predicateId: number;
        objectId: number;
    }, props: unknown): Promise<void>;
    appendBegin(): Promise<void>;
    appendCommit(): Promise<void>;
    appendAbort(): Promise<void>;
    reset(): Promise<void>;
    truncateTo(offset: number): Promise<void>;
    close(): Promise<void>;
    private writeRecordSync;
}
export declare class WalReplayer {
    private readonly dbPath;
    constructor(dbPath: string);
    replay(): Promise<{
        addFacts: FactInput[];
        deleteFacts: FactInput[];
        nodeProps: Array<{
            nodeId: number;
            value: unknown;
        }>;
        edgeProps: Array<{
            ids: {
                subjectId: number;
                predicateId: number;
                objectId: number;
            };
            value: unknown;
        }>;
        safeOffset: number;
        version: number;
    }>;
}
//# sourceMappingURL=wal.d.ts.map